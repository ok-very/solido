use crate::dsp::shared::Shared;
use crate::tuning::gravity_control::GravityState;

/// Reverb bus UI state (global, not per-organism).
pub struct ReverbBusUiState {
    pub reverb_type: String,
    pub return_level: Shared,
    pub params: Vec<(String, Shared)>,
}

/// Tape delay bus UI state (global, not per-organism).
pub struct TapeDelayBusUiState {
    pub return_level: Shared,
    /// (name, handle) — time, feedback, hf_damp.
    pub params: Vec<(String, Shared)>,
}

/// Per-effect bypass state, tracked on app.
pub struct EffectsBypassState {
    pub gravity_bypassed: bool,
    pub reverb_bypassed: bool,
    pub reverb_stored_return: f32,
    pub tape_bypassed: bool,
    pub tape_stored_return: f32,
}

impl Default for EffectsBypassState {
    fn default() -> Self {
        Self {
            gravity_bypassed: false,
            reverb_bypassed: false,
            reverb_stored_return: 0.5,
            tape_bypassed: false,
            tape_stored_return: 0.5,
        }
    }
}

/// Valid UI range for a tape delay param.
pub fn tape_delay_param_range(name: &str) -> (f32, f32) {
    match name {
        "time" => (0.05, 2.0),
        "feedback" => (0.0, 0.95),
        _ => (0.0, 1.0),
    }
}

/// Horizontal effects panel: Gravity | Reverb | Tape Delay.
pub fn show_effects_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reverb: &Option<ReverbBusUiState>,
    tape_delay: &Option<TapeDelayBusUiState>,
    gravity: &mut GravityState,
    manual_gravity: &mut bool,
    bypass: &mut EffectsBypassState,
) {
    egui::Window::new(format!(
        "{} Effects",
        egui_phosphor::regular::WAVEFORM
    ))
    .open(open)
    .default_pos([300.0, 60.0])
    .default_width(660.0)
    .min_width(400.0)
    .resizable(true)
    .collapsible(true)
    .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(22, 22, 22)))
    .show(ctx, |ui| {
        ui.columns(3, |cols| {
            // --- Gravity column ---
            show_gravity_slot(&mut cols[0], gravity, manual_gravity, bypass);

            // --- Reverb column ---
            show_reverb_slot(&mut cols[1], reverb, bypass);

            // --- Tape Delay column ---
            show_tape_delay_slot(&mut cols[2], tape_delay, bypass);
        });
    });
}

fn show_gravity_slot(
    ui: &mut egui::Ui,
    gravity: &mut GravityState,
    manual_gravity: &mut bool,
    bypass: &mut EffectsBypassState,
) {
    ui.vertical(|ui| {
        // Header with bypass toggle
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Gravity")
                    .strong()
                    .size(13.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bypass_label = if bypass.gravity_bypassed { "OFF" } else { "ON" };
                let bypass_color = if bypass.gravity_bypassed {
                    egui::Color32::from_rgb(120, 60, 60)
                } else {
                    egui::Color32::from_rgb(60, 120, 60)
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(bypass_label).size(10.0).color(bypass_color),
                    ).small())
                    .clicked()
                {
                    bypass.gravity_bypassed = !bypass.gravity_bypassed;
                    if bypass.gravity_bypassed {
                        *manual_gravity = true;
                        gravity.pitch_gravity = 0.0;
                        gravity.rhythm_gravity = 0.0;
                        gravity.gamaka_depth = 0.0;
                        gravity.morph_speed = 0.1;
                    }
                }
            });
        });

        ui.separator();

        let enabled = !bypass.gravity_bypassed;
        ui.add_enabled(enabled, egui::Checkbox::new(manual_gravity, "Manual mode"));
        ui.add_enabled(
            enabled && *manual_gravity,
            egui::Slider::new(&mut gravity.pitch_gravity, 0.0..=1.0).text("Pitch"),
        );
        ui.add_enabled(
            enabled && *manual_gravity,
            egui::Slider::new(&mut gravity.rhythm_gravity, 0.0..=1.0).text("Rhythm"),
        );
        ui.add_enabled(
            enabled && *manual_gravity,
            egui::Slider::new(&mut gravity.gamaka_depth, 0.0..=1.0).text("Gamaka"),
        );
        ui.add_enabled(
            enabled && *manual_gravity,
            egui::Slider::new(&mut gravity.morph_speed, 0.1..=2.0).text("Morph"),
        );
    });
}

