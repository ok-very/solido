use std::collections::HashMap;

use crate::module::{PortId, SignalType};
use crate::reactor::{SeedReactor, SIGNAL_LOG_CAPACITY};

pub fn show(
    ui: &mut egui::Ui,
    reactor: &SeedReactor,
    port_names: &HashMap<PortId, (String, String)>,
) {
    ui.label(format!(
        "{} events (last {})",
        reactor.signal_log.len(),
        SIGNAL_LOG_CAPACITY
    ));

    let interesting: Vec<_> = reactor
        .signal_log
        .iter()
        .rev()
        .filter(|e| matches!(e.signal_type, SignalType::Trigger) || e.value_str != "0.000")
        .take(20)
        .collect();

    for event in &interesting {
        let src = port_names
            .get(&event.src_port)
            .map(|(m, p)| format!("{}.{}", m, p))
            .unwrap_or_else(|| format!("m{}", event.src_module));
        let dst = port_names
            .get(&event.dst_port)
            .map(|(m, p)| format!("{}.{}", m, p))
            .unwrap_or_else(|| format!("m{}", event.dst_module));

        let color = match event.signal_type {
            SignalType::Trigger => egui::Color32::from_rgb(255, 180, 50),
            SignalType::Float => egui::Color32::from_rgb(150, 200, 255),
            _ => egui::Color32::GRAY,
        };

        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_gray(100), format!("t{}", event.tick));
            ui.colored_label(color, format!("{} {} -> {}", event.value_str, src, dst));
        });
    }
}
