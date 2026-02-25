use crate::module::schema::ModuleTier;
use crate::reactor::SeedReactor;

pub fn show(ui: &mut egui::Ui, reactor: &SeedReactor) {
    // Infrastructure modules — fixed routing, no emotions
    let mut infra_modules: Vec<_> = reactor
        .schemas()
        .iter()
        .filter(|(_, s)| s.tier == ModuleTier::Infrastructure)
        .collect();
    infra_modules.sort_by_key(|(&id, _)| id);

    if !infra_modules.is_empty() {
        ui.label(
            egui::RichText::new("Infrastructure")
                .color(egui::Color32::from_rgb(140, 140, 160))
                .strong(),
        );
        ui.separator();

        for (&mod_id, schema) in &infra_modules {
            ui.horizontal(|ui| {
                ui.label(format!("{:<16}", schema.name));
                ui.colored_label(
                    egui::Color32::from_rgb(140, 140, 160),
                    format!(
                        "edges={}",
                        reactor.infra_router.edge_count()
                    ),
                );
            });
            let _ = mod_id; // used only for sort key
        }

        ui.add_space(8.0);
    }

    // Organism modules — AffinityGraph with emotions
    let mut emotions: Vec<_> = reactor.graph.emotions.iter().collect();
    emotions.sort_by_key(|(&id, _)| id);

    if !emotions.is_empty() {
        ui.label(
            egui::RichText::new("Organisms")
                .color(egui::Color32::from_rgb(200, 160, 80))
                .strong(),
        );
        ui.separator();

        for (&mod_id, emotion) in &emotions {
            let mod_name = reactor
                .schemas()
                .get(&mod_id)
                .map(|s| s.name.as_str())
                .unwrap_or("?");

            let v_color = if emotion.valence > 0.3 {
                egui::Color32::from_rgb(80, 200, 80)
            } else if emotion.valence > -0.3 {
                egui::Color32::from_rgb(200, 200, 80)
            } else {
                egui::Color32::from_rgb(200, 80, 80)
            };

            ui.horizontal(|ui| {
                ui.label(format!("{:<16}", mod_name));
                ui.colored_label(v_color, format!("v={:+.2}", emotion.valence));
                ui.label(format!(
                    "a={:.2} act={:.1}",
                    emotion.arousal, emotion.activity
                ));
            });
        }
    }

    if infra_modules.is_empty() && emotions.is_empty() {
        ui.label("No modules registered");
    }
}
