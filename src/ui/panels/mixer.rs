use crate::audio::automation::AutomationTarget;
use crate::audio::mixer_state::{AutomationMode, MixerState};

/// Floating mixer window with per-channel faders, meters, pan, mute/solo.
pub fn show_mixer_panel(ctx: &egui::Context, open: &mut bool, state: &mut MixerState) {
    egui::Window::new(format!(
        "{} Mixer",
        egui_phosphor::regular::FADERS
    ))
    .open(open)
    .default_pos([100.0, 400.0])
    .default_width(500.0)
    .min_width(300.0)
    .resizable(true)
    .collapsible(true)
    .frame(
        egui::Frame::window(&ctx.style())
            .fill(egui::Color32::from_rgb(24, 24, 24)),
    )
    .show(ctx, |ui| {
        // Channel strips in horizontal layout
        ui.horizontal(|ui| {
            let strip_count = state.handles.strips.len();
            let mut shown = 0;
            for i in 0..strip_count {
                if !state.handles.strips[i].alive {
                    continue;
                }
                if shown > 0 {
                    ui.separator();
                }
                show_channel_strip(ui, state, i);
                shown += 1;
            }
            // Master fader
            ui.separator();
            show_master_strip(ui, state);
        });

        ui.add_space(4.0);
        ui.separator();

        // Automation + transport bar
        show_transport_bar(ui, state);
    });
}

fn show_channel_strip(ui: &mut egui::Ui, state: &mut MixerState, index: usize) {
    // Read current values before entering closure to avoid borrow conflicts
    let label = state.handles.strips[index].label.clone();
    let meter = if index < state.meter_count {
        state.meters[index]
    } else {
        Default::default()
    };
    let cur_gain = state.handles.strips[index].gain.value();
    let cur_pan = state.handles.strips[index].pan.value();
    let cur_mute = state.handles.strips[index].mute.value() > 0.5;
    let cur_solo = state.handles.strips[index].solo.value() > 0.5;

    ui.vertical(|ui| {
        ui.set_min_width(60.0);

        // Label
        ui.label(
            egui::RichText::new(&label)
                .size(11.0)
                .color(egui::Color32::from_gray(180)),
        );

        ui.add_space(2.0);

        // Meter bar (RMS + peak)
        let meter_height = 80.0;
        let meter_width = 12.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(meter_width, meter_height),
            egui::Sense::hover(),
        );

        // Background
        ui.painter().rect_filled(
            rect,
            2.0,
            egui::Color32::from_rgb(15, 15, 15),
        );

        // RMS bar (bottom-up)
        let rms_clamped = meter.rms.clamp(0.0, 1.0);
        if rms_clamped > 0.001 {
            let bar_height = rms_clamped * meter_height;
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.max.y - bar_height),
                rect.max,
            );
            let color = meter_color(rms_clamped);
            ui.painter().rect_filled(bar_rect, 1.0, color);
        }

        // Peak line
        let peak_clamped = meter.peak.clamp(0.0, 1.0);
        if peak_clamped > 0.001 {
            let y = rect.max.y - peak_clamped * meter_height;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
        }

        ui.add_space(4.0);

        // Gain fader (vertical slider)
        let mut gain = cur_gain;
        let response = ui.add(
            egui::Slider::new(&mut gain, 0.0..=2.0)
                .orientation(egui::SliderOrientation::Vertical)
                .custom_formatter(|v, _| format!("{:.2}", v))
                .text("")
        );
        if response.changed() {
            state.handles.strips[index].gain.set(gain);
            state.record_automation(index, AutomationTarget::Gain, gain);
        }

        ui.add_space(2.0);

        // Pan slider (horizontal)
        let mut pan = cur_pan;
        let pan_response = ui.add(
            egui::Slider::new(&mut pan, -1.0..=1.0)
                .custom_formatter(|v, _| {
                    if v.abs() < 0.05 {
                        "C".to_string()
                    } else if v < 0.0 {
                        format!("L{:.0}", -v * 100.0)
                    } else {
                        format!("R{:.0}", v * 100.0)
                    }
                })
                .show_value(true),
        );
        if pan_response.changed() {
            state.handles.strips[index].pan.set(pan);
            state.record_automation(index, AutomationTarget::Pan, pan);
        }

        ui.add_space(2.0);

        // Mute / Solo buttons
        ui.horizontal(|ui| {
            let mute_color = if cur_mute {
                egui::Color32::from_rgb(200, 60, 60)
            } else {
                egui::Color32::from_gray(80)
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("M").size(11.0).color(mute_color),
                ))
                .clicked()
            {
                state.handles.strips[index]
                    .mute
                    .set(if cur_mute { 0.0 } else { 1.0 });
            }

            let solo_color = if cur_solo {
                egui::Color32::from_rgb(220, 180, 40)
            } else {
                egui::Color32::from_gray(80)
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("S").size(11.0).color(solo_color),
                ))
                .clicked()
            {
                state.handles.strips[index]
                    .solo
                    .set(if cur_solo { 0.0 } else { 1.0 });
            }
        });
    });
}