fn show_reverb_slot(
    ui: &mut egui::Ui,
    reverb: &Option<ReverbBusUiState>,
    bypass: &mut EffectsBypassState,
) {
    ui.vertical(|ui| {
        let type_label = reverb
            .as_ref()
            .map(|r| r.reverb_type.as_str())
            .unwrap_or("plate");

        // Header with bypass toggle
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} Reverb [{}]",
                    egui_phosphor::regular::SPARKLE,
                    type_label
                ))
                .strong()
                .size(13.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bypass_label = if bypass.reverb_bypassed { "OFF" } else { "ON" };
                let bypass_color = if bypass.reverb_bypassed {
                    egui::Color32::from_rgb(120, 60, 60)
                } else {
                    egui::Color32::from_rgb(60, 120, 60)
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(bypass_label).size(10.0).color(bypass_color),
                    ).small())
                    .clicked()
                {
                    if let Some(ref bus) = reverb {
                        bypass.reverb_bypassed = !bypass.reverb_bypassed;
                        if bypass.reverb_bypassed {
                            bypass.reverb_stored_return = bus.return_level.value();
                            bus.return_level.set(0.0);
                        } else {
                            bus.return_level.set(bypass.reverb_stored_return);
                        }
                    }
                }
            });
        });

        ui.separator();

        if let Some(ref bus) = reverb {
            let enabled = !bypass.reverb_bypassed;

            // Return level
            let mut ret = bus.return_level.value();
            if ui
                .add_enabled(
                    enabled,
                    egui::Slider::new(&mut ret, 0.0..=1.0)
                        .text("return")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                bus.return_level.set(ret);
            }

            // Reverb params (size, dcy, damp)
            for (name, handle) in &bus.params {
                let mut val = handle.value();
                if ui
                    .add_enabled(enabled, egui::Slider::new(&mut val, 0.0..=1.0).text(name))
                    .changed()
                {
                    handle.set(val);
                }
            }
        } else {
            ui.label(egui::RichText::new("No reverb bus").weak().small());
        }
    });
}

fn show_tape_delay_slot(
    ui: &mut egui::Ui,
    tape_delay: &Option<TapeDelayBusUiState>,
    bypass: &mut EffectsBypassState,
) {
    ui.vertical(|ui| {
        // Header with bypass toggle
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} Tape Delay",
                    egui_phosphor::regular::WAVES,
                ))
                .strong()
                .size(13.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bypass_label = if bypass.tape_bypassed { "OFF" } else { "ON" };
                let bypass_color = if bypass.tape_bypassed {
                    egui::Color32::from_rgb(120, 60, 60)
                } else {
                    egui::Color32::from_rgb(60, 120, 60)
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(bypass_label).size(10.0).color(bypass_color),
                    ).small())
                    .clicked()
                {
                    if let Some(ref bus) = tape_delay {
                        bypass.tape_bypassed = !bypass.tape_bypassed;
                        if bypass.tape_bypassed {
                            bypass.tape_stored_return = bus.return_level.value();
                            bus.return_level.set(0.0);
                        } else {
                            bus.return_level.set(bypass.tape_stored_return);
                        }
                    }
                }
            });
        });

        ui.separator();

        if let Some(ref bus) = tape_delay {
            let enabled = !bypass.tape_bypassed;

            // Return level
            let mut ret = bus.return_level.value();
            if ui
                .add_enabled(
                    enabled,
                    egui::Slider::new(&mut ret, 0.0..=1.0)
                        .text("return")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                bus.return_level.set(ret);
            }

            // Tape delay params (time, feedback, hf_damp)
            for (name, handle) in &bus.params {
                let (min, max) = tape_delay_param_range(name);
                let mut val = handle.value();
                if ui
                    .add_enabled(enabled, egui::Slider::new(&mut val, min..=max).text(name))
                    .changed()
                {
                    handle.set(val);
                }
            }
        } else {
            ui.label(egui::RichText::new("No tape delay bus").weak().small());
        }
    });
}
