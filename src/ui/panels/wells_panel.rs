use crate::tuning::gravity_well::{GravityField, WellEnergy, RegenState};

/// Action returned from the wells panel for app.rs to handle.
pub enum WellsPanelAction {
    /// Regenerate wells with this count.
    Regenerate(usize),
}

/// Dedicated wells panel: count slider, visibility toggles, per-well root/strength/radius + energy.
pub fn show_wells_panel(
    ctx: &egui::Context,
    open: &mut bool,
    gravity_field: &mut GravityField,
    well_energy: &[WellEnergy],
    show_overlays: &mut bool,
    show_hover_tags: &mut bool,
) -> Option<WellsPanelAction> {
    let mut action = None;

    egui::Window::new(format!(
        "{} Wells",
        egui_phosphor::regular::GLOBE_HEMISPHERE_WEST
    ))
    .open(open)
    .default_pos([10.0, 400.0])
    .default_width(240.0)
    .min_width(200.0)
    .resizable(true)
    .collapsible(true)
    .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 24)))
    .show(ctx, |ui| {
        ui.checkbox(show_overlays, "Show well overlays");
        ui.checkbox(show_hover_tags, "Show hover tags");
        ui.separator();

        // Count slider
        let mut count = gravity_field.len();
        if ui
            .add(egui::Slider::new(&mut count, 1..=6).text("Count"))
            .changed()
        {
            action = Some(WellsPanelAction::Regenerate(count));
        }

        ui.separator();

        for (_wi, well) in gravity_field.wells_mut().iter_mut().enumerate() {
            // Find matching energy state
            let energy_state = well_energy.iter().find(|e| e.well_id == well.id);

            let header_color = match energy_state.map(|e| &e.regen_state) {
                Some(RegenState::Healthy) => egui::Color32::from_rgb(80, 200, 100),
                Some(RegenState::Wavering) => egui::Color32::from_rgb(220, 180, 60),
                Some(RegenState::Dormant { .. }) => egui::Color32::from_rgb(160, 60, 60),
                None => egui::Color32::from_gray(140),
            };

            let energy_val = energy_state.map(|e| e.energy).unwrap_or(1.0);
            let state_label = match energy_state.map(|e| &e.regen_state) {
                Some(RegenState::Healthy) => "OK",
                Some(RegenState::Wavering) => "LOW",
                Some(RegenState::Dormant { .. }) => "OFF",
                None => "?",
            };

            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "Well {} {}",
                    well.id,
                    state_label,
                ))
                .strong()
                .size(11.0)
                .color(header_color),
            )
            .default_open(true)
            .show(ui, |ui| {
                // Energy bar
                let bar_w = ui.available_width().min(180.0);
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(bar_w, 10.0), egui::Sense::hover());
                if ui.is_rect_visible(bar_rect) {
                    let painter = ui.painter();
                    painter.rect_filled(
                        bar_rect,
                        2.0,
                        egui::Color32::from_rgb(25, 25, 32),
                    );
                    let fill_w = energy_val.clamp(0.0, 1.0) * bar_rect.width();
                    if fill_w > 0.5 {
                        let bar_color = if energy_val > 0.5 {
                            egui::Color32::from_rgb(60, 180, 80)
                        } else if energy_val > 0.2 {
                            egui::Color32::from_rgb(200, 170, 50)
                        } else {
                            egui::Color32::from_rgb(200, 60, 60)
                        };
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                bar_rect.min,
                                egui::vec2(fill_w, bar_rect.height()),
                            ),
                            2.0,
                            bar_color,
                        );
                    }
                    painter.text(
                        bar_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{:.0}%", energy_val * 100.0),
                        egui::FontId::monospace(8.0),
                        egui::Color32::from_gray(200),
                    );
                }

                ui.add_space(2.0);

                // Strength slider
                ui.add(
                    egui::Slider::new(&mut well.strength, 0.0..=1.0)
                        .text("Str")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );

                // Radius slider
                ui.add(
                    egui::Slider::new(&mut well.radius, 150.0..=500.0).text("Rad"),
                );
            });
        }
    });

    action
}
