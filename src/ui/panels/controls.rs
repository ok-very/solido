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

/// Actions returned from the control panel for app.rs to handle.
pub enum ControlPanelAction {
    /// Panic all organisms (stop + note-off all).
    PanicAll,
}

/// Global control panel: transport, gravity sliders, raga/tala dropdowns.
/// Called from app.rs (not show_workspace) because it needs &mut SeedReactor.
pub fn show_control_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &mut SeedReactor,
    ids: &ControlPanelIds,
    gravity: &mut GravityState,
    manual_gravity: &mut bool,
) -> Option<ControlPanelAction> {
    let mut action = None;

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
            // --- Transport Section ---
            ui.heading("Transport");

            ui.horizontal(|ui| {
                let playing = reactor.clock.is_playing();
                let play_label = if playing { "\u{23F8} Pause" } else { "\u{25B6} Play" };
                if ui.button(play_label).clicked() {
                    reactor.clock.playing.set(if playing { 0.0 } else { 1.0 });
                }
                if ui.button("\u{23F9} Stop").clicked() {
                    reactor.clock.playing.set(0.0);
                    action = Some(ControlPanelAction::PanicAll);
                }
            });

            // BPM slider — reads/writes GlobalClock directly
            let mut bpm = reactor.clock.bpm_value();
            if ui
                .add(egui::Slider::new(&mut bpm, 20.0..=300.0).text("BPM"))
                .changed()
            {
                reactor.clock.bpm.set(bpm);
            }

            ui.separator();

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
                    m.receive_event(&crate::modules::raga_module::SetRaga(selected_raga.clone()));
                }
            }

            ui.separator();

            // --- Tala dropdown ---
            ui.heading("Tala");
            let (current_tala, tala_list) = {
                if let Some(m) = reactor.module_ref(ids.tala_id) {
                    if let Some(t) = m.as_any().downcast_ref::<TalaModule>() {
                        (
                            t.current_tala_name().to_string(),
                            t.tala_list()
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
                    m.receive_event(&crate::modules::tala_module::SetTala(selected_tala.clone()));
                }
            }
        });

    action
}
