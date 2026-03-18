use crate::audio::midi_bus::CcLearnState;
use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{
    CellUiState, KillAction, OrganismPanelState, OrganismUiState, hue_to_color32, species_icon,
};
use crate::ui::panels::synth_detail::SynthDetailAction;

/// Actions returned from the MC20 controller panel.
pub struct Mc20Actions {
    pub select: Option<SelectAction>,
    pub synth_detail_action: Option<SynthDetailAction>,
}

/// Selection action: user clicked an organism to arm/select it.
pub struct SelectAction {
    pub panel_idx: usize,
}

/// Persistent state for the MC20 patch bay.
pub struct PatchBayState {
    /// Currently selected output jack index (None = no selection).
    pub selected_output: Option<usize>,
    /// Active connections: (output_idx, input_idx).
    pub connections: Vec<(usize, usize)>,
}

impl PatchBayState {
    pub fn new() -> Self {
        Self {
            selected_output: None,
            connections: Vec::new(),
        }
    }
}

/// Output jack names for the patch bay.
const OUTPUT_JACKS: &[&str] = &[
    "PITCH", "GATE", "VEL", "VALENCE", "AROUSAL", "CONSON", "CHAOS",
    "SEQ", "LFO", "EG", "CONTOUR", "DENSITY", "BEAT.PH",
];

/// Input jack names for the patch bay.
const INPUT_JACKS: &[&str] = &[
    "FREQ.CV", "CUTOFF", "GAIN", "PW", "DRIVE", "CALM", "LINK.BIAS",
    "RATE.CV", "DEPTH", "TRIG", "SWING", "CHAOS.IN", "FIELD", "AVOID.B",
];

fn is_log_param(name: &str) -> bool {
    matches!(name, "root_hz" | "cutoff" | "lfo_rate")
}

fn cell_icon(cell_type: &str) -> &'static str {
    match cell_type {
        "xy_pad_cell" => egui_phosphor::regular::CROSSHAIR,
        "call_response_cell" => egui_phosphor::regular::CHAT_CIRCLE,
        _ => egui_phosphor::regular::CUBE,
    }
}

/// Draw the MC20 controller as a floating window.
///
/// Internal layout: Left organism strip | Center cell params | Right XY/EG | Bottom patch bay
pub fn show_mc20(
    ctx: &egui::Context,
    open: &mut bool,
    panel: &OrganismPanelState,
    midi_armed_idx: Option<usize>,
    cc_learn: &CcLearnState,
    patch_bay: &mut PatchBayState,
) -> Mc20Actions {
    let mut actions = Mc20Actions {
        select: None,
        synth_detail_action: None,
    };

    egui::Window::new(format!("{} MC20", egui_phosphor::regular::SLIDERS))
        .open(open)
        .default_pos([100.0, 40.0])
        .default_size([900.0, 520.0])
        .min_width(600.0)
        .min_height(360.0)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // Main horizontal layout: left strip | center cells | right XY/EG
            let available = ui.available_size();
            let bottom_height = 110.0;
            let top_height = available.y - bottom_height;

            // Top region: organism strip + cells + right panel
            ui.allocate_ui(egui::vec2(available.x, top_height), |ui| {
                ui.horizontal(|ui| {
                    // Left: organism selector strip (fixed 68px)
                    ui.allocate_ui(egui::vec2(68.0, top_height), |ui| {
                        show_organism_strip(ui, panel, midi_armed_idx, &mut actions);
                    });

                    ui.separator();

                    // Right: XY pad + EG (fixed 212px)
                    let right_width = 212.0;
                    let center_width = ui.available_width() - right_width - 8.0;

                    // Center: cell parameter grid
                    ui.allocate_ui(egui::vec2(center_width, top_height), |ui| {
                        if let Some(org) = selected_organism(panel) {
                            show_cell_grid(ui, org, &mut actions);
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("Select an organism")
                                        .size(14.0)
                                        .weak(),
                                );
                            });
                        }
                    });

                    ui.separator();

                    // Right: XY pad + EG compact
                    ui.allocate_ui(egui::vec2(right_width, top_height), |ui| {
                        if let Some(org) = selected_organism(panel) {
                            show_right_panel(ui, org);
                        }
                    });
                });
            });

            ui.separator();

            // Bottom: patch bay + CC map
            show_patch_bay(ui, patch_bay, cc_learn);
        });

    actions
}

fn selected_organism(panel: &OrganismPanelState) -> Option<&OrganismUiState> {
    panel.selected.and_then(|idx| panel.organisms.get(idx))
}

