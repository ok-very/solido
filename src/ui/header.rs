use crate::ui::PanelVisibility;
use crate::ui::tabs;

const HEADER_HEIGHT: f32 = 32.0;

/// Draw the workspace header bar with tab toggles, title, and window controls.
pub fn show_header(ctx: &egui::Context, visibility: &mut PanelVisibility) {
    egui::TopBottomPanel::top("workspace_header")
        .exact_height(HEADER_HEIGHT)
        .frame(
            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(18, 18, 18))
                .inner_margin(egui::Margin::symmetric(8, 0)),
        )
        .show(ctx, |ui| {
            let header_rect = ui.max_rect();

            // Full header area is draggable
            let drag_response = ui.interact(
                header_rect,
                egui::Id::new("header_drag"),
                egui::Sense::click_and_drag(),
            );
            if drag_response.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag_response.double_clicked() {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
            }

            // Left side: tab icon buttons
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(header_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.add_space(4.0);
                    tabs::show_tab_buttons(ui, visibility);
                },
            );

            // Center: title
            ui.painter().text(
                header_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Solido v0.6",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(120),
            );

            // Right side: window controls
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(header_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.visuals_mut().button_frame = false;
                    ui.add_space(4.0);
                    show_window_controls(ui, ctx);
                },
            );
        });
}

fn show_window_controls(ui: &mut egui::Ui, ctx: &egui::Context) {
    let btn_size = 14.0;

    // Close
    if ui
        .add(egui::Button::new(
            egui::RichText::new(egui_phosphor::regular::X).size(btn_size),
        ))
        .on_hover_text("Close")
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // Maximize / Restore
    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let max_icon = if is_max {
        egui_phosphor::regular::CORNERS_IN
    } else {
        egui_phosphor::regular::CORNERS_OUT
    };
    if ui
        .add(egui::Button::new(
            egui::RichText::new(max_icon).size(btn_size),
        ))
        .on_hover_text(if is_max { "Restore" } else { "Maximize" })
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
    }

    // Minimize
    if ui
        .add(egui::Button::new(
            egui::RichText::new(egui_phosphor::regular::MINUS).size(btn_size),
        ))
        .on_hover_text("Minimize")
        .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

/// Detect mouse near window edges and start resize for custom frame.
pub fn handle_window_resize(ctx: &egui::Context) {
    let margin = 12.0;
    let screen = ctx.input(|i| i.viewport_rect());
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };

    let near_left = pos.x < screen.min.x + margin;
    let near_right = pos.x > screen.max.x - margin;
    let near_top = pos.y < screen.min.y + margin;
    let near_bottom = pos.y > screen.max.y - margin;

    let direction = match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => Some(egui::viewport::ResizeDirection::NorthWest),
        (_, true, true, _) => Some(egui::viewport::ResizeDirection::NorthEast),
        (true, _, _, true) => Some(egui::viewport::ResizeDirection::SouthWest),
        (_, true, _, true) => Some(egui::viewport::ResizeDirection::SouthEast),
        (true, _, _, _) => Some(egui::viewport::ResizeDirection::West),
        (_, true, _, _) => Some(egui::viewport::ResizeDirection::East),
        (_, _, true, _) => Some(egui::viewport::ResizeDirection::North),
        (_, _, _, true) => Some(egui::viewport::ResizeDirection::South),
        _ => None,
    };

    if let Some(dir) = direction {
        let cursor = match dir {
            egui::viewport::ResizeDirection::North | egui::viewport::ResizeDirection::South => {
                egui::CursorIcon::ResizeVertical
            }
            egui::viewport::ResizeDirection::West | egui::viewport::ResizeDirection::East => {
                egui::CursorIcon::ResizeHorizontal
            }
            egui::viewport::ResizeDirection::NorthWest
            | egui::viewport::ResizeDirection::SouthEast => egui::CursorIcon::ResizeNwSe,
            egui::viewport::ResizeDirection::NorthEast
            | egui::viewport::ResizeDirection::SouthWest => egui::CursorIcon::ResizeNeSw,
        };
        ctx.set_cursor_icon(cursor);

        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
