use crate::affinity::emotion::ModuleEmotion;
use crate::audio::mixer_state::MixerState;
use crate::audio::voice_bus::BusMeterReport;
use crate::module::ModuleId;
use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::key::SolidoKey;
use crate::modules::audio_analysis::AudioAnalysisModule;
use crate::modules::quantizer::QuantizerModule;
use crate::modules::raga_module::RagaModule;
use crate::modules::tala_module::TalaModule;
use crate::organism::dna::OrganismDna;
use crate::organism::module::OrganismModule;
use crate::organism::registry::OrganismRegistry;
use crate::reactor::SeedReactor;
use crate::recorder::Recorder;
use crate::renderer::blob_renderer::{self, BlobRenderResources, BlobUniforms};
use crate::renderer::font_atlas::FontAtlas;
use crate::renderer::organism_renderer;
use crate::renderer::shape_atlas::ShapeAtlas;
use crate::audio::reverb_bus::ReverbBusHandles;
use crate::substrate::audio::AudioSubstrate;
use crate::substrate::channel::Receiver;
use crate::tuning::gravity_control::GravityState;
use crate::ui::panels::controls::ControlPanelIds;
use crate::dsp::cell::CellRegistry;
use crate::ui::panels::organism_panel::{CellUiState, OrganismPanelState, OrganismUiState, ReverbBusUiState};
use crate::ui::panels::presets::{PresetAction, PresetPanelState};
use crate::ui::{self, DebugModuleIds, WorkspaceState};

