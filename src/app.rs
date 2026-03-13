use crate::affinity::emotion::ModuleEmotion;
use crate::audio::gain_staging;
use crate::audio::mixer_state::MixerState;
use crate::audio::voice_bus::{BusMeterReport, ChannelStrip};
use crate::module::{ModuleId, Signal};
use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::key::SolidoKey;
use crate::modules::audio_analysis::AudioAnalysisModule;
use crate::modules::quantizer::QuantizerModule;
use crate::modules::raga_module::RagaModule;
use crate::modules::scale_module::ScaleModule;
use crate::modules::tala_module::TalaModule;
use crate::organism::dna::OrganismDna;
use crate::organism::module::OrganismModule;
use crate::organism::registry::OrganismRegistry;
use crate::reactor::SeedReactor;
use crate::recorder::Recorder;
use crate::renderer::biofield_renderer::{self, BioFieldRenderResources, BioFieldUniforms};
use crate::renderer::font_atlas::FontAtlas;
use crate::renderer::organism_renderer;
use crate::renderer::shape_atlas::ShapeAtlas;
use crate::audio::reverb_bus::ReverbBusHandles;
use crate::audio::tape_delay_bus::TapeDelayBusHandles;
use crate::substrate::audio::{AudioSubstrate, SpawnPayload};
use crate::substrate::channel::{self, Receiver};
use crate::tuning::gravity_control::GravityState;
use crate::tuning::gravity_well::{
    pitch_class_name, consonance_weight, GravityField, WellEnergy,
    WellInfluence, WellProximity,
    LJ_GRAVITY, LJ_SOFTENING, LJ_TRENCH_FRACTION,
    MAX_WELL_FORCE, BEAT_PULSE_AMPLITUDE, OCTAVE_THRESHOLD,
    NAV_WEIGHT,
};
use crate::ui::panels::controls::ControlPanelIds;
use crate::dsp::cell::{cell_type_ranges, find_range};
use crate::ui::panels::effects_panel::{EffectsBypassState, ReverbBusUiState, TapeDelayBusUiState};
use crate::ui::panels::organism_panel::{CellUiState, KillAction, OrganismPanelState, OrganismUiState};
use crate::ui::panels::presets::{PresetAction, PresetPanelState};
use crate::ui::panels::spawn_panel::{show_spawn_panel, SpawnAction};
use crate::ui::panels;
use crate::ui::{self, DebugModuleIds, WorkspaceState};

const FONT_JSON: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL-msdf.json");
const FONT_PNG: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL.png");
const SHAPE_PNG: &[u8] = include_bytes!("../assets/elements/cvx-corner.png");

/// Per-organism dispatch data for well force computation.
#[derive(Clone)]
struct WellDispatchEntry {
    mod_id: ModuleId,
    org_id: u32,
    pos: [f32; 2],
    scale_affinity: f32,
    fidelity: f32,
    spectral_centroid: f32,
    org_root: u8,
    lj_gravity_scale: f32,
    beat_pulse_sensitivity: f32,
    max_speed: f32,
}

/// Fixed physics timestep: 120Hz (8.33ms per substep).
const PHYS_DT: f32 = 1.0 / 120.0;
/// Maximum accumulator cap (100ms = 12 substeps). Prevents spiral-of-death
/// when a frame takes too long — physics just slows down instead.
const PHYS_MAX_ACCUM: f32 = 0.1;

pub struct SolidoApp {
    last_frame_time: Option<f64>,
    start_time: f64,
    recorder: Recorder,
    render_state: Option<egui_wgpu::RenderState>,
    reactor: SeedReactor,
    workspace: WorkspaceState,
    kbd_id: ModuleId,
    analysis_id: ModuleId,
    quantizer_id: ModuleId,
    tala_id: ModuleId,
    raga_id: ModuleId,
    scale_id: ModuleId,
    audio: Option<AudioSubstrate>,
    // S05: VoiceBus mixer state + meter receiver
    mixer_state: Option<MixerState>,
    meter_rx: Option<Receiver<BusMeterReport>>,
    // Organism panel: per-cell bypass + param handles for UI
    organism_panel: Option<OrganismPanelState>,
    // Reverb bus handles for UI
    _reverb_bus_handles: Option<ReverbBusHandles>,
    // Tape delay bus handles (kept alive; shared handles cloned into organism_panel)
    tape_delay_bus_handles: Option<TapeDelayBusHandles>,
    // S09: Organism simulation + blob rendering
    organism_registry: OrganismRegistry,
    gravity_state: GravityState,
    /// Aggregate emotion for gravity state (averaged across modules).
    aggregate_emotion: ModuleEmotion,
    /// Previous frame's BPM value — used to detect changes and propagate to organisms.
    prev_bpm: f32,
    beat_phase: f32,
    /// When true, gravity sliders are manual; when false, emotion-driven.
    manual_gravity: bool,
    /// Preset panel state.
    preset_panel: PresetPanelState,
    /// All DNA presets loaded at startup (both active and inactive) for the spawn panel.
    available_dna: Vec<OrganismDna>,
    /// Next audio-thread organism index (incremented on each spawn).
    next_audio_idx: usize,
    /// Spatial harmonic gravity wells.
    gravity_field: GravityField,
    /// Cached base weights from ScaleModule (updated each frame).
    cached_base_weights: [f32; 12],
    /// Well being dragged (by well id).
    dragging_well: Option<u32>,
    /// Well being scaled (by well id).
    scaling_well: Option<u32>,
    /// Pre-allocated buffer for gravity well dispatch (avoids per-frame Vec allocation).
    well_dispatch_buf: Vec<WellDispatchEntry>,
    /// Per-well energy state (parallel to gravity_field.wells()).
    well_energy: Vec<WellEnergy>,
    /// Per-organism well proximity (parallel to well_dispatch_buf).
    well_proximity_buf: Vec<WellProximity>,
    /// Global base key: 0=C, 1=C#, ... 11=B. Environment owns the key.
    base_key: u8,
    /// Show gravity well overlays on canvas.
    show_well_overlays: bool,
    /// Show organism hover tags on canvas.
    show_hover_tags: bool,
    /// Previous frame's gravity bypass state — used to detect transitions.
    prev_gravity_bypassed: bool,
    /// Per-effect bypass state.
    effects_bypass: EffectsBypassState,
    /// Reverb bus UI state (global, separated from organism panel).
    reverb_bus_ui: Option<ReverbBusUiState>,
    /// Tape delay bus UI state (global, separated from organism panel).
    tape_delay_bus_ui: Option<TapeDelayBusUiState>,
    /// Fixed-timestep physics accumulator (carries remainder between frames).
    phys_accumulator: f32,
    /// S39: Frame counter for navigation event detection.
    nav_frame_tick: u64,
}

/// Deterministic spawn position derived from DNA seed.
fn seeded_spawn_pos(seed: u64, viewport: [f32; 4], margin: f32) -> [f32; 2] {
    let h1 = seed.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(0x6c62272e07bb0142);
    let h2 = h1.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(0x6c62272e07bb0142);
    let nx = (h1 >> 16 & 0xFFFF) as f32 / 65535.0;
    let ny = (h2 >> 16 & 0xFFFF) as f32 / 65535.0;
    let [min_x, min_y, max_x, max_y] = viewport;
    [
        min_x + margin + nx * (max_x - min_x - 2.0 * margin),
        min_y + margin + ny * (max_y - min_y - 2.0 * margin),
    ]
}

