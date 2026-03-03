use crate::organism::dna::OrganismDna;
use crate::ui::panels::organism_panel::hue_to_color32;

pub enum SpawnAction {
    Spawn(OrganismDna),
}

/// Show the "Spawn Organism" floating window.
///
/// Returns a `SpawnAction` if the user clicks "Spawn" on a preset row.
/// `available` is the full list of DNA presets (both active and inactive).
pub fn show_spawn_panel(
    ctx: &egui::Context,
    available: &[OrganismDna],
    open: &mut bool,
) -> Option<SpawnAction> {
    let mut action = None;
    egui::Window::new(format!(
        "{} Spawn Organism",
        egui_phosphor::regular::PLUS_CIRCLE
    ))
    .open(open)
    .resizable(false)
    .default_pos([300.0, 200.0])
    .default_width(300.0)
    .show(ctx, |ui| {
        if available.is_empty() {
            ui.label(egui::RichText::new("No DNA presets loaded.").weak());
            return;
        }
        for dna in available {
            ui.horizontal(|ui| {
                // Hue swatch
                let color = hue_to_color32(dna.render.hue);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);

                // Species + name
                ui.colored_label(color, format!("[{}]", dna.species));
                ui.label(&dna.name);

                // Spawn button right-aligned
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Spawn").clicked() {
                        action = Some(SpawnAction::Spawn(dna.clone()));
                    }
                });
            });
        }
    });
    action
}
