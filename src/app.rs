use crate::module::ModuleId;
use crate::modules::cursor_input::CursorInputModule;
use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::key::SolidoKey;
use crate::modules::audio_analysis::AudioAnalysisModule;
use crate::reactor::SeedReactor;
use crate::recorder::Recorder;
use crate::renderer::font_atlas::FontAtlas;
use crate::renderer::organism_renderer::{self, OrganismRenderResources, Uniforms};
use crate::renderer::shape_atlas::ShapeAtlas;
use crate::substrate::audio::AudioSubstrate;

const FONT_JSON: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL-msdf.json");
const FONT_PNG: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL.png");
const SHAPE_PNG: &[u8] = include_bytes!("../assets/elements/cvx-corner.png");

pub struct SolidoApp {
    last_frame_time: Option<f64>,
    start_time: f64,
    recorder: Recorder,
    render_state: Option<egui_wgpu::RenderState>,
    reactor: SeedReactor,
    kbd_id: ModuleId,
    cursor_id: ModuleId,
    analysis_id: ModuleId,
    audio: Option<AudioSubstrate>,
}

impl SolidoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let font_atlas = FontAtlas::load_msdf(FONT_JSON, FONT_PNG);
        let shape_atlas = ShapeAtlas::load_png(SHAPE_PNG);

        let render_state = cc.wgpu_render_state.clone();
        if let Some(rs) = render_state.as_ref() {
            organism_renderer::init_resources(rs, &font_atlas, &shape_atlas);
        }

        let mut reactor = SeedReactor::new();
        let kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let cursor_id = reactor.register(Box::new(CursorInputModule::new()));
        let analysis_id = reactor.register(Box::new(AudioAnalysisModule::new()));

        let audio = AudioSubstrate::new();

        log::info!(
            "Reactor initialized: {} modules, {} edges, audio={}",
            reactor.module_count(),
            reactor.edge_count(),
            audio.is_some(),
        );

        Self {
            last_frame_time: None,
            start_time: 0.0,
            recorder: Recorder::new(300),
            render_state,
            reactor,
            kbd_id,
            cursor_id,
            analysis_id,
            audio,
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
        let keys: Vec<SolidoKey> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        egui_key_to_solido(*key)
                    } else {
                        None
                    }
                })
                .collect()
        });

        if let Some(module) = self.reactor.module_mut(self.kbd_id) {
            if let Some(kbd) = module.as_any_mut().downcast_mut::<KeyboardInputModule>() {
                for key in keys {
                    kbd.feed_key(key);
                }
            }
        }

        // Feed cursor position to the cursor module
        let cursor_pos = ctx.input(|i| i.pointer.hover_pos());
        let screen = ctx.input(|i| i.viewport_rect());
        if let Some(pos) = cursor_pos {
            let nx = pos.x / screen.width();
            let ny = pos.y / screen.height();
            if let Some(module) = self.reactor.module_mut(self.cursor_id) {
                if let Some(cursor) = module.as_any_mut().downcast_mut::<CursorInputModule>() {
                    cursor.feed_position(nx, ny);
                }
            }
        }

        // Feed audio analysis from the substrate
        if let Some(ref mut audio) = self.audio {
            if let Some(analysis) = audio.latest_analysis() {
                if let Some(module) = self.reactor.module_mut(self.analysis_id) {
                    if let Some(am) = module.as_any_mut().downcast_mut::<AudioAnalysisModule>() {
                        am.feed_metrics(analysis.rms, analysis.peak);
                    }
                }
            }
        }

        // Tick the reactor
        self.reactor.tick(delta);

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

        // Timeline panel
        if self.recorder.ui(ctx) {
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

        ctx.request_repaint();
    }
}
