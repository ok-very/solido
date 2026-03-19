use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{CellUiState, OrganismUiState, species_icon};
use crate::ui::widgets::param_knob;

/// Action returned from the synth detail panel.
pub enum SynthDetailAction {
    ToggleListening(ModuleId),
}

fn is_log_param(name: &str) -> bool {
    matches!(name, "root_hz" | "freq" | "cutoff" | "lfo_rate" | "center_hz")
}

fn cell_icon(cell_type: &str) -> &'static str {
    match cell_type {
        "osc_cell" | "saw_bank_cell" => egui_phosphor::regular::WAVE_SINE,
        "filter_cell" | "diode_filter_cell" => egui_phosphor::regular::FUNNEL,
        "seq_cell" | "logic_seq_cell" => egui_phosphor::regular::LIST_NUMBERS,
        "lfo_cell" => egui_phosphor::regular::WAVE_TRIANGLE,
        "env_cell" | "accent_env_cell" => egui_phosphor::regular::CHART_LINE_UP,
        "sample_cell" => egui_phosphor::regular::SPEAKER_HIGH,
        "melodic_cell" | "walk_cell" => egui_phosphor::regular::MUSIC_NOTES,
        "mixer_cell" => egui_phosphor::regular::SLIDERS_HORIZONTAL,
        "xy_pad_cell" => egui_phosphor::regular::CROSSHAIR,
        "call_response_cell" => egui_phosphor::regular::CHAT_CIRCLE,
        "slew_cell" => egui_phosphor::regular::ARROW_BEND_RIGHT_DOWN,
        "func_gen_cell" => egui_phosphor::regular::FUNCTION,
        "noise_burst_cell" => egui_phosphor::regular::BROADCAST,
        _ => egui_phosphor::regular::CUBE,
    }
}

fn cell_accent(cell_type: &str) -> egui::Color32 {
    match cell_type {
        "osc_cell" | "saw_bank_cell" => egui::Color32::from_rgb(210, 160, 60),
        "filter_cell" | "diode_filter_cell" => egui::Color32::from_rgb(70, 140, 200),
        "seq_cell" => egui::Color32::from_rgb(80, 180, 100),
        "lfo_cell" => egui::Color32::from_rgb(150, 100, 200),
        "env_cell" | "accent_env_cell" => egui::Color32::from_rgb(80, 180, 180),
        "sample_cell" => egui::Color32::from_rgb(200, 110, 70),
        "melodic_cell" | "walk_cell" => egui::Color32::from_rgb(200, 170, 70),
        "mixer_cell" => egui::Color32::from_rgb(140, 140, 155),
        "xy_pad_cell" => egui::Color32::from_rgb(140, 130, 200),
        "slew_cell" => egui::Color32::from_rgb(120, 160, 120),
        "func_gen_cell" => egui::Color32::from_rgb(170, 120, 150),
        "logic_seq_cell" => egui::Color32::from_rgb(100, 200, 140),
        "call_response_cell" => egui::Color32::from_rgb(200, 160, 100),
        "noise_burst_cell" => egui::Color32::from_rgb(170, 170, 180),
        "drum_voice_cell" | "strike_voice_cell" => egui::Color32::from_rgb(200, 130, 100),
        _ => egui::Color32::from_rgb(120, 120, 135),
    }
}

fn short_cell_name(cell_type: &str) -> &'static str {
    match cell_type {
        "osc_cell" => "OSC",
        "saw_bank_cell" => "SAW BANK",
        "filter_cell" => "FILTER",
        "diode_filter_cell" => "DIODE",
        "seq_cell" => "SEQ",
        "logic_seq_cell" => "LOGIC SEQ",
        "lfo_cell" => "LFO",
        "env_cell" => "ENV",
        "accent_env_cell" => "ACCENT",
        "sample_cell" => "SAMPLE",
        "melodic_cell" => "MELODIC",
        "walk_cell" => "WALK",
        "mixer_cell" => "MIXER",
        "slew_cell" => "SLEW",
        "func_gen_cell" => "FUNC GEN",
        "noise_burst_cell" => "NOISE",
        "drum_voice_cell" => "DRUM",
        "strike_voice_cell" => "STRIKE",
        _ => "CELL",
    }
}

/// Draw the synth detail panel for a selected organism.
/// Shows per-cell params with rotary knobs, and an XY pad widget for xy_pad_cell.
pub fn show_synth_detail(
    ctx: &egui::Context,
    open: &mut bool,
    org: &OrganismUiState,
) -> Option<SynthDetailAction> {
    let mut action: Option<SynthDetailAction> = None;

    egui::Window::new(format!(
        "{} {} — Synth",
        species_icon(&org.species),
        org.name,
    ))
    .open(open)
    .default_pos([900.0, 60.0])
    .default_width(320.0)
    .resizable(true)
    .collapsible(true)
    .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(16, 16, 22)))
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for cell in &org.cells {
                if cell.cell_type == "xy_pad_cell" {
                    show_xy_pad_widget(ui, cell);
                } else if cell.cell_type == "call_response_cell" {
                    if let Some(a) = show_cr_cell_module(ui, cell, org) {
                        action = Some(a);
                    }
                } else {
                    show_cell_module(ui, cell);
                }
                ui.add_space(4.0);
            }
        });
    });

    action
}