const FONT_JSON: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL-msdf.json");
const FONT_PNG: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL.png");
const SHAPE_PNG: &[u8] = include_bytes!("../assets/elements/cvx-corner.png");

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
    _audio: Option<AudioSubstrate>,
    // S05: VoiceBus mixer state + meter receiver
    mixer_state: Option<MixerState>,
    meter_rx: Option<Receiver<BusMeterReport>>,
    // Organism panel: per-cell bypass + param handles for UI
    organism_panel: Option<OrganismPanelState>,
    // Reverb bus handles for UI
    _reverb_bus_handles: Option<ReverbBusHandles>,
    // S09: Organism simulation + blob rendering
    organism_registry: OrganismRegistry,
    gravity_state: GravityState,
    /// Aggregate emotion for gravity state (averaged across modules).
    aggregate_emotion: ModuleEmotion,
    beat_phase: f32,
    /// When true, gravity sliders are manual; when false, emotion-driven.
    manual_gravity: bool,
    /// Preset panel state.
    preset_panel: PresetPanelState,
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
            // Initialize both renderers: old L-shape and new blob
            organism_renderer::init_resources(rs, &font_atlas, &shape_atlas);
            blob_renderer::init_resources(rs, &font_atlas, &shape_atlas);
        }

        let mut reactor = SeedReactor::new();
        let kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let analysis_id = reactor.register(Box::new(AudioAnalysisModule::new()));
        let raga_id = reactor.register(Box::new(RagaModule::new()));
        let quantizer_id = reactor.register(Box::new(QuantizerModule::new()));
        let tala_id = reactor.register(Box::new(TalaModule::new()));

        // Load organism DNA presets
        let dna_paths = [
            "assets/dna/dron-alpha.json",
            "assets/dna/hoso-malabar.json",
            "assets/dna/spgl-kepler.json",
        ];
        let dna_list: Vec<OrganismDna> = dna_paths
            .iter()
            .filter_map(|p| {
                crate::organism::dna_io::load(std::path::Path::new(p))
                    .map_err(|e| log::warn!("Failed to load DNA {p}: {e}"))
                    .ok()
            })
            .collect();

        // Audio substrate + OrganismDsp
        // Organisms are built inside AudioSubstrate at the discovered sample rate.
        let mut organism_registry = OrganismRegistry::new();
        organism_registry.world_bounds = [0.0, 0.0, 1200.0, 700.0];

        let (audio, mixer_state, meter_rx, organism_panel, reverb_bus_handles) = match AudioSubstrate::new(&dna_list) {
            Some((substrate, org_endpoints, bus_handles, reverb_handles, meter_rx)) => {
                // S13: Register OrganismModules with reactor + spawn visual state
                let initial_positions = [
                    [400.0, 350.0],
                    [700.0, 300.0],
                    [550.0, 450.0],
                ];
                let initial_velocities = [
                    [15.0, 8.0],
                    [-10.0, 12.0],
                    [5.0, -5.0],
                ];

                // Step 1: Clone cell bypass + param Shared handles from &org_endpoints
                // (borrow pass — endpoints not consumed yet).
                let cell_registry = CellRegistry::new();
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

                        // Look up param ranges from the registry
                        let param_ranges: Vec<(String, f32, f32)> = params
                            .iter()
                            .map(|(name, _)| {
                                let (min, max) = cell_registry
                                    .param_range(&cell_dna.cell_type, name)
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

                // Step 2: Spawn organisms, consume endpoints, collect org_ids.
                let mut org_ids: Vec<u32> = Vec::new();
                for (i, (dna, endpoint)) in dna_list.iter().zip(org_endpoints).enumerate() {
                    let pos = initial_positions.get(i).copied().unwrap_or([400.0, 350.0]);
                    let vel = initial_velocities.get(i).copied().unwrap_or([0.0, 0.0]);

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
                        org.pseudopod_gain = dna.body.pseudopod_gain;
                        org.extension_speed = dna.body.extension_speed;
                        org.retraction_speed = dna.body.retraction_speed;
                        org.velocity = vel;
                        org.energy = 0.7;
                        org.arousal = dna.emotion.base_arousal;
                        org.valence = dna.emotion.base_valence;
                        org.species = dna.species.clone();
                        org.interaction_rules = dna.physics.interaction_rules.clone();
                    }

                    // Register OrganismModule with reactor (Organism tier → AffinityGraph)
                    let module = OrganismModule::new(
                        dna.clone(),
                        endpoint.shared_handles,
                        endpoint.analysis_rx,
                        endpoint.cmd_tx,
                        org_id,
                    );
                    let _mod_id = reactor.register(Box::new(module));
                    eprintln!(
                        "  organism: {} (species={}, org_id={})",
                        dna.name, dna.species, org_id
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

                    panel_organisms.push(OrganismUiState {
                        name: dna.name.clone(),
                        species: dna.species.clone(),
                        hue: dna.render.hue,
                        organism_id: org_ids[i],
                        mixer_mute,
                        mixer_gain,
                        cells,
                        shape_id,
                        reverb_send,
                    });
                }

                // Build reverb bus UI state
                let reverb_bus_ui = reverb_handles.as_ref().map(|rh| {
                    ReverbBusUiState {
                        reverb_type: rh.reverb_type.clone(),
                        return_level: rh.return_level.clone(),
                        params: rh.params.clone(),
                    }
                });

                let organism_panel = OrganismPanelState {
                    organisms: panel_organisms,
                    reverb_bus: reverb_bus_ui,
                };

                let mixer_state = MixerState::new(bus_handles);
                (Some(substrate), Some(mixer_state), Some(meter_rx), Some(organism_panel), reverb_handles)
            }
            None => {
                log::warn!("Audio unavailable");
                (None, None, None, None, None)
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
            _audio: audio,
            mixer_state,
            meter_rx,
            organism_panel,
            _reverb_bus_handles: reverb_bus_handles,
            organism_registry,
            gravity_state: GravityState::neutral(),
            aggregate_emotion: ModuleEmotion::new(5.0),
            beat_phase: 0.0,
            manual_gravity: false,
            preset_panel: PresetPanelState::new(std::path::PathBuf::from("assets/presets")),
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
            if let Some(r) = m.as_any_mut().downcast_mut::<RagaModule>() {
                r.set_raga_by_name(&preset.raga);
            }
        }
        if let Some(m) = self.reactor.module_mut(self.tala_id) {
            if let Some(t) = m.as_any_mut().downcast_mut::<TalaModule>() {
                t.set_tala_by_name(&preset.tala);
                t.set_tempo(preset.tempo_bpm);
            }
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
        egui::Key::Escape => Some(SolidoKey::Escape),
        egui::Key::F1 => Some(SolidoKey::F1),
        egui::Key::F2 => Some(SolidoKey::F2),
        egui::Key::F3 => Some(SolidoKey::F3),
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
        // Deferred readback from previous frame's capture (blob renderer)
        if self.recorder.pending_capture {
            if let Some(rs) = self.render_state.as_ref() {
                let renderer = rs.renderer.read();
                if let Some(resources) = renderer.callback_resources.get::<BlobRenderResources>() {
                    let now_time = self.last_frame_time.unwrap_or(0.0) as f32;
                    let frame_num = self.recorder.next_frame_number();
                    if let Some(frame) = blob_renderer::read_captured_frame(
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
                            eprintln!("[kbd] press: {:?}", sk);
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
            if let Some(kbd) = module.as_any_mut().downcast_mut::<KeyboardInputModule>() {
                if !ctrl_held {
                    for key in keys_for_module {
                        kbd.feed_key(key);
                    }
                }
                for key in releases {
                    kbd.feed_key_release(key);
                }
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

        // Tick the reactor (module signal routing + learning)
        self.reactor.tick(delta);

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

        // S09b: Bridge per-organism emotion from reactor → visual state (AD-2)
        for (&mod_id, module) in self.reactor.modules_iter() {
            if let Some(org_mod) = module.as_any().downcast_ref::<OrganismModule>() {
                if let Some(emotion) = self.reactor.graph.emotions.get(&mod_id) {
                    if let Some(org) = self.organism_registry.get_mut(org_mod.organism_id()) {
                        let alpha = (delta * 3.0).min(1.0);
                        org.arousal += (emotion.arousal - org.arousal) * alpha;
                        org.valence += (emotion.valence - org.valence) * alpha;
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
        self.organism_registry.update_glob_groups(&affinities, 0.65);

        self.organism_registry.tick(delta);

        // --- Workspace UI (header, status bar, debug panel, mixer, organisms, ledger, recorder) ---
        let ids = DebugModuleIds {
            kbd_id: self.kbd_id,
            quantizer_id: self.quantizer_id,
            analysis_id: self.analysis_id,
            raga_id: self.raga_id,
            tala_id: self.tala_id,
        };

        let export_clicked = ui::show_workspace(
            ctx,
            &mut self.workspace,
            &self.reactor,
            &mut self.recorder,
            &ids,
            self.mixer_state.as_mut(),
            self.organism_panel.as_ref(),
            &self.gravity_state,
            self.beat_phase,
        );

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
            };
            crate::ui::panels::controls::show_control_panel(
                ctx,
                &mut self.workspace.panels.controls,
                &mut self.reactor,
                &ctrl_ids,
                &mut self.gravity_state,
                &mut self.manual_gravity,
            );
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

        // S09b: Build blob GPU payload — per-organism emotion drives visuals (AD-1)
        let (org_gpu, lobe_gpu) = self.organism_registry.build_gpu_payload(self.beat_phase);

        let blob_uniforms = BlobUniforms {
            viewport: [screen.width() * dpr, screen.height() * dpr],
            time: (now - self.start_time) as f32,
            organism_count: org_gpu.len() as f32,
            dpr,
            beat_phase: self.beat_phase,
            gravity_strength: self.gravity_state.pitch_gravity,
            cross_smin_k: 1.5,  // metaball threshold (additive potential field)
        };

        // Central panel with blob SDF renderer
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(9, 9, 9)))
            .show(ctx, |ui| {
                let (response, painter) = ui.allocate_painter(
                    ui.available_size(),
                    egui::Sense::click_and_drag(),
                );

                let cb = blob_renderer::create_paint_callback(
                    blob_uniforms,
                    org_gpu,
                    lobe_gpu,
                    vec![],  // no glyphs yet
                    response.rect,
                    self.recorder.is_recording,
                    (screen.width() * dpr) as u32,
                    (screen.height() * dpr) as u32,
                );
                painter.add(cb);
            });

        // Set pending capture flag after render
        if self.recorder.is_recording {
            self.recorder.pending_capture = true;
        }

        ctx.request_repaint();
    }
}
