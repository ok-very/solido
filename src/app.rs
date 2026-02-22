use crate::recorder::Recorder;
use crate::renderer::font_atlas::FontAtlas;
use crate::renderer::organism_renderer::{self, OrganismRenderResources, Uniforms};
use crate::renderer::shape_atlas::ShapeAtlas;

const FONT_JSON: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL-msdf.json");
const FONT_PNG: &[u8] = include_bytes!("../assets/fonts/Okuda-A5PL-msdf/Okuda-A5PL.png");
const SHAPE_PNG: &[u8] = include_bytes!("../assets/elements/cvx-corner.png");

pub struct SolidoApp {
    last_frame_time: Option<f64>,
    start_time: f64,
    recorder: Recorder,
    render_state: Option<egui_wgpu::RenderState>,
}

impl SolidoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let font_atlas = FontAtlas::load_msdf(FONT_JSON, FONT_PNG);
        let shape_atlas = ShapeAtlas::load_png(SHAPE_PNG);

        let render_state = cc.wgpu_render_state.clone();
        if let Some(rs) = render_state.as_ref() {
            organism_renderer::init_resources(rs, &font_atlas, &shape_atlas);
        }

        Self {
            last_frame_time: None,
            start_time: 0.0,
            recorder: Recorder::new(300),
            render_state,
        }
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
        let _delta = self
            .last_frame_time
            .map(|t| (now - t) as f32)
            .unwrap_or(0.016);
        self.last_frame_time = Some(now);

        if self.start_time == 0.0 {
            self.start_time = now;
        }

        // Viewport
        let screen = ctx.input(|i| i.viewport_rect());
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