// ─── LEFT STRIP: Organism Selector ───────────────────────────────────────

fn show_organism_strip(
    ui: &mut egui::Ui,
    panel: &OrganismPanelState,
    midi_armed_idx: Option<usize>,
    actions: &mut Mc20Actions,
) {
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("ORG")
                .strong()
                .size(10.0)
                .color(egui::Color32::from_gray(120)),
        );
    });
    ui.add_space(2.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, org) in panel.organisms.iter().enumerate() {
            let is_selected = panel.selected == Some(i);
            let is_armed = midi_armed_idx == Some(i);
            let color = hue_to_color32(org.hue);

            let bg = if is_selected {
                egui::Color32::from_rgb(35, 35, 45)
            } else {
                egui::Color32::TRANSPARENT
            };

            let resp = egui::Frame::NONE
                .fill(bg)
                .inner_margin(egui::Margin::symmetric(2, 2))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if is_armed {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::KEYBOARD)
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(255, 200, 80)),
                            );
                        }
                        ui.label(
                            egui::RichText::new(species_icon(&org.species))
                                .size(14.0)
                                .color(color),
                        );
                        let short = if org.name.len() > 4 { &org.name[..4] } else { &org.name };
                        ui.label(
                            egui::RichText::new(short)
                                .size(9.0)
                                .color(if is_selected { egui::Color32::WHITE } else { egui::Color32::from_gray(140) }),
                        );
                        if org.mixer_mute.value() > 0.5 {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::SPEAKER_SLASH)
                                    .size(9.0)
                                    .color(egui::Color32::from_rgb(200, 80, 80)),
                            );
                        }
                    });
                });

            if resp.response.interact(egui::Sense::click()).clicked() {
                actions.select = Some(SelectAction { panel_idx: i });
            }
        }
    });
}

// ─── CENTER: Cell Parameter Grid ─────────────────────────────────────────

fn show_cell_grid(ui: &mut egui::Ui, org: &OrganismUiState, actions: &mut Mc20Actions) {
    ui.horizontal(|ui| {
        let color = hue_to_color32(org.hue);
        ui.label(
            egui::RichText::new(format!("{} {}", species_icon(&org.species), org.name))
                .strong()
                .size(13.0)
                .color(color),
        );
    });
    ui.add_space(2.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let available_width = ui.available_width();
        let col_count = if available_width > 500.0 { 3 }
            else if available_width > 300.0 { 2 }
            else { 1 };

        let cells: Vec<&CellUiState> = org.cells.iter()
            .filter(|c| c.cell_type != "xy_pad_cell")
            .collect();

        ui.columns(col_count, |columns| {
            for (i, cell) in cells.iter().enumerate() {
                let col = i % col_count;
                if cell.cell_type == "call_response_cell" {
                    show_cr_cell(&mut columns[col], cell, org, actions);
                } else {
                    show_cell_module(&mut columns[col], cell);
                }
                columns[col].add_space(4.0);
            }
        });
    });
}

fn show_cell_module(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(25, 25, 30))
        .stroke(egui::Stroke::new(0.5, egui::Color32::from_gray(50)))
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        cell_icon(&cell.cell_type),
                        cell.cell_type.to_uppercase().replace('_', " "),
                    ))
                    .strong()
                    .size(10.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui.checkbox(&mut active, egui::RichText::new("on").size(9.0)).changed() {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });
            ui.add_space(1.0);

            for (name, handle) in &cell.params {
                let (min, max) = cell.param_ranges.iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, mn, mx)| (*mn, *mx))
                    .unwrap_or((0.0, 1.0));
                let mut val = handle.value();
                let log = is_log_param(name);
                let slider = egui::Slider::new(&mut val, min..=max)
                    .text(name)
                    .logarithmic(log)
                    .max_decimals(if log { 1 } else { 3 });
                if ui.add(slider).changed() {
                    handle.set(val);
                }
            }
        });
}

