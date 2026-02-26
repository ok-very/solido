use crate::affinity::ledger::{LedgerEvent, LedgerEventType, LedgerReason};
use crate::module::PortId;
use crate::reactor::SeedReactor;
use crate::ui::build_port_names;
use std::collections::HashMap;

/// Persistent state for the ledger view panel.
pub struct LedgerViewState {
    pub filter_event_type: Option<LedgerEventType>,
    pub auto_scroll: bool,
}

impl Default for LedgerViewState {
    fn default() -> Self {
        Self {
            filter_event_type: None,
            auto_scroll: true,
        }
    }
}

/// Scrolling panel showing affinity ledger events with color coding and filtering.
pub fn show_ledger_panel(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &SeedReactor,
    view_state: &mut LedgerViewState,
) {
    let port_names = build_port_names(reactor);

    egui::Window::new(format!("{} Ledger", egui_phosphor::regular::SCROLL))
        .open(open)
        .default_pos([300.0, 400.0])
        .default_width(500.0)
        .min_width(350.0)
        .resizable(true)
        .collapsible(true)
        .frame(
            egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 20)),
        )
        .show(ctx, |ui| {
            // Filter bar
            ui.horizontal(|ui| {
                ui.label("Filter:");
                if ui
                    .selectable_label(view_state.filter_event_type.is_none(), "All")
                    .clicked()
                {
                    view_state.filter_event_type = None;
                }
                for (ev_type, label) in [
                    (LedgerEventType::Strengthened, "Strong"),
                    (LedgerEventType::Weakened, "Weak"),
                    (LedgerEventType::Explored, "Explore"),
                    (LedgerEventType::Pruned, "Prune"),
                    (LedgerEventType::Created, "Create"),
                ] {
                    if ui
                        .selectable_label(
                            view_state.filter_event_type == Some(ev_type),
                            label,
                        )
                        .clicked()
                    {
                        view_state.filter_event_type = Some(ev_type);
                    }
                }
                ui.separator();
                ui.checkbox(&mut view_state.auto_scroll, "Auto");
            });

            ui.separator();

            // Scrolling event list
            let ledger = &reactor.graph.ledger;
            let events: Vec<_> = ledger
                .events()
                .iter()
                .filter(|e| {
                    view_state
                        .filter_event_type
                        .map_or(true, |f| e.event_type == f)
                })
                .collect();

            let row_height = 18.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(400.0)
                .stick_to_bottom(view_state.auto_scroll)
                .show_rows(ui, row_height, events.len(), |ui, row_range| {
                    for i in row_range {
                        show_ledger_row(ui, events[i], &port_names);
                    }
                });

            // Event count footer
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "{} events ({} total)",
                    events.len(),
                    ledger.len()
                ))
                .monospace()
                .size(10.0)
                .color(egui::Color32::from_gray(100)),
            );
        });
}

fn show_ledger_row(
    ui: &mut egui::Ui,
    event: &LedgerEvent,
    port_names: &HashMap<PortId, (String, String)>,
) {
    let (color, icon) = match event.event_type {
        LedgerEventType::Created => (egui::Color32::from_rgb(180, 180, 80), "*"),
        LedgerEventType::Strengthened => (egui::Color32::from_rgb(80, 200, 80), "+"),
        LedgerEventType::Weakened => (egui::Color32::from_rgb(200, 80, 80), "-"),
        LedgerEventType::Explored => (egui::Color32::from_rgb(80, 140, 220), "?"),
        LedgerEventType::Pruned => (egui::Color32::from_rgb(120, 120, 120), "x"),
    };

    let (_, src_port, _, dst_port) = event.edge_id;
    let src_name = port_names
        .get(&src_port)
        .map(|(m, p)| format!("{}.{}", m, p))
        .unwrap_or_else(|| format!(":{}", src_port.0));
    let dst_name = port_names
        .get(&dst_port)
        .map(|(m, p)| format!("{}.{}", m, p))
        .unwrap_or_else(|| format!(":{}", dst_port.0));

    let delta = event.weight_after - event.weight_before;
    let reason = match event.reason {
        LedgerReason::Delivery => "dlvr",
        LedgerReason::Hebbian => "hebb",
        LedgerReason::Decay => "decay",
        LedgerReason::Exploration => "expl",
        LedgerReason::Manual => "manual",
    };

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(icon)
                .monospace()
                .size(11.0)
                .color(color),
        );
        ui.label(
            egui::RichText::new(format!(
                "t:{:<6} {} -> {} w:{:.3}{:+.3} ({})",
                event.tick, src_name, dst_name, event.weight_before, delta, reason
            ))
            .monospace()
            .size(11.0)
            .color(egui::Color32::from_gray(170)),
        );
    });
}
