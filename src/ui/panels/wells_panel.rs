use crate::tuning::gravity_well::{pitch_class_name, GravityField};

/// Action returned from the wells panel for app.rs to handle.
pub enum WellsPanelAction {
    /// Regenerate wells with this count.
    Regenerate(usize),
}

/// Dedicated wells panel: count slider, visibility toggles, per-well root/strength/radius.
pub fn show_wells_panel(
    ctx: &egui::Context,
    open: &mut bool,
    gravity_field: &mut GravityField,
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
    .default_width(220.0)
    .min_width(180.0)
    .resizable(true)
    .collapsible(true)
    .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 24)))
    .show(ctx, |ui| {
        // Visibility toggles
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

        // Per-well rows
        let pitch_classes: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];

        for well in gravity_field.wells_mut() {
            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "Well {} [{}]",
                    well.id,
                    pitch_class_name(well.root_pitch_class)
                ))
                .strong()
                .size(12.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                // Root pitch class dropdown
                let mut selected = well.root_pitch_class as usize;
                let current_name = pitch_classes[selected % 12];
                egui::ComboBox::from_id_salt(format!("well_root_{}", well.id))
                    .selected_text(current_name)
                    .width(60.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in pitch_classes.iter().enumerate() {
                            ui.selectable_value(&mut selected, i, *name);
                        }
                    });
                well.root_pitch_class = selected as u8;

                // Strength slider
                ui.add(
                    egui::Slider::new(&mut well.strength, 0.0..=1.0)
                        .text("Strength")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );

                // Radius slider
                ui.add(
                    egui::Slider::new(&mut well.radius, 150.0..=500.0).text("Radius"),
                );
            });
        }
    });

    action
}