/// Small initial velocity derived from DNA seed (20% of max_speed).
fn seeded_spawn_vel(seed: u64, max_speed: f32) -> [f32; 2] {
    let h1 = seed.wrapping_mul(0xbf58476d1ce4e5b9).wrapping_add(0x94d049bb133111eb);
    let h2 = h1.wrapping_mul(0xbf58476d1ce4e5b9).wrapping_add(0x94d049bb133111eb);
    let vx = (h1 >> 16 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0;
    let vy = (h2 >> 16 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0;
    [vx * max_speed * 0.5, vy * max_speed * 0.5]
}

impl SolidoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register Phosphor icon font
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let font_atlas = FontAtlas::load_msdf(FONT_JSON, FONT_PNG);
        let shape_atlas = ShapeAtlas::load_png(SHAPE_PNG);

        let render_state = cc.wgpu_render_state.clone();
        if let Some(rs) = render_state.as_ref() {
            organism_renderer::init_resources(rs, &font_atlas, &shape_atlas);
            biofield_renderer::init_resources(rs);
        }

        let mut reactor = SeedReactor::new();
        let kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let analysis_id = reactor.register(Box::new(AudioAnalysisModule::new()));
        let raga_id = reactor.register(Box::new(RagaModule::new()));
        let quantizer_id = reactor.register(Box::new(QuantizerModule::new()));
        let scale_id = reactor.register(Box::new(ScaleModule::new()));
        let tala_id = reactor.register(Box::new(TalaModule::new().with_clock(reactor.clock.clone())));

        // Load organism DNA presets
        let dna_paths = [
            "assets/dna/dron-alpha.json",
            "assets/dna/hoso-malabar.json",
            "assets/dna/spgl-kepler.json",
            "assets/dna/acid-kinoko.json",
            "assets/dna/tblk-dha.json",
            "assets/dna/kkit-909.json",
            "assets/dna/isao-tomita.json",
        ];
        // All loaded DNAs (active and inactive) — used to populate the spawn panel.
        let available_dna: Vec<OrganismDna> = dna_paths
            .iter()
            .filter_map(|p| {
                crate::organism::dna_io::load(std::path::Path::new(p))
                    .map_err(|e| log::warn!("Failed to load DNA {p}: {e}"))
                    .ok()
            })
            .collect();
        // Only active organisms are started at launch.
        let dna_list: Vec<OrganismDna> = available_dna.iter().filter(|d| d.active).cloned().collect();

        // Audio substrate + OrganismDsp
        // Organisms are built inside AudioSubstrate at the discovered sample rate.
        let mut organism_registry = OrganismRegistry::new();
        organism_registry.world_bounds = [0.0, 0.0, 1200.0, 700.0];

        let (audio, mixer_state, meter_rx, organism_panel, reverb_bus_handles, tape_delay_bus_handles, reverb_bus_ui, tape_delay_bus_ui) = match AudioSubstrate::new(&dna_list, reactor.clock.playing.clone()) {
            Some((substrate, org_endpoints, bus_handles, reverb_handles, tape_delay_handles, meter_rx)) => {
                // S13: Register OrganismModules with reactor + spawn visual state

                // Step 1: Clone cell bypass + param Shared handles from &org_endpoints
                // (borrow pass — endpoints not consumed yet).
                let mut panel_cells: Vec<Vec<CellUiState>> = Vec::new();
                for (dna, endpoint) in dna_list.iter().zip(&org_endpoints) {
                    let cells: Vec<CellUiState> = dna.cells.iter().enumerate().map(|(ci, cell_dna)| {
                        let bypass = endpoint.shared_handles
                            .get(&format!("cell{}.bypass", ci))
                            .cloned()
                            .unwrap_or_else(|| crate::dsp::shared::shared(0.0));

                        // Collect all non-bypass params for this cell
                        let prefix = format!("cell{}.", ci);
                        let mut params: Vec<(String, crate::dsp::shared::Shared)> = endpoint
                            .shared_handles
                            .iter()
                            .filter(|(k, _)| k.starts_with(&prefix) && !k.ends_with(".bypass"))
                            .map(|(k, v)| {
                                let param_name = k.strip_prefix(&prefix).unwrap().to_string();
                                (param_name, v.clone())
                            })
                            .collect();
                        params.sort_by(|a, b| a.0.cmp(&b.0));

                        // Look up param ranges from the cell type's PARAM_RANGES constant
                        let ranges = cell_type_ranges(&cell_dna.cell_type);
                        let param_ranges: Vec<(String, f32, f32)> = params
                            .iter()
                            .map(|(name, _)| {
                                let (min, max) = find_range(ranges, name)
                                    .unwrap_or((0.0, 1.0));
                                (name.clone(), min, max)
                            })
                            .collect();

                        CellUiState {
                            cell_type: cell_dna.cell_type.clone(),
                            bypass,
                            params,
                            param_ranges,
                        }
                    }).collect();
                    panel_cells.push(cells);
                }

                // Step 2: Spawn organisms, consume endpoints, collect org_ids + mod_ids.
                let mut org_ids: Vec<u32> = Vec::new();
                let mut mod_ids: Vec<ModuleId> = Vec::new();
                for (_i, (dna, endpoint)) in dna_list.iter().zip(org_endpoints).enumerate() {
                    let pos = seeded_spawn_pos(dna.seed, [0.0, 0.0, 1200.0, 700.0], 100.0);
                    let vel = seeded_spawn_vel(dna.seed, dna.physics.max_speed);

                    // Spawn OrganismState in registry from DNA params
                    let org_id = organism_registry.spawn(
                        pos,
                        dna.body.lobe_count,
                        dna.body.core_radius,
                    );
                    org_ids.push(org_id);
                    if let Some(org) = organism_registry.get_mut(org_id) {
                        org.base_hue = dna.render.hue;
                        org.smin_k = dna.render.smin_k;
                        org.edge_softness = dna.render.edge_softness;
                        org.base_glow = dna.render.glow;
                        org.pulse_response = dna.render.pulse_response;
                        org.drag = dna.physics.drag;
                        org.max_speed = dna.physics.max_speed;
                        org.mass = dna.physics.mass;
                        org.viscosity = dna.physics.viscosity;
                        org.pseudopod_gain = dna.body.pseudopod_gain;
                        org.extension_speed = dna.body.extension_speed;
                        org.retraction_speed = dna.body.retraction_speed;
                        org.shape_amplitude = dna.body.shape_amplitude;
                        org.shape_frequency = dna.body.shape_frequency;
                        org.rd_reactivity = dna.body.rd_reactivity;
                        org.rd_feed = dna.body.rd_feed;
                        org.rd_kill = dna.body.rd_kill;
                        org.rd_scale = dna.body.rd_scale;
                        org.harmonic_count = dna.body.harmonic_count;
                        org.harmonic_amp = dna.body.harmonic_amp;
                        org.elongation = dna.body.elongation;
                        org.chladni_m = dna.body.chladni_m;
                        org.chladni_n = dna.body.chladni_n;
                        org.velocity = vel;
                        let spd = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
                        org.visual_dir = if spd > 1.0 { [vel[0] / spd, vel[1] / spd] } else { [1.0, 0.0] };
                        org.smooth_speed = spd;
                        org.prev_audio_energy = 0.0;
                        org.scale_affinity = dna.scale_affinity;
                        org.root_pitch_class = dna.root_pitch_class;
                        org.energy = 0.7;
                        org.arousal = dna.emotion.base_arousal;
                        org.valence = dna.emotion.base_valence;
                        org.desire_to_connect = dna.emotion.desire_to_connect;
                        org.species = dna.species.clone();
                        org.interaction_rules = dna.physics.interaction_rules.clone();
                        org.reverb_send_base = dna.sends.as_ref()
                            .and_then(|s| s.reverb.as_ref())
                            .map(|r| r.send)
                            .unwrap_or(0.0);
                        org.tape_delay_send_base = dna.sends.as_ref()
                            .and_then(|s| s.tape_delay.as_ref())
                            .map(|td| td.send)
                            .unwrap_or(0.0);
                    }

                    // Register OrganismModule with reactor (Organism tier → AffinityGraph)
                    let module = OrganismModule::new(
                        dna.clone(),
                        endpoint.shared_handles,
                        endpoint.analysis_rx,
                        endpoint.cmd_tx,
                        org_id,
                    );
                    let mod_id = reactor.register(Box::new(module));
                    mod_ids.push(mod_id);
                    eprintln!(
                        "  organism: {} (species={}, org_id={}, mod_id={})",
                        dna.name, dna.species, org_id, mod_id
                    );
                }

                // Step 3: Build OrganismPanelState merging cell handles + org_ids
                // + bus_handles strip data + DNA identity fields.
                // Clone mixer strip Shared handles before MixerState consumes bus_handles.
                let mut panel_organisms: Vec<OrganismUiState> = Vec::new();
                for (i, (dna, cells)) in dna_list.iter().zip(panel_cells).enumerate() {
                    let strip_idx = i; // organisms start at index 0
                    let (mixer_mute, mixer_gain) = if strip_idx < bus_handles.strips.len() {
                        (
                            bus_handles.strips[strip_idx].mute.clone(),
                            bus_handles.strips[strip_idx].gain.clone(),
                        )
                    } else {
                        (
                            crate::dsp::shared::shared(0.0),
                            crate::dsp::shared::shared(0.6),
                        )
                    };
                    let shape_id = match dna.species.as_str() {
                        "tblk" => 0,
                        "dron" => 1,
                        "melo" => 2,
                        _ => 3,
                    };
                    // Get reverb send handle from reverb bus handles
                    let reverb_send = reverb_handles.as_ref()
                        .and_then(|rh| rh.send_levels.get(i).cloned());

                    // Get tape delay send handle from tape delay bus handles
                    let tape_delay_send = tape_delay_handles.as_ref()
                        .and_then(|th| th.send_levels.get(i).cloned());

                    panel_organisms.push(OrganismUiState {
                        name: dna.name.clone(),
                        species: dna.species.clone(),
                        hue: dna.render.hue,
                        organism_id: org_ids[i],
                        mod_id: mod_ids[i],
                        mixer_mute,
                        mixer_gain,
                        cells,
                        shape_id,
                        reverb_send,
                        tape_delay_send,
                        audio_idx: i,
                    });
                }

                // Build reverb bus UI state (stored on app, not organism panel)
                let reverb_bus_ui = reverb_handles.as_ref().map(|rh| {
                    ReverbBusUiState {
                        reverb_type: rh.reverb_type.clone(),
                        return_level: rh.return_level.clone(),
                        params: rh.params.clone(),
                    }
                });

                // Build tape delay bus UI state (stored on app, not organism panel)
                let tape_delay_bus_ui = tape_delay_handles.as_ref().map(|th| {
                    TapeDelayBusUiState {
                        return_level: th.return_level.clone(),
                        params: th.params.clone(),
                    }
                });

                let organism_panel = OrganismPanelState {
                    organisms: panel_organisms,
                    reverb_bus: None,
                    tape_delay_bus: None,
                };

                let mixer_state = MixerState::new(bus_handles);
                (Some(substrate), Some(mixer_state), Some(meter_rx), Some(organism_panel), reverb_handles, tape_delay_handles, reverb_bus_ui, tape_delay_bus_ui)
            }
            None => {
                log::warn!("Audio unavailable");
                (None, None, None, None, None, None, None, None)
            }
        };

        eprintln!(
            "Reactor initialized: {} modules, {} edges (infra={}, organism={}), audio={}",
            reactor.module_count(),
            reactor.edge_count(),
            reactor.infra_edge_count(),
            reactor.organism_edge_count(),
            audio.is_some(),
        );

        // Dump infra edges for debugging
        {
            let port_names = crate::ui::build_port_names(&reactor);
            for ((src_mod, src_port), targets) in reactor.infra_router.iter_routes() {
                for (dst_mod, dst_port, sig_type) in targets {
                    let src_name = port_names.get(src_port).map(|(m, p)| format!("{}.{}", m, p)).unwrap_or_else(|| format!("mod{}:{}", src_mod, src_port));
                    let dst_name = port_names.get(dst_port).map(|(m, p)| format!("{}.{}", m, p)).unwrap_or_else(|| format!("mod{}:{}", dst_mod, dst_port));
                    eprintln!("  edge: {} -> {} ({:?})", src_name, dst_name, sig_type);
                }
            }
        }

        Self {
            last_frame_time: None,
            start_time: 0.0,
            recorder: Recorder::new(300),
            render_state,
            reactor,
            workspace: WorkspaceState::default(),
            kbd_id,
            analysis_id,
            quantizer_id,
            tala_id,
            raga_id,
            scale_id,
            audio,
            mixer_state,
            meter_rx,
            organism_panel,
            _reverb_bus_handles: reverb_bus_handles,
            tape_delay_bus_handles,
            organism_registry,
            gravity_state: GravityState::neutral(),
            aggregate_emotion: ModuleEmotion::new(5.0),
            beat_phase: 0.0,
            manual_gravity: false,
            prev_bpm: 130.0,
            preset_panel: PresetPanelState::new(std::path::PathBuf::from("assets/presets")),
            available_dna,
            next_audio_idx: dna_list.len(),
            gravity_field: GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42),
            cached_base_weights: [1.0; 12],
            dragging_well: None,
            scaling_well: None,
            well_dispatch_buf: Vec::with_capacity(16),
            well_energy: (0..3).map(|i| WellEnergy::new(i as u32)).collect(),
            well_proximity_buf: Vec::with_capacity(16),
            base_key: 0,
            show_well_overlays: true,
            show_hover_tags: true,
            prev_gravity_bypassed: false,
            effects_bypass: EffectsBypassState::default(),
            reverb_bus_ui,
            tape_delay_bus_ui,
            phys_accumulator: 0.0,
            nav_frame_tick: 0,
        }
    }

    /// Apply LJ well forces + energy drain to organisms (once per frame).
    /// Softened Lennard-Jones creates orbital trench instead of center-seeking pull.
    fn apply_well_forces(&mut self) {
        let dispatch_len = self.well_dispatch_buf.len();

        // Resize proximity buffer to match dispatch buffer
        self.well_proximity_buf.clear();
        self.well_proximity_buf.resize(dispatch_len, WellProximity::default());

        if self.effects_bypass.gravity_bypassed {
            return;
        }

        // Global audio energy for beat pulse (average across organisms)
        let org_count = self.organism_registry.organisms().len().max(1);
        let global_audio_energy: f32 = self.organism_registry.organisms().iter()
            .map(|o| o.audio_energy)
            .sum::<f32>()
            / org_count as f32;

        // Per-well occupant tracking for energy drain
        let well_count = self.gravity_field.wells().len();
        let mut well_occ_count = [0u32; 6];
        let mut well_occ_influence = [0.0f32; 6];

        // Per-well centroid list for spectral niche: (dispatch_idx, centroid)
        let mut well_occupants: [Vec<(usize, f32)>; 6] = Default::default();

        for i in 0..dispatch_len {
            let entry = &self.well_dispatch_buf[i];
            if entry.scale_affinity < 0.001 {
                continue;
            }

            let mut total_fx = 0.0_f32;
            let mut total_fy = 0.0_f32;
            let mut prox = WellProximity::default();

            for (wi, well) in self.gravity_field.wells().iter().enumerate() {
                let dx = well.position[0] - entry.pos[0];
                let dy = well.position[1] - entry.pos[1];
                let r_sq = dx * dx + dy * dy;
                let r = r_sq.sqrt().max(0.001);

                if r > well.radius * 1.2 {
                    continue;
                }

                let interval = ((well.root_pitch_class as i8 - entry.org_root as i8)
                    .rem_euclid(12)) as u8;
                let consonance = consonance_weight(interval);

                let m_well = self.well_energy[wi].energy * well.strength;
                let g_eff = LJ_GRAVITY * consonance * entry.scale_affinity
                    * entry.lj_gravity_scale;

                // Trench model: displacement from equilibrium ring
                let r_eq = well.radius * LJ_TRENCH_FRACTION;
                let displacement = r - r_eq;
                let eps_sq = LJ_SOFTENING * LJ_SOFTENING;
                let f_radial = g_eff * m_well * displacement / (r_sq + eps_sq);

                // Beat pulse: amplify outward push when inside trench
                let beat_mod = if displacement < 0.0 {
                    1.0 + global_audio_energy * BEAT_PULSE_AMPLITUDE
                        * entry.beat_pulse_sensitivity
                } else {
                    1.0
                };

                let f_net = (f_radial * beat_mod).clamp(-MAX_WELL_FORCE, MAX_WELL_FORCE);

                total_fx += (dx / r) * f_net;
                total_fy += (dy / r) * f_net;

                // Track occupancy for energy drain + build WellInfluence
                if r < well.radius && wi < 6 {
                    let influence = (1.0 - (r / well.radius).powi(2)).max(0.0);
                    well_occ_count[wi] += 1;
                    well_occ_influence[wi] += influence;

                    // Record influence in WellProximity
                    let idx = prox.influence_count as usize;
                    if idx < 6 {
                        let quality = influence * consonance;
                        prox.influences[idx] = WellInfluence {
                            well_id: well.id,
                            influence,
                            consonance,
                            quality,
                        };
                        prox.influence_count += 1;
                        if quality > prox.best_quality {
                            prox.best_quality = quality;
                        }
                    }

                    // Track centroid for spectral niche penalty
                    if entry.spectral_centroid > 0.0 {
                        well_occupants[wi].push((i, entry.spectral_centroid));
                    }
                }
            }

            if let Some(org) = self.organism_registry.get_mut(entry.org_id) {
                org.apply_force([total_fx, total_fy]);
            }

            self.well_proximity_buf[i] = prox;
        }

        // Tick well energy with occupancy data
        for wi in 0..well_count.min(6) {
            self.well_energy[wi].tick(well_occ_count[wi], well_occ_influence[wi]);
        }

        // Spectral niche penalty: pairwise centroid overlap within each well
        for wi in 0..well_count.min(6) {
            let occupants = &well_occupants[wi];
            if occupants.len() < 2 {
                continue;
            }
            for &(idx_a, cent_a) in occupants {
                let mut max_overlap = 0.0_f32;
                for &(idx_b, cent_b) in occupants {
                    if idx_a == idx_b || cent_a <= 0.0 || cent_b <= 0.0 {
                        continue;
                    }
                    let octave_dist = (cent_a / cent_b).log2().abs();
                    let overlap = (1.0 - octave_dist / OCTAVE_THRESHOLD).max(0.0);
                    if overlap > max_overlap {
                        max_overlap = overlap;
                    }
                }
                // Use worst (max) overlap as the niche penalty for this organism
                if max_overlap > self.well_proximity_buf[idx_a].niche_penalty {
                    self.well_proximity_buf[idx_a].niche_penalty = max_overlap;
                }
            }
        }

        // Finalize net_score and distribute to OrganismModule + OrganismState
        for i in 0..dispatch_len {
            let prox = &mut self.well_proximity_buf[i];
            let best_well_idx = prox.influences.iter()
                .take(prox.influence_count as usize)
                .enumerate()
                .max_by(|(_, a), (_, b)| a.quality.partial_cmp(&b.quality).unwrap())
                .map(|(idx, _)| idx);

            let well_energy = best_well_idx
                .and_then(|idx| {
                    let wid = prox.influences[idx].well_id as usize;
                    self.well_energy.get(wid).map(|we| we.energy)
                })
                .unwrap_or(0.0);

            prox.net_score = prox.best_quality * well_energy * (1.0 - prox.niche_penalty);

            let entry = &self.well_dispatch_buf[i];
            if let Some(org) = self.organism_registry.get_mut(entry.org_id) {
                org.well_net_score = prox.net_score;
            }
        }
    }

    /// Distribute well proximity data to OrganismModules (separate pass,
    /// because apply_well_forces borrows organism_registry mutably).
    fn distribute_well_proximity(&mut self) {
        for i in 0..self.well_dispatch_buf.len() {
            let mod_id = self.well_dispatch_buf[i].mod_id;
            let prox = self.well_proximity_buf[i].clone();
            if let Some(m) = self.reactor.module_mut(mod_id) {
                if let Some(org_mod) = m.as_any_mut().downcast_mut::<OrganismModule>() {
                    org_mod.set_well_proximity(prox);
                }
            }
        }
    }

    /// S39: Detect navigation events and apply valence/arousal rewards.
    fn detect_navigation_events(&mut self) {
        if self.effects_bypass.gravity_bypassed {
            return;
        }

        self.nav_frame_tick += 1;
        let current_tick = self.nav_frame_tick;
        let dispatch_len = self.well_dispatch_buf.len();

        for i in 0..dispatch_len {
            // Copy fields to avoid borrow conflicts
            let mod_id = self.well_dispatch_buf[i].mod_id;
            let org_id = self.well_dispatch_buf[i].org_id;
            let pos = self.well_dispatch_buf[i].pos;
            let scale_affinity = self.well_dispatch_buf[i].scale_affinity;
            let org_root = self.well_dispatch_buf[i].org_root;
            let max_speed = self.well_dispatch_buf[i].max_speed;

            if scale_affinity < 0.01 {
                continue; // KKIT exemption
            }

            // Compute organism speed from velocity
            let org_speed = self.organism_registry.get(org_id)
                .map(|o| (o.velocity[0] * o.velocity[0] + o.velocity[1] * o.velocity[1]).sqrt())
                .unwrap_or(0.0);

            // Ensure tracker exists and reset for this frame
            let tracker = self.organism_registry.ensure_well_tracker(org_id);
            tracker.reset_delta();

            // Track which wells are in range this frame
            let mut active_well_ids = Vec::new();

            for well in self.gravity_field.wells() {
                let dx = well.position[0] - pos[0];
                let dy = well.position[1] - pos[1];
                let distance = (dx * dx + dy * dy).sqrt();

                if distance < well.radius {
                    active_well_ids.push(well.id);
                }

                let interval = ((well.root_pitch_class as i8 - org_root as i8)
                    .rem_euclid(12)) as u8;
                let consonance = consonance_weight(interval);

                tracker.process_well(
                    well.id,
                    distance,
                    well.radius,
                    org_speed,
                    max_speed,
                    consonance,
                    scale_affinity,
                    current_tick,
                );
            }

            tracker.finalize_frame(&active_well_ids, org_speed, max_speed, scale_affinity, current_tick);
            tracker.decay_trap_stress(&active_well_ids);

            let nav_delta = tracker.nav_valence_delta;
            let max_trap = tracker.max_trap_stress();

            // Apply to ModuleEmotion
            if let Some(emotion) = self.reactor.graph.emotions.get_mut(&mod_id) {
                emotion.apply_navigation_reward(nav_delta, NAV_WEIGHT);
                if max_trap > 0.0 {
                    emotion.apply_trap_arousal(max_trap);
                }
            }
        }
    }

    fn capture_preset(&self, name: String) -> crate::preset::Preset {
        let raga_name = self
            .reactor
            .module_ref(self.raga_id)
            .and_then(|m| m.as_any().downcast_ref::<RagaModule>())
            .map(|r| r.current_raga_name().to_string())
            .unwrap_or_else(|| "bhairav".into());
        let (tala_name, tempo) = self
            .reactor
            .module_ref(self.tala_id)
            .and_then(|m| m.as_any().downcast_ref::<TalaModule>())
            .map(|t| (t.current_tala_name().to_string(), t.tempo_bpm()))
            .unwrap_or(("teentaal".into(), 120.0));

        crate::preset::Preset {
            name,
            raga: raga_name,
            tala: tala_name,
            tempo_bpm: tempo,
            pitch_gravity: self.gravity_state.pitch_gravity,
            rhythm_gravity: self.gravity_state.rhythm_gravity,
            gamaka_depth: self.gravity_state.gamaka_depth,
            morph_speed: self.gravity_state.morph_speed,
            manual_gravity: self.manual_gravity,
        }
    }

    fn apply_preset(&mut self, preset: &crate::preset::Preset) {
        if let Some(m) = self.reactor.module_mut(self.raga_id) {
            m.receive_event(&crate::modules::raga_module::SetRaga(preset.raga.clone()));
        }
        if let Some(m) = self.reactor.module_mut(self.tala_id) {
            m.receive_event(&crate::modules::tala_module::SetTala(preset.tala.clone()));
            m.receive_event(&crate::modules::tala_module::SetTempo(preset.tempo_bpm as f32));
        }
        self.gravity_state.pitch_gravity = preset.pitch_gravity;
        self.gravity_state.rhythm_gravity = preset.rhythm_gravity;
        self.gravity_state.gamaka_depth = preset.gamaka_depth;
        self.gravity_state.morph_speed = preset.morph_speed;
        self.manual_gravity = preset.manual_gravity;
    }

    fn load_preset_by_index(&mut self, idx: usize) {
        if idx < self.preset_panel.presets.len() {
            let path = self.preset_panel.presets[idx].1.clone();
            match crate::preset::load(&path) {
                Ok(preset) => {
                    eprintln!("[preset] loaded '{}'", preset.name);
                    self.apply_preset(&preset);
                    self.preset_panel.last_message =
                        Some(format!("Loaded '{}'", preset.name));
                }
                Err(e) => {
                    eprintln!("[preset] load error: {e}");
                    self.preset_panel.last_message = Some(format!("Error: {e}"));
                }
            }
        }
    }

    /// Spawn a new organism instance from a DNA preset at runtime.
    ///
    /// Builds DSP + channels on the control thread, then sends a `SpawnPayload`
    /// to the audio callback via SPSC ring buffer. The callback integrates it
    /// at the next boundary — no allocation on the audio thread.
    fn spawn_organism(&mut self, dna: OrganismDna) {
        // Read sample rate first (immutable borrow, then drop)
        let sr = match self.audio {
            Some(ref a) => a.sample_rate as f32,
            None => {
                log::warn!("[spawn] No audio substrate — cannot spawn '{}'", dna.name);
                return;
            }
        };

        // 1. Build DSP on control thread (allocates freely)
        let (org_dsp, shared_handles) = match crate::dsp::organism_dsp::OrganismDsp::from_dna(&dna, sr) {
            Some(pair) => pair,
            None => {
                log::warn!("[spawn] from_dna failed for '{}'", dna.name);
                return;
            }
        };

        // 2. Create command + analysis channels
        let (cmd_tx, cmd_rx) = channel::channel::<crate::dsp::command::DspCommand>(64);
        let (analysis_tx, analysis_rx) = channel::channel::<crate::dsp::command::DspAnalysis>(32);

        // 3. Create VoiceBus strip (strip goes to audio, handles stay on control thread)
        let (strip, strip_handles) = ChannelStrip::new(&dna.name, gain_staging::species_gain(&dna.name));

        // 4. Clone mute/gain before strip_handles is consumed by push
        let mute_clone = strip_handles.mute.clone();
        let gain_clone = strip_handles.gain.clone();

        // 5. Create send Shared values on control thread
        let reverb_send = dna.sends.as_ref()
            .and_then(|s| s.reverb.as_ref())
            .map(|r| crate::dsp::shared::shared(r.send));
        let tape_delay_send = dna.sends.as_ref()
            .and_then(|s| s.tape_delay.as_ref())
            .map(|td| crate::dsp::shared::shared(td.send));

        // 6. Send payload to audio thread
        let payload = SpawnPayload {
            dsp: org_dsp,
            cmd_rx,
            analysis_tx,
            strip,
            reverb_send: reverb_send.clone(),
            tape_delay_send: tape_delay_send.clone(),
        };
        if let Some(ref mut a) = self.audio {
            let _ = a.spawn_tx.try_send(payload);
        }

        // 7. Extend mixer strip handle list (control thread side)
        if let Some(ref mut ms) = self.mixer_state {
            ms.handles.strips.push(strip_handles);
        }

        // 8. Extend bus handle lists (control thread side)
        if let Some(ref mut rh) = self._reverb_bus_handles {
            if let Some(s) = reverb_send.clone() {
                rh.push_send(s);
            }
        }
        if let Some(ref mut th) = self.tape_delay_bus_handles {
            if let Some(s) = tape_delay_send.clone() {
                th.push_send(s);
            }
        }

        // 9. Spawn visual organism in registry
        let pos = seeded_spawn_pos(dna.seed, [0.0, 0.0, 1200.0, 700.0], 100.0);
        let org_id = self.organism_registry.spawn(pos, dna.body.lobe_count, dna.body.core_radius);
        if let Some(org) = self.organism_registry.get_mut(org_id) {
            org.base_hue = dna.render.hue;
            org.smin_k = dna.render.smin_k;
            org.edge_softness = dna.render.edge_softness;
            org.base_glow = dna.render.glow;
            org.pulse_response = dna.render.pulse_response;
            org.drag = dna.physics.drag;
            org.max_speed = dna.physics.max_speed;
            org.mass = dna.physics.mass;
            org.viscosity = dna.physics.viscosity;
            org.pseudopod_gain = dna.body.pseudopod_gain;
            org.extension_speed = dna.body.extension_speed;
            org.retraction_speed = dna.body.retraction_speed;
            org.shape_amplitude = dna.body.shape_amplitude;
            org.shape_frequency = dna.body.shape_frequency;
            org.rd_reactivity = dna.body.rd_reactivity;
            org.rd_feed = dna.body.rd_feed;
            org.rd_kill = dna.body.rd_kill;
            org.rd_scale = dna.body.rd_scale;
            org.harmonic_count = dna.body.harmonic_count;
            org.harmonic_amp = dna.body.harmonic_amp;
            org.elongation = dna.body.elongation;
            org.chladni_m = dna.body.chladni_m;
            org.chladni_n = dna.body.chladni_n;
            let vel = seeded_spawn_vel(dna.seed, dna.physics.max_speed);
            org.velocity = vel;
            let spd = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
            org.visual_dir = if spd > 1.0 { [vel[0] / spd, vel[1] / spd] } else { [1.0, 0.0] };
            org.smooth_speed = spd;
            org.prev_audio_energy = 0.0;
            org.scale_affinity = dna.scale_affinity;
            org.root_pitch_class = dna.root_pitch_class;
            org.energy = 0.7;
            org.arousal = dna.emotion.base_arousal;
            org.valence = dna.emotion.base_valence;
            org.desire_to_connect = dna.emotion.desire_to_connect;
            org.species = dna.species.clone();
            org.interaction_rules = dna.physics.interaction_rules.clone();
            org.reverb_send_base = dna.sends.as_ref()
                .and_then(|s| s.reverb.as_ref())
                .map(|r| r.send)
                .unwrap_or(0.0);
            org.tape_delay_send_base = dna.sends.as_ref()
                .and_then(|s| s.tape_delay.as_ref())
                .map(|td| td.send)
                .unwrap_or(0.0);
        }

        // 10. Register OrganismModule with reactor
        let mod_id = self.reactor.register(Box::new(OrganismModule::new(
            dna.clone(),
            shared_handles.clone(),
            analysis_rx,
            cmd_tx,
            org_id,
        )));

        // 11. Build CellUiState for organism panel
        let cells: Vec<CellUiState> = dna.cells.iter().enumerate().map(|(ci, cell_dna)| {
            let bypass = shared_handles
                .get(&format!("cell{}.bypass", ci))
                .cloned()
                .unwrap_or_else(|| crate::dsp::shared::shared(0.0));
            let prefix = format!("cell{}.", ci);
            let mut params: Vec<(String, crate::dsp::shared::Shared)> = shared_handles
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix) && !k.ends_with(".bypass"))
                .map(|(k, v)| (k.strip_prefix(&prefix).unwrap().to_string(), v.clone()))
                .collect();
            params.sort_by(|a, b| a.0.cmp(&b.0));
            let ranges = cell_type_ranges(&cell_dna.cell_type);
            let param_ranges: Vec<(String, f32, f32)> = params.iter().map(|(name, _)| {
                let (min, max) = find_range(ranges, name).unwrap_or((0.0, 1.0));
                (name.clone(), min, max)
            }).collect();
            CellUiState { cell_type: cell_dna.cell_type.clone(), bypass, params, param_ranges }
        }).collect();

        // 12. Add to organism panel
        let shape_id = match dna.species.as_str() { "tblk" => 0, "dron" => 1, "melo" => 2, _ => 3 };
        let reverb_send_ui = self._reverb_bus_handles.as_ref()
            .and_then(|rh| rh.send_levels.last().cloned());
        let tape_delay_send_ui = self.tape_delay_bus_handles.as_ref()
            .and_then(|th| th.send_levels.last().cloned());

        let audio_idx = self.next_audio_idx;
        self.next_audio_idx += 1;

        if let Some(ref mut panel) = self.organism_panel {
            panel.organisms.push(OrganismUiState {
                name: dna.name.clone(),
                species: dna.species.clone(),
                hue: dna.render.hue,
                organism_id: org_id,
                mod_id,
                mixer_mute: mute_clone,
                mixer_gain: gain_clone,
                cells,
                shape_id,
                reverb_send: reverb_send_ui,
                tape_delay_send: tape_delay_send_ui,
                audio_idx,
            });
        }

        eprintln!("[spawn] '{}' (org_id={}, mod_id={}, audio_idx={})", dna.name, org_id, mod_id, audio_idx);
    }

    /// Kill an organism: silence audio, tombstone DSP slot, remove from panel/reactor/registry.
    ///
    /// Phase A: zero sends + mute (immediate silence).
    /// Phase B: send tombstone index to audio thread (stops DSP tick, reclaims CPU).
    fn kill_organism(&mut self, ka: KillAction) {
        // 1. Mute dry path + zero effect sends BEFORE removing from panel
        if let Some(ref panel) = self.organism_panel {
            if let Some(org_ui) = panel.organisms.get(ka.panel_idx) {
                org_ui.mixer_mute.set(1.0);
                // Zero reverb send — stops feeding the reverb bus
                if let Some(ref rs) = org_ui.reverb_send {
                    rs.set(0.0);
                }
                // Zero tape delay send — stops feeding the delay bus
                if let Some(ref ts) = org_ui.tape_delay_send {
                    ts.set(0.0);
                }
            }
        }

        // 2. Send tombstone index to audio thread (marks slot dead, skips tick)
        if let Some(ref mut audio) = self.audio {
            let _ = audio.despawn_tx.try_send(ka.audio_idx);
        }

        // 3. Remove from organism panel
        if let Some(ref mut panel) = self.organism_panel {
            panel.organisms.remove(ka.panel_idx);
        }

        // 4. Unregister from reactor (removes edges + affinity state)
        self.reactor.unregister(ka.mod_id);

        // 5. Despawn from visual registry
        self.organism_registry.despawn(ka.org_id);

        eprintln!("[kill] org_id={}, mod_id={}, audio_idx={}", ka.org_id, ka.mod_id, ka.audio_idx);
    }
}

