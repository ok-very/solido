use crate::reactor::SeedReactor;

pub fn show(ui: &mut egui::Ui, reactor: &SeedReactor) {
    ui.label(format!("tick: {}", reactor.tick_count()));
    ui.label(format!("modules: {}", reactor.module_count()));
    ui.label(format!("edges: {}", reactor.edge_count()));
}