fn show_master_strip(ui: &mut egui::Ui, state: &mut MixerState) {
    ui.vertical(|ui| {
        ui.set_min_width(60.0);

        ui.label(
            egui::RichText::new("MST")
                .size(11.0)
                .color(egui::Color32::from_gray(220)),
        );

        ui.add_space(2.0 + 80.0 + 4.0); // Skip meter space to align with channel strips

        // Master gain fader
        let mut mg = state.handles.master_gain.value();
        let response = ui.add(
            egui::Slider::new(&mut mg, 0.0..=2.0)
                .orientation(egui::SliderOrientation::Vertical)
                .custom_formatter(|v, _| format!("{:.2}", v))
                .text("")
        );
        if response.changed() {
            state.handles.master_gain.set(mg);
        }
    });
}

fn show_transport_bar(ui: &mut egui::Ui, state: &mut MixerState) {
    ui.horizontal(|ui| {
        // Automation mode dropdown
        ui.label(
            egui::RichText::new("Auto:")
                .size(11.0)
                .color(egui::Color32::from_gray(140)),
        );
        egui::ComboBox::from_id_salt("auto_mode")
            .selected_text(state.automation_mode.label())
            .width(60.0)
            .show_ui(ui, |ui| {
                for mode in AutomationMode::ALL {
                    ui.selectable_value(
                        &mut state.automation_mode,
                        mode,
                        mode.label(),
                    );
                }
            });

        ui.add_space(12.0);

        // Transport time display
        let secs = state.transport_time;
        let mins = (secs / 60.0) as u32;
        let remainder = secs % 60.0;
        ui.label(
            egui::RichText::new(format!("{}:{:05.2}", mins, remainder))
                .size(12.0)
                .monospace()
                .color(egui::Color32::from_gray(180)),
        );

        ui.add_space(8.0);

        // Rewind button
        if ui
            .add(egui::Button::new(
                egui::RichText::new(egui_phosphor::regular::SKIP_BACK).size(14.0),
            ))
            .on_hover_text("Rewind")
            .clicked()
        {
            state.rewind();
        }

        // Play/Pause toggle
        let play_icon = if state.transport_running {
            egui_phosphor::regular::PAUSE
        } else {
            egui_phosphor::regular::PLAY
        };
        if ui
            .add(egui::Button::new(
                egui::RichText::new(play_icon).size(14.0),
            ))
            .on_hover_text(if state.transport_running {
                "Pause"
            } else {
                "Play"
            })
            .clicked()
        {
            state.toggle_transport();
        }
    });
}

/// Map meter level to a thermal-palette color.
fn meter_color(level: f32) -> egui::Color32 {
    if level < 0.3 {
        egui::Color32::from_rgb(40, 180, 60)
    } else if level < 0.8 {
        let t = (level - 0.3) / 0.5;
        egui::Color32::from_rgb(
            (40.0 + t * 180.0) as u8,
            (180.0 - t * 40.0) as u8,
            (60.0 - t * 40.0) as u8,
        )
    } else {
        egui::Color32::from_rgb(220, 50, 30)
    }
}
