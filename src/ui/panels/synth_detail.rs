use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{CellUiState, OrganismUiState, species_icon};

/// Action returned from the synth detail panel.
pub enum SynthDetailAction {
    ToggleListening(ModuleId),
}

/// Whether a param name should use logarithmic scaling.
fn is_log_param(name: &str) -> bool {
    matches!(name, "root_hz" | "cutoff" | "lfo_rate")
}

/// Map cell type to a Phosphor icon.
fn cell_icon(cell_type: &str) -> &'static str {
    match cell_type {
        "xy_pad_cell" => egui_phosphor::regular::CROSSHAIR,
        "call_response_cell" => egui_phosphor::regular::CHAT_CIRCLE,
        _ => egui_phosphor::regular::CUBE,
    }
}

/// CR state code to display label.
fn cr_state_label(state: u8) -> &'static str {
    match state {
        1 => "Listen",
        2 => "Respond",
        _ => "Idle",
    }
}

/// Draw the synth detail panel for a selected organism.
/// Shows per-cell params with sliders, and an XY pad widget for xy_pad_cell.
/// Returns an optional action for app.rs to dispatch.
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
    .default_width(300.0)
    .resizable(true)
    .collapsible(true)
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
                ui.add_space(2.0);
            }
        });
    });

    action
}

/// Render a single cell as a framed module panel with param sliders.
fn show_cell_module(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(30, 30, 30))
        .show(ui, |ui| {
            // Header: cell type + bypass toggle
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        cell_icon(&cell.cell_type),
                        cell.cell_type.to_uppercase().replace('_', " "),
                    ))
                    .strong()
                    .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").small())
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(2.0);

            // Param sliders
            for (name, handle) in &cell.params {
                let (min, max) = cell
                    .param_ranges
                    .iter()
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

/// Render call_response_cell with listening toggle and state indicator.
fn show_cr_cell_module(
    ui: &mut egui::Ui,
    cell: &CellUiState,
    org: &OrganismUiState,
) -> Option<SynthDetailAction> {
    let mut action: Option<SynthDetailAction> = None;

    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(30, 30, 30))
        .show(ui, |ui| {
            // Header: cell type + bypass toggle
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} CALL RESPONSE",
                        cell_icon("call_response_cell"),
                    ))
                    .strong()
                    .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").small())
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(2.0);

            // CR status bar: listening toggle + state indicator
            ui.horizontal(|ui| {
                // Listening toggle
                let mut listening = org.cr_listening;
                if ui.checkbox(&mut listening, egui::RichText::new("Listen (L)").small()).changed() {
                    action = Some(SynthDetailAction::ToggleListening(org.mod_id));
                }

                ui.separator();

                // State label with color
                let state_label = cr_state_label(org.cr_state);
                let state_color = match org.cr_state {
                    1 => egui::Color32::from_rgb(255, 200, 80),  // Listen = amber
                    2 => egui::Color32::from_rgb(120, 220, 120), // Respond = green
                    _ => egui::Color32::from_gray(100),          // Idle = dim
                };
                ui.label(
                    egui::RichText::new(state_label)
                        .small()
                        .color(state_color),
                );
            });

            ui.add_space(2.0);

            // Param sliders
            for (name, handle) in &cell.params {
                let (min, max) = cell
                    .param_ranges
                    .iter()
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

    action
}

/// Render an XY pad as a 200x200 interactive touch area.
/// Y is inverted (top = 1.0, Kaoscillator convention).
fn show_xy_pad_widget(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(30, 30, 30))
        .show(ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} XY PAD",
                        egui_phosphor::regular::CROSSHAIR,
                    ))
                    .strong()
                    .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").small())
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(4.0);

            // Find X and Y handles
            let x_handle = cell.params.iter().find(|(n, _)| n == "x").map(|(_, h)| h);
            let y_handle = cell.params.iter().find(|(n, _)| n == "y").map(|(_, h)| h);

            let x_val = x_handle.map(|h| h.value()).unwrap_or(0.5);
            let y_val = y_handle.map(|h| h.value()).unwrap_or(0.5);

            // Pad area
            let pad_size = egui::vec2(200.0, 200.0);
            let (rect, response) = ui.allocate_exact_size(pad_size, egui::Sense::click_and_drag());

            // Handle interaction
            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let nx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    // Y inverted: top = 1.0
                    let ny = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);

                    if let Some(h) = x_handle {
                        h.set(nx);
                    }
                    if let Some(h) = y_handle {
                        h.set(ny);
                    }
                }
            }

            let painter = ui.painter_at(rect);

            // Dark background
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 20));

            // Grid lines (4x4)
            let grid_color = egui::Color32::from_gray(40);
            for i in 1..4 {
                let frac = i as f32 / 4.0;
                // Vertical
                let vx = rect.left() + frac * rect.width();
                painter.line_segment(
                    [egui::pos2(vx, rect.top()), egui::pos2(vx, rect.bottom())],
                    egui::Stroke::new(0.5, grid_color),
                );
                // Horizontal
                let hy = rect.top() + frac * rect.height();
                painter.line_segment(
                    [egui::pos2(rect.left(), hy), egui::pos2(rect.right(), hy)],
                    egui::Stroke::new(0.5, grid_color),
                );
            }

            // Border
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)), egui::StrokeKind::Inside);

            // Crosshair at current position
            let px = rect.left() + x_val * rect.width();
            // Y inverted
            let py = rect.top() + (1.0 - y_val) * rect.height();
            let cross_color = egui::Color32::from_rgb(160, 160, 220);

            painter.line_segment(
                [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                egui::Stroke::new(0.5, cross_color.linear_multiply(0.4)),
            );
            painter.line_segment(
                [egui::pos2(rect.left(), py), egui::pos2(rect.right(), py)],
                egui::Stroke::new(0.5, cross_color.linear_multiply(0.4)),
            );

            // Filled dot at position
            painter.circle_filled(egui::pos2(px, py), 5.0, cross_color);
            painter.circle_stroke(
                egui::pos2(px, py),
                5.0,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );

            // Value labels
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("X: {:.2}  Y: {:.2}", x_val, y_val))
                        .small()
                        .weak(),
                );
            });
        });
}