/// Convert an egui key to a SolidoKey, if mapped.
fn egui_key_to_solido(key: egui::Key) -> Option<SolidoKey> {
    match key {
        egui::Key::Num1 => Some(SolidoKey::Num1),
        egui::Key::Num2 => Some(SolidoKey::Num2),
        egui::Key::Num3 => Some(SolidoKey::Num3),
        egui::Key::Num4 => Some(SolidoKey::Num4),
        egui::Key::Num5 => Some(SolidoKey::Num5),
        egui::Key::Num6 => Some(SolidoKey::Num6),
        egui::Key::Num7 => Some(SolidoKey::Num7),
        egui::Key::ArrowUp => Some(SolidoKey::ArrowUp),
        egui::Key::ArrowDown => Some(SolidoKey::ArrowDown),
        egui::Key::ArrowLeft => Some(SolidoKey::ArrowLeft),
        egui::Key::ArrowRight => Some(SolidoKey::ArrowRight),
        egui::Key::Space => Some(SolidoKey::Space),
        egui::Key::R => Some(SolidoKey::R),
        egui::Key::T => Some(SolidoKey::T),
        egui::Key::P => Some(SolidoKey::P),
        egui::Key::D => Some(SolidoKey::D),
        egui::Key::E => Some(SolidoKey::E),
        egui::Key::S => Some(SolidoKey::S),
        egui::Key::Escape => Some(SolidoKey::Escape),
        egui::Key::F1 => Some(SolidoKey::F1),
        egui::Key::F2 => Some(SolidoKey::F2),
        egui::Key::F3 => Some(SolidoKey::F3),
        egui::Key::OpenBracket => Some(SolidoKey::OpenBracket),
        egui::Key::CloseBracket => Some(SolidoKey::CloseBracket),
        _ => None,
    }
}

