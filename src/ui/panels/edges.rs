use std::collections::HashMap;

use crate::module::PortId;
use crate::reactor::SeedReactor;

pub fn show(
    ui: &mut egui::Ui,
    reactor: &SeedReactor,
    port_names: &HashMap<PortId, (String, String)>,
) {
    let mut edges: Vec<_> = reactor.graph.edges.iter().collect();
    edges.sort_by(|a, b| {
        b.1.weight
            .partial_cmp(&a.1.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (&(src_mod, src_port, dst_mod, dst_port), edge) in &edges {
        let src_name = port_names
            .get(&src_port)
            .map(|(m, p)| format!("{}.{}", m, p))
            .unwrap_or_else(|| format!("m{}:{}", src_mod, src_port));
        let dst_name = port_names
            .get(&dst_port)
            .map(|(m, p)| format!("{}.{}", m, p))
            .unwrap_or_else(|| format!("m{}:{}", dst_mod, dst_port));

        let w = edge.weight;
        let color = if w > 0.6 {
            egui::Color32::from_rgb(80, 200, 80)
        } else if w > 0.4 {
            egui::Color32::from_rgb(200, 200, 80)
        } else {
            egui::Color32::from_rgb(200, 80, 80)
        };

        let header = format!("{:.2}  {} -> {}", w, src_name, dst_name);
        let id = egui::Id::new(("edge", src_mod, src_port, dst_mod, dst_port));
        egui::CollapsingHeader::new(egui::RichText::new(&header).color(color).monospace())
            .id_salt(id)
            .default_open(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "  goodput: {:.3}  impact: {:.3}",
                    edge.goodput, edge.impact
                ));
                ui.label(format!(
                    "  eligibility: {:.3}  age: {}",
                    edge.eligibility, edge.age_blocks
                ));
            });
    }
}
