use crate::module::ModuleId;
use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::key::SolidoKey;
use crate::modules::audio_analysis::AudioAnalysisModule;
use crate::modules::quantizer::QuantizerModule;
use crate::modules::raga_module::RagaModule;
use crate::modules::tala_module::TalaModule;
use crate::modules::voice_module::VoiceModule;
use crate::reactor::SeedReactor;
use crate::recorder::Recorder;
use crate::renderer::font_atlas::FontAtlas;
use crate::renderer::organism_renderer::{self, OrganismRenderResources, Uniforms};
use crate::renderer::shape_atlas::ShapeAtlas;
use crate::substrate::audio::AudioSubstrate;
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
    voice_id: Option<ModuleId>,
    _audio: Option<AudioSubstrate>,
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
        }

        let mut reactor = SeedReactor::new();
        let kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        // Cursor module disabled — auto-wires cursor.x/y [0,1] to voice.amplitude [0,1]
        // causing mouse movement to modulate volume. Will re-enable with proper
        // edge filtering in a future session.
        let analysis_id = reactor.register(Box::new(AudioAnalysisModule::new()));
        let raga_id = reactor.register(Box::new(RagaModule::new()));
        let quantizer_id = reactor.register(Box::new(QuantizerModule::new()));
        let tala_id = reactor.register(Box::new(TalaModule::new()));

        // Audio substrate + VoiceModule — channels go to the module, substrate keeps stream alive
        let (audio, voice_id) = match AudioSubstrate::new() {
            Some((substrate, cmd_tx, analysis_rx)) => {
                let vid = reactor.register(Box::new(VoiceModule::new(cmd_tx, analysis_rx)));
                (Some(substrate), Some(vid))
            }
            None => {
                log::warn!("Audio unavailable — VoiceModule not registered");
                (None, None)
            }
        };

        eprintln!(
            "Reactor initialized: {} modules, {} edges (infra={}), audio={}",
            reactor.module_count(),
            reactor.edge_count(),
            reactor.infra_edge_count(),
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
            voice_id,
            _audio: audio,
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
        egui::Key::Escape => Some(SolidoKey::Escape),
        _ => None,
    }
}

impl eframe::App for SolidoApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Deferred readback from previous frame's capture
        if self.recorder.pending_capture {
            if let Some(rs) = self.render_state.as_ref() {
                let renderer = rs.renderer.read();
                if let Some(resources) = renderer.callback_resources.get::<OrganismRenderResources>() {
                    let now_time = self.last_frame_time.unwrap_or(0.0) as f32;
                    let frame_num = self.recorder.next_frame_number();
                    if let Some(frame) = organism_renderer::read_captured_frame(
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

        // Feed keyboard events to the keyboard module
        let (keys, releases): (Vec<SolidoKey>, Vec<SolidoKey>) = ctx.input(|i| {
            let mut presses = Vec::new();
            let mut releases = Vec::new();
            for event in &i.events {
                if let egui::Event::Key { key, pressed, repeat, .. } = event {
                    // Skip repeats — only handle initial press/release
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

        if let Some(module) = self.reactor.module_mut(self.kbd_id) {
            if let Some(kbd) = module.as_any_mut().downcast_mut::<KeyboardInputModule>() {
                for key in keys {
                    kbd.feed_key(key);
                }
                for key in releases {
                    kbd.feed_key_release(key);
                }
            }
        }

        let screen = ctx.input(|i| i.viewport_rect());

        // Tick the reactor
        self.reactor.tick(delta);

        // --- Workspace UI (header, debug panel, recorder) ---
        let ids = DebugModuleIds {
            kbd_id: self.kbd_id,
            quantizer_id: self.quantizer_id,
            voice_id: self.voice_id,
            analysis_id: self.analysis_id,
        };

        let export_clicked = ui::show_workspace(
            ctx,
            &mut self.workspace,
            &self.reactor,
            &mut self.recorder,
            &ids,
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

        // Viewport
        let dpr = ctx.pixels_per_point();

        // Build uniforms — empty scene (0 organisms)
        let uniforms = Uniforms {
            viewport: [screen.width() * dpr, screen.height() * dpr],
            time: (now - self.start_time) as f32,
            organism_count: 0.0,
            dpr,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };

        // Central panel with SDF renderer
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(9, 9, 9)))
            .show(ctx, |ui| {
                let (response, painter) = ui.allocate_painter(
                    ui.available_size(),
                    egui::Sense::click_and_drag(),
                );

                let cb = organism_renderer::create_paint_callback(
                    uniforms,
                    vec![],  // no organisms
                    vec![],  // no glyphs
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