/// Extract pairwise organism affinity weights from the reactor's AffinityGraph.
///
/// For each pair of OrganismModules, averages all edge weights connecting them.
/// Returns (org_id_a, org_id_b, avg_weight) tuples.
fn extract_organism_affinities(
    reactor: &SeedReactor,
) -> Vec<(u32, u32, f32)> {
    use crate::organism::sim::OrganismId;

    // Collect (ModuleId, OrganismId) for all OrganismModules
    let mut module_to_org: Vec<(ModuleId, OrganismId)> = Vec::new();
    for (&mod_id, module) in reactor.modules_iter() {
        if let Some(org_mod) = module.as_any().downcast_ref::<OrganismModule>() {
            module_to_org.push((mod_id, org_mod.organism_id()));
        }
    }

    let mut pairs = Vec::new();
    for i in 0..module_to_org.len() {
        for j in (i + 1)..module_to_org.len() {
            let (mod_a, org_a) = module_to_org[i];
            let (mod_b, org_b) = module_to_org[j];

            // Find all edges between mod_a and mod_b (both directions)
            let mut weight_sum = 0.0_f32;
            let mut count = 0_u32;
            for (&(src, _, dst, _), edge) in &reactor.graph.edges {
                if (src == mod_a && dst == mod_b) || (src == mod_b && dst == mod_a) {
                    weight_sum += edge.weight;
                    count += 1;
                }
            }

            if count > 0 {
                pairs.push((org_a, org_b, weight_sum / count as f32));
            }
        }
    }
    pairs
}

