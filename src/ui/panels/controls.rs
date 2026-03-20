use crate::module::ModuleId;
use crate::modules::raga_module::RagaModule;
use crate::modules::scale_module::ScaleModule;
use crate::reactor::SeedReactor;
use crate::ui::widgets::circle_of_fifths::{self, CircleOfFifthsState, MicroRingData};
use crate::ui::panels::organism_panel::hue_to_color32;

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

/// Global control panel: transport, circle of fifths, scale/raga chips, tala dropdown.
pub fn show_control_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &mut SeedReactor,
    ids: &ControlPanelIds,
    base_key: &mut u8,
) -> Option<ControlPanelAction> {
    let mut action = None;

    // Read scale state before the window (immutable borrow)
    let (current_scale_name, current_scale_idx, scale_weights, scale_hue, scale_groups_data) = {
        if let Some(m) = reactor.module_ref(ids.scale_id) {
            if let Some(s) = m.as_any().downcast_ref::<ScaleModule>() {
                let idx = s.current_index();
                let reg = s.registry();
                let mode = reg.get_by_index(idx).unwrap();
                let groups: Vec<(&str, Vec<(usize, String, String, f32)>)> = reg.groups().into_iter().map(|g| {
                    let scales = reg.scales_in_group(g).into_iter().map(|(i, sm)| {
                        (i, sm.name.to_string(), sm.short.to_string(), sm.hue)
                    }).collect();
                    (g, scales)
                }).collect();
                (mode.name.to_string(), idx, mode.gravity_weights, mode.hue, groups)
            } else {
                (String::new(), 0, [0.0; 12], 0.0, vec![])
            }
        } else {
            (String::new(), 0, [0.0; 12], 0.0, vec![])
        }
    };

    // Read raga state + micro-tuning for the circle of fifths
    let (current_raga, raga_list, micro_data) = {
        if let Some(m) = reactor.module_ref(ids.raga_id) {
            if let Some(r) = m.as_any().downcast_ref::<RagaModule>() {
                let (cents, weights, count) = r.micro_tuning();
                let micro = if count > 0 {
                    Some(MicroRingData {
                        cents,
                        weights,
                        count,
                        raga_hue: r.current_raga().hue,
                    })
                } else {
                    None
                };
                (
                    r.current_raga_name().to_string(),
                    r.raga_list().into_iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    micro,
                )
            } else {
                ("?".into(), vec![], None)
            }
        } else {
            ("?".into(), vec![], None)
        }
    };

    // Pending mutations (collected during UI, applied after)
    let mut pending_scale: Option<String> = None;
    let mut pending_raga: Option<String> = None;

    egui::Window::new(format!("{} Controls", egui_phosphor::regular::GEAR))
        .open(open)
        .default_pos([10.0, 300.0])
        .default_width(300.0)
        .min_width(260.0)
        .resizable(true)
        .collapsible(true)
        .frame(
            egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 24)),
        )
        .show(ctx, |ui| {
            // ── Transport ──
            ui.horizontal(|ui| {
                let playing = reactor.clock.is_playing();
                let play_icon = if playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                if ui.button(egui::RichText::new(play_icon).size(16.0)).clicked() {
                    reactor.clock.playing.set(if playing { 0.0 } else { 1.0 });
                }
                if ui.button(egui::RichText::new(egui_phosphor::regular::STOP).size(16.0)).clicked() {
                    reactor.clock.playing.set(0.0);
                    action = Some(ControlPanelAction::PanicAll);
                }
                let mut bpm = reactor.clock.bpm_value();
                if ui.add(
                    egui::Slider::new(&mut bpm, 20.0..=300.0)
                        .text("BPM")
                        .max_decimals(0)
                ).changed() {
                    reactor.clock.bpm.set(bpm);
                }
            });

            ui.separator();

            // ── Circle of Fifths ──
            let mut cof_state = CircleOfFifthsState {
                key: *base_key,
                gravity_weights: scale_weights,
                scale_hue,
                micro_tuning: micro_data,
            };
            let cof_resp = circle_of_fifths::show_circle_of_fifths(ui, &mut cof_state);
            if cof_resp.key_changed {
                *base_key = cof_resp.key;
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // ── Scale Chips ──
            for (group_name, scales) in &scale_groups_data {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(*group_name)
                            .size(9.0)
                            .color(egui::Color32::from_gray(80)),
                    );
                    for (idx, name, short, hue) in scales {
                        let is_active = *idx == current_scale_idx;
                        let fill = if is_active {
                            hue_to_color32(*hue / 360.0)
                        } else {
                            egui::Color32::from_gray(36)
                        };
                        let text_color = if is_active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_gray(140)
                        };
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(short.as_str()).size(10.0).color(text_color),
                            )
                            .fill(fill)
                            .rounding(10.0)
                            .min_size(egui::vec2(32.0, 16.0)),
                        );
                        if btn.clicked() {
                            pending_scale = Some(name.clone());
                        }
                        btn.on_hover_text(name.as_str());
                    }
                });
            }

            ui.add_space(2.0);

            // ── Raga Chips ──
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Raga")
                        .size(9.0)
                        .color(egui::Color32::from_gray(80)),
                );
                for name in &raga_list {
                    let is_active = *name == current_raga;
                    let fill = if is_active {
                        egui::Color32::from_rgb(140, 80, 40)
                    } else {
                        egui::Color32::from_gray(36)
                    };
                    let text_color = if is_active {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(140)
                    };
                    // Short label: first 3 chars capitalized
                    let short: String = name.chars().take(3).collect::<String>();
                    let short = format!("{}{}", short[..1].to_uppercase(), &short[1..]);
                    let btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(&short).size(10.0).color(text_color),
                        )
                        .fill(fill)
                        .rounding(10.0)
                        .min_size(egui::vec2(32.0, 16.0)),
                    );
                    if btn.clicked() {
                        pending_raga = Some(name.clone());
                    }
                    btn.on_hover_text(name.as_str());
                }
            });

        });

    // ── Apply mutations ──
    if let Some(name) = pending_scale {
        if let Some(m) = reactor.module_mut(ids.scale_id) {
            m.receive_event(&crate::modules::scale_module::SetScale(name));
        }
    }
    if let Some(name) = pending_raga {
        if let Some(m) = reactor.module_mut(ids.raga_id) {
            m.receive_event(&crate::modules::raga_module::SetRaga(name));
        }
    }

    action
}
