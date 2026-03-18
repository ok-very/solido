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
    debug_btn.on_hover_text("Inspector (F1)");

    // Mixer toggle
    let mixer_btn = ui.selectable_label(
        visibility.mixer,
        egui::RichText::new(egui_phosphor::regular::FADERS).size(16.0),
    );
    if mixer_btn.clicked() {
        visibility.mixer = !visibility.mixer;
    }
    mixer_btn.on_hover_text("Mixer (F2)");

    // Controls toggle
    let ctrl_btn = ui.selectable_label(
        visibility.controls,
        egui::RichText::new(egui_phosphor::regular::GEAR).size(16.0),
    );
    if ctrl_btn.clicked() {
        visibility.controls = !visibility.controls;
    }
    ctrl_btn.on_hover_text("Controls");

    // Organisms toggle
    let org_btn = ui.selectable_label(
        visibility.organisms,
        egui::RichText::new(egui_phosphor::regular::DNA).size(16.0),
    );
    if org_btn.clicked() {
        visibility.organisms = !visibility.organisms;
    }
    org_btn.on_hover_text("Organisms");

    // Ledger toggle
    let ledger_btn = ui.selectable_label(
        visibility.ledger,
        egui::RichText::new(egui_phosphor::regular::SCROLL).size(16.0),
    );
    if ledger_btn.clicked() {
        visibility.ledger = !visibility.ledger;
    }
    ledger_btn.on_hover_text("Ledger (F3)");

    // Presets toggle
    let preset_btn = ui.selectable_label(
        visibility.presets,
        egui::RichText::new(egui_phosphor::regular::FLOPPY_DISK).size(16.0),
    );
    if preset_btn.clicked() {
        visibility.presets = !visibility.presets;
    }
    preset_btn.on_hover_text("Presets");

    // Effects toggle
    let effects_btn = ui.selectable_label(
        visibility.effects,
        egui::RichText::new(egui_phosphor::regular::WAVEFORM).size(16.0),
    );
    if effects_btn.clicked() {
        visibility.effects = !visibility.effects;
    }
    effects_btn.on_hover_text("Effects");

    // Wells toggle
    let wells_btn = ui.selectable_label(
        visibility.wells,
        egui::RichText::new(egui_phosphor::regular::GLOBE_HEMISPHERE_WEST).size(16.0),
    );
    if wells_btn.clicked() {
        visibility.wells = !visibility.wells;
    }
    wells_btn.on_hover_text("Wells");

    // Spawn toggle
    let spawn_btn = ui.selectable_label(
        visibility.spawn,
        egui::RichText::new(egui_phosphor::regular::PLUS_CIRCLE).size(16.0),
    );
    if spawn_btn.clicked() {
        visibility.spawn = !visibility.spawn;
    }
    spawn_btn.on_hover_text("Spawn Organism");

    // MC20 controller toggle
    let mc20_btn = ui.selectable_label(
        visibility.mc20,
        egui::RichText::new(egui_phosphor::regular::SLIDERS).size(16.0),
    );
    if mc20_btn.clicked() {
        visibility.mc20 = !visibility.mc20;
    }
    mc20_btn.on_hover_text("MC20 Controller");

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
