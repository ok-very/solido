use crate::module::ModuleId;
use crate::modules::raga_module::RagaModule;
use crate::modules::scale_module::ScaleModule;
use crate::modules::tala_module::TalaModule;
use crate::reactor::SeedReactor;
use crate::tuning::gravity_well::pitch_class_name;

/// Module IDs needed by the control panel for downcasting.
pub struct ControlPanelIds {
    pub raga_id: ModuleId,
    pub tala_id: ModuleId,
    pub scale_id: ModuleId,
}

/// Actions returned from the control panel for app.rs to handle.
pub enum ControlPanelAction {
    /// Panic all organisms (stop + note-off all).
    PanicAll,
}

/// Global control panel: transport, key, scale/raga/tala dropdowns.
/// Called from app.rs (not show_workspace) because it needs &mut SeedReactor.
pub fn show_control_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &mut SeedReactor,
    ids: &ControlPanelIds,
    base_key: &mut u8,
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

            // --- Key dropdown ---
            ui.heading("Key");
            let pitch_classes = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
            let mut selected_key = *base_key as usize;
            egui::ComboBox::from_label("Key")
                .selected_text(pitch_class_name(*base_key))
                .show_ui(ui, |ui| {
                    for (i, name) in pitch_classes.iter().enumerate() {
                        ui.selectable_value(&mut selected_key, i, *name);
                    }
                });
            *base_key = selected_key as u8;

            ui.separator();

            // --- Scale dropdown ---
            ui.heading("Scale");
            let (current_scale, scale_list) = {
                if let Some(m) = reactor.module_ref(ids.scale_id) {
                    if let Some(s) = m.as_any().downcast_ref::<ScaleModule>() {
                        (
                            s.current_scale_name().to_string(),
                            s.scale_list()
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

            let mut selected_scale = current_scale.clone();
            egui::ComboBox::from_label("Scale")
                .selected_text(&selected_scale)
                .show_ui(ui, |ui| {
                    for name in &scale_list {
                        ui.selectable_value(&mut selected_scale, name.clone(), name);
                    }
                });
            if selected_scale != current_scale {
                if let Some(m) = reactor.module_mut(ids.scale_id) {
                    m.receive_event(&crate::modules::scale_module::SetScale(selected_scale.clone()));
                }
            }

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
