use crate::preset::{self, Preset};
use std::path::PathBuf;

/// Persistent state for the preset panel.
pub struct PresetPanelState {
    pub presets: Vec<(String, PathBuf)>,
    pub preset_dir: PathBuf,
    pub new_preset_name: String,
    pub last_message: Option<String>,
}

impl PresetPanelState {
    pub fn new(preset_dir: PathBuf) -> Self {
        let presets = preset::scan_presets(&preset_dir);
        Self {
            presets,
            preset_dir,
            new_preset_name: String::new(),
            last_message: None,
        }
    }

    pub fn refresh(&mut self) {
        self.presets = preset::scan_presets(&self.preset_dir);
    }
}

/// Action returned from the preset panel UI.
pub enum PresetAction {
    Save(String),
    Load(usize),
}

/// Preset panel: save/load musical presets.
/// Returns Some(action) if the user clicked Save or Load.
pub fn show_preset_panel(
    ctx: &egui::Context,
    open: &mut bool,
    panel_state: &mut PresetPanelState,
) -> Option<PresetAction> {
    let mut action = None;

    egui::Window::new(format!("{} Presets", egui_phosphor::regular::FLOPPY_DISK))
        .open(open)
        .default_pos([500.0, 100.0])
        .default_width(250.0)
        .resizable(true)
        .collapsible(true)
        .frame(
            egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(24, 24, 24)),
        )
        .show(ctx, |ui| {
            // Save section
            ui.heading("Save Current State");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut panel_state.new_preset_name);
                if ui.button("Save").clicked() && !panel_state.new_preset_name.is_empty() {
                    action = Some(PresetAction::Save(panel_state.new_preset_name.clone()));
                }
            });

            if let Some(ref msg) = panel_state.last_message {
                ui.label(
                    egui::RichText::new(msg)
                        .size(11.0)
                        .color(egui::Color32::from_gray(160)),
                );
            }

            ui.separator();

            // Preset list
            ui.heading("Saved Presets");
            if ui.button("Refresh").clicked() {
                panel_state.refresh();
            }

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, (name, _path)) in panel_state.presets.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if i < 9 {
                                ui.label(
                                    egui::RichText::new(format!("Ctrl+{}", i + 1))
                                        .monospace()
                                        .size(10.0)
                                        .color(egui::Color32::from_gray(100)),
                                );
                            }
                            if ui.button(name).clicked() {
                                action = Some(PresetAction::Load(i));
                            }
                        });
                    }
                    if panel_state.presets.is_empty() {
                        ui.label(
                            egui::RichText::new("No presets saved yet")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100)),
                        );
                    }
                });
        });

    action
}