fn show_cr_cell(
    ui: &mut egui::Ui,
    cell: &CellUiState,
    org: &OrganismUiState,
    actions: &mut Mc20Actions,
) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(25, 25, 30))
        .stroke(egui::Stroke::new(0.5, egui::Color32::from_gray(50)))
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} CALL RESPONSE", cell_icon("call_response_cell")))
                        .strong().size(10.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui.checkbox(&mut active, egui::RichText::new("on").size(9.0)).changed() {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });
            ui.add_space(1.0);
            ui.horizontal(|ui| {
                let mut listening = org.cr_listening;
                if ui.checkbox(&mut listening, egui::RichText::new("Listen").size(9.0)).changed() {
                    actions.synth_detail_action = Some(SynthDetailAction::ToggleListening(org.mod_id));
                }
                ui.separator();
                let (label, color) = match org.cr_state {
                    1 => ("Listen", egui::Color32::from_rgb(255, 200, 80)),
                    2 => ("Respond", egui::Color32::from_rgb(120, 220, 120)),
                    _ => ("Idle", egui::Color32::from_gray(100)),
                };
                ui.label(egui::RichText::new(label).size(9.0).color(color));
            });
            ui.add_space(1.0);
            for (name, handle) in &cell.params {
                let (min, max) = cell.param_ranges.iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, mn, mx)| (*mn, *mx))
                    .unwrap_or((0.0, 1.0));
                let mut val = handle.value();
                let log = is_log_param(name);
                let slider = egui::Slider::new(&mut val, min..=max)
                    .text(name).logarithmic(log)
                    .max_decimals(if log { 1 } else { 3 });
                if ui.add(slider).changed() { handle.set(val); }
            }
        });
}

// ─── RIGHT PANEL: XY Pad + EG ───────────────────────────────────────────

fn show_right_panel(ui: &mut egui::Ui, org: &OrganismUiState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(env) = org.cells.iter().find(|c| c.cell_type == "env_cell") {
            show_eg_compact(ui, env);
            ui.add_space(6.0);
        }
        if let Some(env) = org.cells.iter().find(|c| c.cell_type == "accent_env_cell") {
            show_eg_compact(ui, env);
        }
    });
}

/// Show the XY pad as a standalone floating window.
/// Called from app.rs when the selected organism has an xy_pad_cell.
pub fn show_xy_pad_window(ctx: &egui::Context, cell: &CellUiState) {
    egui::Window::new(format!("{} XY Pad", egui_phosphor::regular::CROSSHAIR))
        .default_pos([950.0, 60.0])
        .default_size([220.0, 250.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            show_xy_pad(ui, cell);
        });
}

fn show_xy_pad(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(20, 20, 25))
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{} XY PAD", egui_phosphor::regular::CROSSHAIR))
                    .strong().size(10.0),
            );
            ui.add_space(3.0);

            let x_handle = cell.params.iter().find(|(n, _)| n == "x").map(|(_, h)| h);
            let y_handle = cell.params.iter().find(|(n, _)| n == "y").map(|(_, h)| h);
            let x_val = x_handle.map(|h| h.value()).unwrap_or(0.5);
            let y_val = y_handle.map(|h| h.value()).unwrap_or(0.5);

            let size = ui.available_width().min(196.0);
            let pad_size = egui::vec2(size, size);
            let (rect, response) = ui.allocate_exact_size(pad_size, egui::Sense::click_and_drag());

            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let nx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let ny = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                    if let Some(h) = x_handle { h.set(nx); }
                    if let Some(h) = y_handle { h.set(ny); }
                }
            }

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(12, 12, 16));

            let grid_color = egui::Color32::from_gray(30);
            for i in 1..4 {
                let f = i as f32 / 4.0;
                let vx = rect.left() + f * rect.width();
                painter.line_segment([egui::pos2(vx, rect.top()), egui::pos2(vx, rect.bottom())], egui::Stroke::new(0.5, grid_color));
                let hy = rect.top() + f * rect.height();
                painter.line_segment([egui::pos2(rect.left(), hy), egui::pos2(rect.right(), hy)], egui::Stroke::new(0.5, grid_color));
            }
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(50)), egui::StrokeKind::Inside);

            let px = rect.left() + x_val * rect.width();
            let py = rect.top() + (1.0 - y_val) * rect.height();
            let cc = egui::Color32::from_rgb(160, 160, 220);
            painter.line_segment([egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())], egui::Stroke::new(0.5, cc.linear_multiply(0.4)));
            painter.line_segment([egui::pos2(rect.left(), py), egui::pos2(rect.right(), py)], egui::Stroke::new(0.5, cc.linear_multiply(0.4)));
            painter.circle_filled(egui::pos2(px, py), 4.0, cc);
            painter.circle_stroke(egui::pos2(px, py), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));

            ui.label(egui::RichText::new(format!("X:{:.2} Y:{:.2}", x_val, y_val)).size(9.0).weak());
        });
}

