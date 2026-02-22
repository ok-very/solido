/// Optional UI trait for modules — only compiled with `ui-egui` feature.
///
/// Modules that want a custom inspector panel implement this alongside
/// `ModuleCore`. The auto-generated inspector (port list, emotion gauges,
/// edge weights) wraps whatever this draws.
///
/// Modules that only need the auto-generated display skip this entirely.
pub trait ModuleUi {
    fn ui(&mut self, ui: &mut egui::Ui);
}
