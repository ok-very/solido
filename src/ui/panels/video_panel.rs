//! Video perception panel — displays video analysis features and status.
//!
//! Shows the current video source status, 4 CV feature levels with bar meters,
//! and frame count. Future: overlay layer toggles and file picker.

use crate::module::ModuleId;

/// State fed from app.rs each frame.
pub struct VideoPanelState {
    pub active: bool,
    pub frames_processed: u64,
    pub brightness: f32,
    pub warmth: f32,
    pub motion_energy: f32,
    pub edge_density: f32,
}

impl VideoPanelState {
    pub fn new() -> Self {
        Self {
            active: false,
            frames_processed: 0,
            brightness: 0.0,
            warmth: 0.0,
            motion_energy: 0.0,
            edge_density: 0.0,
        }
    }
}

/// Draw the video perception panel.
pub fn show_video_panel(
    ctx: &egui::Context,
    open: &mut bool,
    state: &VideoPanelState,
) {
    egui::Window::new(format!("{} Video", egui_phosphor::regular::EYE))
        .open(open)
        .default_pos([600.0, 400.0])
        .default_width(220.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(16, 16, 22)))
        .show(ctx, |ui| {
            // Status
            ui.horizontal(|ui| {
                let (dot, label) = if state.active {
                    (egui::Color32::from_rgb(60, 200, 100), "Decoding")
                } else {
                    (egui::Color32::from_rgb(80, 80, 80), "No video")
                };
                let (dot_rect, _) = ui.allocate_exact_size(
                    egui::vec2(8.0, 8.0),
                    egui::Sense::hover(),
                );
                ui.painter().circle_filled(dot_rect.center(), 4.0, dot);
                ui.label(egui::RichText::new(label).size(10.0).color(egui::Color32::from_gray(160)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("f:{}", state.frames_processed))
                            .size(9.0)
                            .monospace()
                            .color(egui::Color32::from_gray(80)),
                    );
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Feature meters
            let bar_width = ui.available_width() - 70.0;
            for (label, value, range, color) in [
                ("BRT", state.brightness, (0.0, 1.0), egui::Color32::from_rgb(220, 200, 80)),
                ("WRM", state.warmth, (-1.0, 1.0), egui::Color32::from_rgb(200, 100, 60)),
                ("MOT", state.motion_energy, (0.0, 1.0), egui::Color32::from_rgb(80, 180, 220)),
                ("EDG", state.edge_density, (0.0, 1.0), egui::Color32::from_rgb(160, 200, 160)),
            ] {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .size(9.0)
                            .monospace()
                            .color(egui::Color32::from_gray(120)),
                    );

                    // Normalize value to [0, 1] for bar width
                    let (lo, hi) = range;
                    let norm = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);

                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(bar_width.max(40.0), 10.0),
                        egui::Sense::hover(),
                    );

                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(24, 24, 30));

                        let fill_rect = egui::Rect::from_min_size(
                            rect.min,
                            egui::vec2(rect.width() * norm, rect.height()),
                        );
                        let dim = egui::Color32::from_rgba_unmultiplied(
                            color.r(), color.g(), color.b(), 140,
                        );
                        painter.rect_filled(fill_rect, 1.0, dim);
                    }

                    ui.label(
                        egui::RichText::new(format!("{:.2}", value))
                            .size(9.0)
                            .monospace()
                            .color(egui::Color32::from_gray(100)),
                    );
                });
                ui.add_space(1.0);
            }
        });
}