impl eframe::App for SolidoApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Deferred readback from previous frame's capture (BioField renderer)
        if self.recorder.pending_capture {
            if let Some(rs) = self.render_state.as_ref() {
                let renderer = rs.renderer.read();
                if let Some(resources) = renderer.callback_resources.get::<BioFieldRenderResources>() {
                    let now_time = self.last_frame_time.unwrap_or(0.0) as f32;
                    let frame_num = self.recorder.next_frame_number();
                    if let Some(frame) = biofield_renderer::read_captured_frame(
                        &rs.device,
                        resources,
                        frame_num,
                        now_time,
                    ) {
                        self.recorder.push_frame(frame);
                    }
                }
            }
            self.recorder.pending_capture = false;
        }

        // Delta time
        let now = ctx.input(|i| i.time);
        let delta = self
            .last_frame_time
            .map(|t| (now - t) as f32)
            .unwrap_or(0.016);
        self.last_frame_time = Some(now);

        if self.start_time == 0.0 {
            self.start_time = now;
        }

        // Collect keyboard events
        let ctrl_held = ctx.input(|i| i.modifiers.ctrl);
        let (keys, releases): (Vec<SolidoKey>, Vec<SolidoKey>) = ctx.input(|i| {
            let mut presses = Vec::new();
            let mut releases = Vec::new();
            for event in &i.events {
                if let egui::Event::Key { key, pressed, repeat, .. } = event {
                    if *repeat {
                        continue;
                    }
                    if let Some(sk) = egui_key_to_solido(*key) {
                        if *pressed {
                            presses.push(sk);
                        } else {
                            releases.push(sk);
                        }
                    }
                }
            }
            (presses, releases)
        });

        // Direct-action keyboard dispatch (before module routing)
        let mut keys_for_module: Vec<SolidoKey> = Vec::new();
        for &key in &keys {
            match key {
                SolidoKey::Space => {
                    // Toggle play/pause
                    let playing = self.reactor.clock.is_playing();
                    self.reactor.clock.playing.set(if playing { 0.0 } else { 1.0 });
                }
                SolidoKey::Escape => {
                    // Stop: pause + panic all organisms
                    self.reactor.clock.playing.set(0.0);
                    self.reactor.broadcast_organism_command(
                        crate::dsp::command::DspCommand::Panic,
                    );
                }
                SolidoKey::OpenBracket => {
                    // BPM -1
                    let bpm = self.reactor.clock.bpm_value();
                    self.reactor.clock.bpm.set((bpm - 1.0).max(20.0));
                }
                SolidoKey::CloseBracket => {
                    // BPM +1
                    let bpm = self.reactor.clock.bpm_value();
                    self.reactor.clock.bpm.set((bpm + 1.0).min(300.0));
                }
                SolidoKey::P => {
                    // Panic: reset gravity
                    self.gravity_state = GravityState::neutral();
                    self.manual_gravity = true;
                    keys_for_module.push(key);
                }
                SolidoKey::F1 => {
                    self.workspace.panels.debug = !self.workspace.panels.debug;
                }
                SolidoKey::F2 => {
                    self.workspace.panels.mixer = !self.workspace.panels.mixer;
                }
                SolidoKey::F3 => {
                    self.workspace.panels.ledger = !self.workspace.panels.ledger;
                }
                _ => {
                    keys_for_module.push(key);
                }
            }
        }

        // Ctrl+S = save preset, Ctrl+1-9 = load preset
        if ctrl_held {
            for &key in &keys {
                match key {
                    SolidoKey::Num1 => self.load_preset_by_index(0),
                    SolidoKey::Num2 => self.load_preset_by_index(1),
                    SolidoKey::Num3 => self.load_preset_by_index(2),
                    SolidoKey::Num4 => self.load_preset_by_index(3),
                    SolidoKey::Num5 => self.load_preset_by_index(4),
                    SolidoKey::Num6 => self.load_preset_by_index(5),
                    SolidoKey::Num7 => self.load_preset_by_index(6),
                    _ => {}
                }
            }
        }

        // Feed remaining keys to the keyboard module
        if let Some(module) = self.reactor.module_mut(self.kbd_id) {
            if !ctrl_held {
                for key in keys_for_module {
                    module.receive_event(&crate::modules::keyboard_input::KeyPress(key));
                }
            }
            for key in releases {
                module.receive_event(&crate::modules::keyboard_input::KeyRelease(key));
            }
        }

        let screen = ctx.input(|i| i.viewport_rect());

        // Drain bus meters and update mixer state
        if let (Some(ms), Some(rx)) = (&mut self.mixer_state, &mut self.meter_rx) {
            while let Some(report) = rx.try_recv() {
                let count = report.count as usize;
                ms.meters[..count].copy_from_slice(&report.meters[..count]);
                ms.meter_count = count;
            }
            ms.apply_automation();
            ms.advance_transport(delta as f64);
        }

        // Tick the reactor (module signal routing + learning) — skip when paused
        if self.reactor.clock.is_playing() {
            self.reactor.tick(delta);
        }

        // S09: Update gravity state from aggregate emotion (unless manual mode)
        let emotion_count = self.reactor.graph.emotions.len();
        if emotion_count > 0 {
            let mut avg_valence = 0.0_f32;
            let mut avg_arousal = 0.0_f32;
            for emotion in self.reactor.graph.emotions.values() {
                avg_valence += emotion.valence;
                avg_arousal += emotion.arousal;
            }
            avg_valence /= emotion_count as f32;
            avg_arousal /= emotion_count as f32;
            self.aggregate_emotion.valence = avg_valence;
            self.aggregate_emotion.arousal = avg_arousal;
        }
        if !self.manual_gravity {
            self.gravity_state = GravityState::from_emotion(&self.aggregate_emotion);
        }

        // Dispatch gravity state to quantizer + tala modules (direct, no graph)
        if let Some(m) = self.reactor.module_mut(self.quantizer_id) {
            if let Some(q) = m.as_any().downcast_ref::<QuantizerModule>() {
                let port = q.gravity_override_port;
                let _ = q;
                let _ = m.receive_signal(port, Signal::Float(self.gravity_state.pitch_gravity));
            }
        }
        if let Some(m) = self.reactor.module_mut(self.tala_id) {
            if let Some(t) = m.as_any().downcast_ref::<TalaModule>() {
                let port = t.gravity_override_port;
                let _ = t;
                let _ = m.receive_signal(port, Signal::Float(self.gravity_state.rhythm_gravity));
            }
        }

        // S09b: Bridge per-organism emotion + audio energy from reactor → visual state (AD-2)
        for (&mod_id, module) in self.reactor.modules_iter() {
            if let Some(org_mod) = module.as_any().downcast_ref::<OrganismModule>() {
                if let Some(org) = self.organism_registry.get_mut(org_mod.organism_id()) {
                    // Audio energy: direct RMS from DSP (already 60Hz smoothed)
                    org.audio_energy = org_mod.audio_rms();

                    // Emotion: lerp from graph state (3Hz smoothing)
                    if let Some(emotion) = self.reactor.graph.emotions.get(&mod_id) {
                        let alpha = (delta * 3.0).min(1.0);
                        org.arousal += (emotion.arousal - org.arousal) * alpha;
                        org.valence += (emotion.valence - org.valence) * alpha;
                    }
                }
            }
        }

        // Gravity Wells: per-organism spatial harmonic field dispatch
        // Phase 1 (read-only): read base weights from ScaleModule, collect organism data
        if let Some(m) = self.reactor.module_ref(self.scale_id) {
            if let Some(sm) = m.as_any().downcast_ref::<ScaleModule>() {
                self.cached_base_weights = sm.current_weights();
            }
        }

        // Collect per-organism data into pre-allocated buffer (no per-frame heap alloc)
        self.well_dispatch_buf.clear();
        for (&mod_id, module) in self.reactor.modules_iter() {
            if let Some(org_mod) = module.as_any().downcast_ref::<OrganismModule>() {
                let org_id = org_mod.organism_id();
                let sa = org_mod.dna().scale_affinity;
                let fid = org_mod.dna().fidelity;
                let sc = org_mod.current_spectral_centroid();
                let wr = &org_mod.dna().physics.well_response;
                let lj_gs = wr.lj_gravity_scale;
                let bps = wr.beat_pulse_sensitivity;
                if let Some(org) = self.organism_registry.get(org_id) {
                    self.well_dispatch_buf.push(WellDispatchEntry {
                        mod_id,
                        org_id,
                        pos: org.position,
                        scale_affinity: sa,
                        fidelity: fid,
                        spectral_centroid: sc,
                        org_root: org.root_pitch_class,
                        lj_gravity_scale: lj_gs,
                        beat_pulse_sensitivity: bps,
                        max_speed: org.max_speed,
                    });
                }
            }
        }

        // Bypass transition: send neutral chromatic weights when gravity is just toggled off
        if self.effects_bypass.gravity_bypassed && !self.prev_gravity_bypassed {
            for i in 0..self.well_dispatch_buf.len() {
                let mod_id = self.well_dispatch_buf[i].mod_id;
                if let Some(m) = self.reactor.module_mut(mod_id) {
                    if let Some(org_mod) = m.as_any_mut().downcast_mut::<OrganismModule>() {
                        org_mod.send_command(
                            crate::dsp::command::DspCommand::SetScaleWeights([1.0; 12], 0.0),
                        );
                    }
                }
            }
        }
        self.prev_gravity_bypassed = self.effects_bypass.gravity_bypassed;

        // Phase 2 (mutate registry) + Phase 3 (mutate reactor): compute and dispatch per-organism
        // Skip when gravity is bypassed — organisms play native patterns.
        if !self.effects_bypass.gravity_bypassed {
            for i in 0..self.well_dispatch_buf.len() {
                let entry = &self.well_dispatch_buf[i];
                let (mod_id, org_id, pos, scale_affinity, fidelity) =
                    (entry.mod_id, entry.org_id, entry.pos, entry.scale_affinity, entry.fidelity);
                let eff = self.gravity_field.effective_weights(pos, self.base_key, &self.cached_base_weights);

                // Update drift target on organism state
                if let Some(org) = self.organism_registry.get_mut(org_id) {
                    org.scale_drift_target = eff.total_influence.min(1.0);
                }

                // Compute blend: base blend from DNA + drift contribution from wells
                let drift_blend = self.organism_registry.get(org_id)
                    .map(|o| o.scale_drift_blend)
                    .unwrap_or(0.0);
                let base_blend = scale_affinity * fidelity;
                let blend = base_blend.max(drift_blend * base_blend);

                // Send transposed+well-blended weights to audio thread
                if let Some(m) = self.reactor.module_mut(mod_id) {
                    if let Some(org_mod) = m.as_any_mut().downcast_mut::<OrganismModule>() {
                        org_mod.send_command(
                            crate::dsp::command::DspCommand::SetScaleWeights(eff.weights, blend),
                        );
                    }
                }
            }
        }

        // Proximity + attachment-based send boost: attached organisms share reverb/delay
        if let Some(ref panel) = self.organism_panel {
            for org_ui in &panel.organisms {
                if let Some(org) = self.organism_registry.get(org_ui.organism_id) {
                    let prox = org.proximity_energy;
                    let max_att = self.organism_registry.max_attachment_for(org_ui.organism_id);
                    if let Some(ref reverb_send) = org_ui.reverb_send {
                        let reverb_boost = prox * 0.15 + max_att * 0.4;
                        reverb_send.set((org.reverb_send_base + reverb_boost).min(1.0));
                    }
                    if let Some(ref tape_send) = org_ui.tape_delay_send {
                        let tape_boost = prox * 0.1 + max_att * 0.2;
                        tape_send.set((org.tape_delay_send_base + tape_boost).min(1.0));
                    }
                }
            }
        }

        // Update beat phase (simple time-based, will connect to TalaGrid later)
        self.beat_phase = ((now - self.start_time) as f32 * 2.0) % 1.0;

        // S09: Tick organism simulation
        // Update world bounds to match viewport
        let dpr = ctx.pixels_per_point();
        self.organism_registry.world_bounds = [
            0.0,
            0.0,
            screen.width() * dpr,
            screen.height() * dpr,
        ];
        // Extract pairwise organism affinities from the affinity graph
        let affinities = extract_organism_affinities(&self.reactor);
        self.organism_registry.update_affinities(&affinities);

        // Physics simulation — fixed 120Hz substep loop.
        // Forces apply once per frame; only integration runs per substep.
        if self.reactor.clock.is_playing() {
            // === Once per frame ===
            self.organism_registry.tick_frame(delta);
            self.organism_registry.apply_audio_impulses();
            self.organism_registry.tick_forces(delta);
            self.apply_well_forces();
            self.distribute_well_proximity();
            self.detect_navigation_events();

            // === Fixed-timestep integration ===
            self.phys_accumulator += delta;
            self.phys_accumulator = self.phys_accumulator.min(PHYS_MAX_ACCUM);
            while self.phys_accumulator >= PHYS_DT {
                self.organism_registry.tick_physics(PHYS_DT);
                self.phys_accumulator -= PHYS_DT;
            }
        }

        // Dynamic master gain: scale based on active organism count
        if let Some(ref ms) = self.mixer_state {
            let active_count = self.organism_registry.organisms().len();
            let target_gain = crate::audio::gain_staging::dynamic_master_gain(active_count);
            ms.handles.master_gain.set(target_gain);
        }

        // Propagate global BPM changes to all organisms
        let current_bpm = self.reactor.clock.bpm_value();
        if (current_bpm - self.prev_bpm).abs() > 0.01 {
            self.prev_bpm = current_bpm;
            self.reactor.broadcast_organism_command(
                crate::dsp::command::DspCommand::SetGlobalBpm(current_bpm),
            );
        }

        // --- Workspace UI (header, status bar, debug panel, mixer, organisms, ledger, recorder) ---
        let ids = DebugModuleIds {
            kbd_id: self.kbd_id,
            quantizer_id: self.quantizer_id,
            analysis_id: self.analysis_id,
            raga_id: self.raga_id,
            tala_id: self.tala_id,
            scale_id: self.scale_id,
        };

        let export_clicked = ui::show_workspace(
            ctx,
            &mut self.workspace,
            &self.reactor,
            &mut self.recorder,
            &ids,
            self.mixer_state.as_mut(),
            &self.gravity_state,
            self.beat_phase,
            self.base_key,
        );

        // Organism panel — called outside show_workspace so we can handle kill actions
        let kill_action = if self.workspace.panels.organisms {
            if let Some(ref panel) = self.organism_panel {
                panels::organism_panel::show_organism_panel(ctx, &mut self.workspace.panels.organisms, panel)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(ka) = kill_action {
            self.kill_organism(ka);
        }

        // Spawn panel — called outside show_workspace so we can handle spawn actions
        if self.workspace.panels.spawn {
            let spawn_action = show_spawn_panel(ctx, &self.available_dna, &mut self.workspace.panels.spawn);
            if let Some(SpawnAction::Spawn(dna)) = spawn_action {
                self.spawn_organism(dna);
            }
        }

        if export_clicked {
            let dir = self.recorder.export_dir.clone();
            match self.recorder.export_range_to_dir(std::path::Path::new(&dir)) {
                Ok(n) => {
                    self.recorder.last_export_msg = Some(format!("Exported {n} frames"));
                }
                Err(e) => {
                    self.recorder.last_export_msg = Some(format!("Error: {e}"));
                }
            }
        }

        // Controls panel (needs &mut reactor, so called outside show_workspace)
        if self.workspace.panels.controls {
            let ctrl_ids = ControlPanelIds {
                raga_id: self.raga_id,
                tala_id: self.tala_id,
                scale_id: self.scale_id,
            };
            let ctrl_action = crate::ui::panels::controls::show_control_panel(
                ctx,
                &mut self.workspace.panels.controls,
                &mut self.reactor,
                &ctrl_ids,
                &mut self.base_key,
            );
            if let Some(crate::ui::panels::controls::ControlPanelAction::PanicAll) = ctrl_action {
                self.reactor.broadcast_organism_command(
                    crate::dsp::command::DspCommand::Panic,
                );
            }
        }

        // Presets panel (needs &mut reactor for apply)
        if self.workspace.panels.presets {
            let action = crate::ui::panels::presets::show_preset_panel(
                ctx,
                &mut self.workspace.panels.presets,
                &mut self.preset_panel,
            );
            if let Some(action) = action {
                match action {
                    PresetAction::Save(name) => {
                        let preset = self.capture_preset(name.clone());
                        let filename = name.to_lowercase().replace(' ', "-") + ".json";
                        let path = self.preset_panel.preset_dir.join(&filename);
                        let _ = std::fs::create_dir_all(&self.preset_panel.preset_dir);
                        match crate::preset::save(&preset, &path) {
                            Ok(()) => {
                                self.preset_panel.last_message =
                                    Some(format!("Saved '{}'", name));
                                self.preset_panel.refresh();
                            }
                            Err(e) => {
                                self.preset_panel.last_message =
                                    Some(format!("Error: {e}"));
                            }
                        }
                    }
                    PresetAction::Load(idx) => {
                        self.load_preset_by_index(idx);
                    }
                }
            }
        }

        // Effects panel
        if self.workspace.panels.effects {
            panels::effects_panel::show_effects_panel(
                ctx,
                &mut self.workspace.panels.effects,
                &self.reverb_bus_ui,
                &self.tape_delay_bus_ui,
                &mut self.gravity_state,
                &mut self.manual_gravity,
                &mut self.effects_bypass,
            );
        }

        // Wells panel
        if self.workspace.panels.wells {
            let wells_action = panels::wells_panel::show_wells_panel(
                ctx,
                &mut self.workspace.panels.wells,
                &mut self.gravity_field,
                &mut self.show_well_overlays,
                &mut self.show_hover_tags,
            );
            if let Some(panels::wells_panel::WellsPanelAction::Regenerate(count)) = wells_action {
                self.gravity_field.regenerate(
                    count,
                    self.organism_registry.world_bounds,
                    42,
                );
                // Reinitialize well energy for new well count
                self.well_energy = (0..count).map(|i| WellEnergy::new(i as u32)).collect();
            }
        }

        // Build BioField GPU payload — audio energy + position per organism
        let cell_data = self.organism_registry.build_gpu_payload();

        let biofield_uniforms = BioFieldUniforms {
            viewport:   [screen.width() * dpr, screen.height() * dpr],
            time:       (now - self.start_time) as f32,
            cell_count: cell_data.len() as f32,
        };

        // Central panel with BioField SDF renderer
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(9, 9, 9)))
            .show(ctx, |ui| {
                let (response, painter) = ui.allocate_painter(
                    ui.available_size(),
                    egui::Sense::click_and_drag(),
                );

                let cb = biofield_renderer::create_paint_callback(
                    biofield_uniforms,
                    cell_data,
                    response.rect,
                    self.recorder.is_recording,
                    (screen.width() * dpr) as u32,
                    (screen.height() * dpr) as u32,
                );
                painter.add(cb);

                // --- Well drag/scale interaction ---
                let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
                let shift_held = ctx.input(|i| i.modifiers.shift);
                let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
                let primary_down = ctx.input(|i| i.pointer.primary_down());
                let primary_released = ctx.input(|i| i.pointer.primary_released());

                // On press: determine if we hit a well center (drag) or ring edge (scale)
                if primary_pressed {
                    if let Some(pos) = pointer_pos {
                        for well in self.gravity_field.wells() {
                            let center = egui::pos2(
                                well.position[0] / dpr + response.rect.left(),
                                well.position[1] / dpr + response.rect.top(),
                            );
                            let radius_screen = well.radius / dpr;
                            let dist = pos.distance(center);

                            if shift_held && (dist - radius_screen).abs() < 15.0 {
                                // Shift+click near ring edge → scale
                                self.scaling_well = Some(well.id);
                                break;
                            } else if dist < 20.0 {
                                // Click near center dot → drag
                                self.dragging_well = Some(well.id);
                                break;
                            }
                        }
                    }
                }

                // During drag/scale
                if primary_down {
                    if let Some(pos) = pointer_pos {
                        if let Some(well_id) = self.dragging_well {
                            if let Some(well) = self.gravity_field.well_mut(well_id) {
                                well.position[0] = (pos.x - response.rect.left()) * dpr;
                                well.position[1] = (pos.y - response.rect.top()) * dpr;
                            }
                        }
                        if let Some(well_id) = self.scaling_well {
                            if let Some(well) = self.gravity_field.well_mut(well_id) {
                                let center = egui::pos2(
                                    well.position[0] / dpr + response.rect.left(),
                                    well.position[1] / dpr + response.rect.top(),
                                );
                                let new_radius = pos.distance(center) * dpr;
                                well.radius = new_radius.clamp(150.0, 500.0);
                            }
                        }
                    }
                }

                // On release: clear interaction
                if primary_released {
                    self.dragging_well = None;
                    self.scaling_well = None;
                }

                // --- Gravity well overlays with note-name labels ---
                if self.show_well_overlays {
                    let dimmed = self.effects_bypass.gravity_bypassed;
                    let alpha_ring: u8 = if dimmed { 10 } else { 30 };
                    let alpha_dot: u8 = if dimmed { 30 } else { 100 };
                    let alpha_label: u8 = if dimmed { 50 } else { 160 };

                    for well in self.gravity_field.wells() {
                        let center = egui::pos2(
                            well.position[0] / dpr + response.rect.left(),
                            well.position[1] / dpr + response.rect.top(),
                        );
                        let radius = well.radius / dpr;

                        // HSV hue → RGB with variable alpha
                        let hue_norm = well.hue / 360.0;
                        let hsva = egui::ecolor::Hsva::new(hue_norm, 0.6, 0.8, 1.0);
                        let rgba = egui::Color32::from(hsva);
                        let ring_color = egui::Color32::from_rgba_unmultiplied(rgba.r(), rgba.g(), rgba.b(), alpha_ring);
                        let dot_color = egui::Color32::from_rgba_unmultiplied(rgba.r(), rgba.g(), rgba.b(), alpha_dot);
                        let label_color = egui::Color32::from_rgba_unmultiplied(rgba.r(), rgba.g(), rgba.b(), alpha_label);

                        painter.circle_stroke(
                            center,
                            radius,
                            egui::Stroke::new(1.5, ring_color),
                        );
                        painter.circle_filled(center, 4.0, dot_color);

                        // Note-name label at center
                        painter.text(
                            center + egui::vec2(0.0, -10.0),
                            egui::Align2::CENTER_CENTER,
                            pitch_class_name(well.root_pitch_class),
                            egui::FontId::monospace(11.0),
                            label_color,
                        );
                    }
                }

                // --- Organism hover tags ---
                if self.show_hover_tags {
                if let Some(hover_pos) = pointer_pos {
                    for org in self.organism_registry.organisms() {
                        let org_screen = egui::pos2(
                            org.position[0] / dpr + response.rect.left(),
                            org.position[1] / dpr + response.rect.top(),
                        );
                        let org_radius_screen = org.visual_radius() / dpr;

                        if hover_pos.distance(org_screen) <= org_radius_screen {
                            // Draw colored ID tag in upper-right of bounding box
                            let tag_text = format!("[{}]", org.id);
                            let tag_pos = egui::pos2(
                                org_screen.x + org_radius_screen * 0.7,
                                org_screen.y - org_radius_screen * 0.7,
                            );

                            let hue_norm = org.base_hue.fract().abs();
                            let hsva = egui::ecolor::Hsva::new(hue_norm, 0.7, 0.7, 1.0);
                            let tag_bg = egui::Color32::from(hsva);
                            let tag_bg_semi = egui::Color32::from_rgba_unmultiplied(
                                tag_bg.r(), tag_bg.g(), tag_bg.b(), 180,
                            );

                            let galley = painter.layout_no_wrap(
                                tag_text,
                                egui::FontId::monospace(10.0),
                                egui::Color32::WHITE,
                            );
                            let text_size = galley.size();
                            let tag_rect = egui::Rect::from_min_size(
                                tag_pos - egui::vec2(0.0, text_size.y * 0.5),
                                text_size + egui::vec2(6.0, 2.0),
                            );

                            painter.rect_filled(tag_rect, 3.0, tag_bg_semi);
                            painter.galley(
                                tag_rect.min + egui::vec2(3.0, 1.0),
                                galley,
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }
                } // show_hover_tags
            });

        // Set pending capture flag after render
        if self.recorder.is_recording {
            self.recorder.pending_capture = true;
        }

        ctx.request_repaint();
    }
}
