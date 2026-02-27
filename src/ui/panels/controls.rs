use crate::module::ModuleId;
use crate::modules::raga_module::RagaModule;
use crate::modules::tala_module::TalaModule;
use crate::reactor::SeedReactor;
use crate::tuning::gravity_control::GravityState;

/// Module IDs needed by the control panel for downcasting.
pub struct ControlPanelIds {
    pub raga_id: ModuleId,
    pub tala_id: ModuleId,
}

/// Global control panel: gravity sliders, raga/tala dropdowns, tempo.
/// Called from app.rs (not show_workspace) because it needs &mut SeedReactor.
pub fn show_control_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &mut SeedReactor,
    ids: &ControlPanelIds,
    gravity: &mut GravityState,
    manual_gravity: &mut bool,
) {
    egui::Window::new(format!("{} Controls", egui_phosphor::regular::GEAR))
        .open(open)
        .default_pos([10.0, 300.0])
        .default_width(240.0)
        .min_width(180.0)
        .resizable(true)
        .collapsible(true)
        .frame(
            egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(24, 24, 24)),
        )
        .show(ctx, |ui| {
            // --- Gravity Section ---
            ui.heading("Gravity");
            ui.checkbox(manual_gravity, "Manual mode");

            let enabled = *manual_gravity;
            ui.add_enabled(
                enabled,
                egui::Slider::new(&mut gravity.pitch_gravity, 0.0..=1.0).text("Pitch"),
            );
            ui.add_enabled(
                enabled,
                egui::Slider::new(&mut gravity.rhythm_gravity, 0.0..=1.0).text("Rhythm"),
            );
            ui.add_enabled(
                enabled,
                egui::Slider::new(&mut gravity.gamaka_depth, 0.0..=1.0).text("Gamaka"),
            );
            ui.add_enabled(
                enabled,
                egui::Slider::new(&mut gravity.morph_speed, 0.1..=2.0).text("Morph"),
            );

            ui.separator();

            // --- Raga dropdown ---
            ui.heading("Raga");
            let (current_raga, raga_list) = {
                if let Some(m) = reactor.module_ref(ids.raga_id) {
                    if let Some(r) = m.as_any().downcast_ref::<RagaModule>() {
                        (
                            r.current_raga_name().to_string(),
                            r.raga_list()
                                .into_iter()
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        ("?".into(), vec![])
                    }
                } else {
                    ("?".into(), vec![])
                }
            };

            let mut selected_raga = current_raga.clone();
            egui::ComboBox::from_label("Raga")
                .selected_text(&selected_raga)
                .show_ui(ui, |ui| {
                    for name in &raga_list {
                        ui.selectable_value(&mut selected_raga, name.clone(), name);
                    }
                });
            if selected_raga != current_raga {
                if let Some(m) = reactor.module_mut(ids.raga_id) {
                    if let Some(r) = m.as_any_mut().downcast_mut::<RagaModule>() {
                        r.set_raga_by_name(&selected_raga);
                    }
                }
            }

            ui.separator();

            // --- Tala dropdown + tempo ---
            ui.heading("Tala");
            let (current_tala, tala_list, current_tempo) = {
                if let Some(m) = reactor.module_ref(ids.tala_id) {
                    if let Some(t) = m.as_any().downcast_ref::<TalaModule>() {
                        (
                            t.current_tala_name().to_string(),
                            t.tala_list()
                                .into_iter()
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>(),
                            t.tempo_bpm(),
                        )
                    } else {
                        ("?".into(), vec![], 120.0)
                    }
                } else {
                    ("?".into(), vec![], 120.0)
                }
            };

            let mut selected_tala = current_tala.clone();
            egui::ComboBox::from_label("Tala")
                .selected_text(&selected_tala)
                .show_ui(ui, |ui| {
                    for name in &tala_list {
                        ui.selectable_value(&mut selected_tala, name.clone(), name);
                    }
                });
            if selected_tala != current_tala {
                if let Some(m) = reactor.module_mut(ids.tala_id) {
                    if let Some(t) = m.as_any_mut().downcast_mut::<TalaModule>() {
                        t.set_tala_by_name(&selected_tala);
                    }
                }
            }

            let mut tempo = current_tempo as f32;
            if ui
                .add(egui::Slider::new(&mut tempo, 20.0..=300.0).text("Tempo"))
                .changed()
            {
                if let Some(m) = reactor.module_mut(ids.tala_id) {
                    if let Some(t) = m.as_any_mut().downcast_mut::<TalaModule>() {
                        t.set_tempo(tempo as f64);
                    }
                }
            }
        });
}
