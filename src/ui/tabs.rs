use crate::ui::PanelVisibility;

pub fn show_tab_buttons(ui: &mut egui::Ui, visibility: &mut PanelVisibility) {
    ui.spacing_mut().item_spacing.x = 2.0;

    // Inspector toggle
    let debug_btn = ui.selectable_label(
        visibility.debug,
        egui::RichText::new(egui_phosphor::regular::SLIDERS_HORIZONTAL).size(16.0),
    );
    if debug_btn.clicked() {
        visibility.debug = !visibility.debug;
    }
    debug_btn.on_hover_text("Inspector");

    // Mixer toggle
    let mixer_btn = ui.selectable_label(
        visibility.mixer,
        egui::RichText::new(egui_phosphor::regular::FADERS).size(16.0),
    );
    if mixer_btn.clicked() {
        visibility.mixer = !visibility.mixer;
    }
    mixer_btn.on_hover_text("Mixer");

    // Recorder toggle
    let rec_btn = ui.selectable_label(
        visibility.recorder,
        egui::RichText::new(egui_phosphor::regular::RECORD).size(16.0),
    );
    if rec_btn.clicked() {
        visibility.recorder = !visibility.recorder;
    }
    rec_btn.on_hover_text("Recorder");
}