fn show_eg_compact(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(20, 20, 25))
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} {}", cell_icon(&cell.cell_type), cell.cell_type.to_uppercase().replace('_', " ")))
                        .strong().size(9.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui.checkbox(&mut active, egui::RichText::new("on").size(9.0)).changed() {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });
            ui.add_space(1.0);
            for (name, handle) in &cell.params {
                let (min, max) = cell.param_ranges.iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, mn, mx)| (*mn, *mx))
                    .unwrap_or((0.0, 1.0));
                let mut val = handle.value();
                let short = name.chars().next().unwrap_or('?').to_uppercase().to_string();
                let slider = egui::Slider::new(&mut val, min..=max).text(&short).max_decimals(2);
                if ui.add(slider).changed() { handle.set(val); }
            }
        });
}

// ─── BOTTOM: Patch Bay + CC Map ──────────────────────────────────────────

fn show_patch_bay(ui: &mut egui::Ui, patch_bay: &mut PatchBayState, cc_learn: &CcLearnState) {
    ui.label(
        egui::RichText::new("P A T C H   B A Y")
            .strong().size(10.0)
            .color(egui::Color32::from_gray(120)),
    );
    ui.add_space(2.0);

    // Outputs row
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("OUT").size(9.0).color(egui::Color32::from_gray(70)));
        ui.add_space(2.0);
        for (i, name) in OUTPUT_JACKS.iter().enumerate() {
            let selected = patch_bay.selected_output == Some(i);
            let color = if selected {
                egui::Color32::from_rgb(255, 180, 50)
            } else {
                egui::Color32::from_rgb(170, 170, 190)
            };
            let bg = if selected {
                egui::Color32::from_rgb(45, 38, 18)
            } else {
                egui::Color32::from_rgb(28, 28, 32)
            };
            if ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", egui_phosphor::regular::CIRCLE, name)).size(9.0).color(color)).fill(bg)).clicked() {
                patch_bay.selected_output = if selected { None } else { Some(i) };
            }
        }
    });

    ui.add_space(1.0);

    // Inputs row
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("IN ").size(9.0).color(egui::Color32::from_gray(70)));
        ui.add_space(2.0);
        for (i, name) in INPUT_JACKS.iter().enumerate() {
            let connected = patch_bay.connections.iter().any(|(_, inp)| *inp == i);
            let compatible = patch_bay.selected_output.is_some();
            let color = if compatible && !connected {
                egui::Color32::from_rgb(100, 200, 100)
            } else if connected {
                egui::Color32::from_rgb(110, 150, 210)
            } else {
                egui::Color32::from_rgb(130, 130, 150)
            };
            if ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", egui_phosphor::regular::CIRCLE, name)).size(9.0).color(color)).fill(egui::Color32::from_rgb(28, 28, 32))).clicked() {
                if let Some(out_idx) = patch_bay.selected_output {
                    if let Some(pos) = patch_bay.connections.iter().position(|(o, inp)| *o == out_idx && *inp == i) {
                        patch_bay.connections.remove(pos);
                    } else {
                        patch_bay.connections.push((out_idx, i));
                    }
                    patch_bay.selected_output = None;
                }
            }
        }
    });

    // Active cables
    if !patch_bay.connections.is_empty() {
        ui.add_space(1.0);
        let colors = [
            egui::Color32::from_rgb(220, 100, 100), egui::Color32::from_rgb(100, 200, 120),
            egui::Color32::from_rgb(100, 140, 220), egui::Color32::from_rgb(220, 180, 80),
            egui::Color32::from_rgb(180, 100, 220), egui::Color32::from_rgb(100, 200, 200),
        ];
        ui.horizontal_wrapped(|ui| {
            for (ci, (o, inp)) in patch_bay.connections.iter().enumerate() {
                let c = colors[ci % colors.len()];
                ui.label(egui::RichText::new(format!("{}→{}", OUTPUT_JACKS.get(*o).unwrap_or(&"?"), INPUT_JACKS.get(*inp).unwrap_or(&"?"))).size(9.0).color(c));
            }
        });
    }

    ui.add_space(1.0);
    ui.separator();

    // CC Map
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("CC").size(9.0).strong().color(egui::Color32::from_gray(100)));
        ui.add_space(4.0);
        for m in &cc_learn.mappings {
            ui.label(egui::RichText::new(format!("{}→{}", m.cc, m.target)).size(9.0).color(egui::Color32::from_rgb(150, 170, 190)));
        }
        if cc_learn.learning {
            ui.label(egui::RichText::new("LEARN...").size(9.0).color(egui::Color32::from_rgb(255, 200, 80)));
        }
    });
}