/// Render a single cell with rotary param knobs.
fn show_cell_module(ui: &mut egui::Ui, cell: &CellUiState) {
    let accent = cell_accent(&cell.cell_type);
    let border = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40);

    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(20, 20, 28))
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        cell_icon(&cell.cell_type),
                        short_cell_name(&cell.cell_type),
                    ))
                    .strong()
                    .size(11.0)
                    .color(accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").size(9.0))
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(2.0);

            // Rotary knobs
            ui.horizontal_wrapped(|ui| {
                for (name, handle) in &cell.params {
                    let (min, max) = cell
                        .param_ranges
                        .iter()
                        .find(|(n, _, _)| n == name)
                        .map(|(_, mn, mx)| (*mn, *mx))
                        .unwrap_or((0.0, 1.0));
                    let log = is_log_param(name);
                    let resp =
                        param_knob::show(ui, name, handle.value(), (min, max), log, accent, false);
                    if resp.changed {
                        handle.set(resp.value);
                    }
                }
            });
        });
}

/// Render call_response_cell with listening toggle, state indicator, and knobs.
fn show_cr_cell_module(
    ui: &mut egui::Ui,
    cell: &CellUiState,
    org: &OrganismUiState,
) -> Option<SynthDetailAction> {
    let mut action: Option<SynthDetailAction> = None;
    let accent = cell_accent("call_response_cell");
    let border = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40);

    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(20, 20, 28))
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} CALL/RESP",
                        egui_phosphor::regular::CHAT_CIRCLE,
                    ))
                    .strong()
                    .size(11.0)
                    .color(accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").size(9.0))
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.horizontal(|ui| {
                let mut listening = org.cr_listening;
                if ui
                    .checkbox(&mut listening, egui::RichText::new("Listen (L)").small())
                    .changed()
                {
                    action = Some(SynthDetailAction::ToggleListening(org.mod_id));
                }
                ui.separator();
                let (label, color) = match org.cr_state {
                    1 => ("REC", egui::Color32::from_rgb(255, 200, 80)),
                    2 => ("PLAY", egui::Color32::from_rgb(120, 220, 120)),
                    _ => ("IDLE", egui::Color32::from_gray(100)),
                };
                ui.label(egui::RichText::new(label).small().color(color));
            });

            ui.add_space(2.0);

            // Rotary knobs
            ui.horizontal_wrapped(|ui| {
                for (name, handle) in &cell.params {
                    let (min, max) = cell
                        .param_ranges
                        .iter()
                        .find(|(n, _, _)| n == name)
                        .map(|(_, mn, mx)| (*mn, *mx))
                        .unwrap_or((0.0, 1.0));
                    let log = is_log_param(name);
                    let resp =
                        param_knob::show(ui, name, handle.value(), (min, max), log, accent, false);
                    if resp.changed {
                        handle.set(resp.value);
                    }
                }
            });
        });

    action
}

/// Render an XY pad as an interactive touch area.
fn show_xy_pad_widget(ui: &mut egui::Ui, cell: &CellUiState) {
    let accent = cell_accent("xy_pad_cell");
    let border = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40);

    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(20, 20, 28))
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} XY PAD",
                        egui_phosphor::regular::CROSSHAIR,
                    ))
                    .strong()
                    .size(11.0)
                    .color(accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").size(9.0))
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(4.0);

            let x_handle = cell.params.iter().find(|(n, _)| n == "x").map(|(_, h)| h);
            let y_handle = cell.params.iter().find(|(n, _)| n == "y").map(|(_, h)| h);
            let x_val = x_handle.map(|h| h.value()).unwrap_or(0.5);
            let y_val = y_handle.map(|h| h.value()).unwrap_or(0.5);

            let size = ui.available_width().min(200.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());

            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let nx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let ny = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                    if let Some(h) = x_handle {
                        h.set(nx);
                    }
                    if let Some(h) = y_handle {
                        h.set(ny);
                    }
                }
            }

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(12, 12, 18));
                let grid_c = egui::Color32::from_gray(30);
                for i in 1..4 {
                    let f = i as f32 / 4.0;
                    let vx = rect.left() + f * rect.width();
                    painter.line_segment(
                        [egui::pos2(vx, rect.top()), egui::pos2(vx, rect.bottom())],
                        egui::Stroke::new(0.5, grid_c),
                    );
                    let hy = rect.top() + f * rect.height();
                    painter.line_segment(
                        [egui::pos2(rect.left(), hy), egui::pos2(rect.right(), hy)],
                        egui::Stroke::new(0.5, grid_c),
                    );
                }
                painter.rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
                    egui::StrokeKind::Inside,
                );

                let px = rect.left() + x_val * rect.width();
                let py = rect.top() + (1.0 - y_val) * rect.height();
                let dim =
                    egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 50);
                painter.line_segment(
                    [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                    egui::Stroke::new(0.5, dim),
                );
                painter.line_segment(
                    [egui::pos2(rect.left(), py), egui::pos2(rect.right(), py)],
                    egui::Stroke::new(0.5, dim),
                );
                painter.circle_filled(egui::pos2(px, py), 5.0, accent);
                painter.circle_stroke(
                    egui::pos2(px, py),
                    5.0,
                    egui::Stroke::new(1.0, egui::Color32::WHITE),
                );
            }

            ui.label(
                egui::RichText::new(format!("X:{x_val:.2} Y:{y_val:.2}"))
                    .size(9.0)
                    .monospace()
                    .weak(),
            );
        });
}
