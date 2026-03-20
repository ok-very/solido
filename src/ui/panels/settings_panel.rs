//! Settings panel — audio device selection, graphics quality, MIDI config.
//!
//! Modeled after Ableton Live's audio preferences (device + SR + buffer)
//! and Blender's graphics presets (quality tiers for GPU workload).

use crate::app::AudioStatus;
use crate::audio::midi_bus::MidiBus;
use crate::config::{self, SolidoConfig, QUALITY_PRESETS};
use crate::substrate::audio::AudioSubstrate;

// ─── State ───────────────────────────────────────────────────────────

pub struct SettingsPanelState {
    /// Cached list of available audio output devices.
    pub available_audio_devices: Vec<String>,
    /// Pending device selection (not yet applied).
    pub selected_device: Option<String>,
    /// Cached list of available MIDI input ports.
    pub available_midi_ports: Vec<String>,
    /// GPU adapter info string (cached at startup).
    pub gpu_info: String,
    /// wgpu backend name (cached at startup).
    pub gpu_backend: String,
    /// True after first open (triggers device enumeration).
    initialized: bool,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self {
            available_audio_devices: Vec::new(),
            selected_device: None,
            available_midi_ports: Vec::new(),
            gpu_info: String::new(),
            gpu_backend: String::new(),
            initialized: false,
        }
    }
}

// ─── Actions ─────────────────────────────────────────────────────────

pub enum SettingsAction {
    /// User wants to change audio device (persists to config, shows restart badge).
    SetAudioDevice(Option<String>),
    /// Change graphics quality preset.
    SetQualityPreset(String),
    /// Connect to a MIDI input port.
    ConnectMidi(String),
    /// Save config to disk.
    SaveConfig,
    /// Reset all settings to defaults.
    ResetDefaults,
}

// ─── Constants ───────────────────────────────────────────────────────

const BG: egui::Color32 = egui::Color32::from_rgb(18, 18, 24);
const SECTION_BG: egui::Color32 = egui::Color32::from_rgb(22, 22, 30);
const LABEL: egui::Color32 = egui::Color32::from_gray(100);
const VALUE: egui::Color32 = egui::Color32::from_gray(180);

// ─── Main ────────────────────────────────────────────────────────────

pub fn show_settings_panel(
    ctx: &egui::Context,
    open: &mut bool,
    state: &mut SettingsPanelState,
    config: &SolidoConfig,
    audio_status: &AudioStatus,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    // Enumerate devices on first open
    if !state.initialized {
        state.available_audio_devices = AudioSubstrate::enumerate_output_devices();
        state.available_midi_ports = MidiBus::list_ports();
        state.selected_device = config.audio.device_name.clone();
        // GPU info is set externally from render_state
        state.initialized = true;
    }

    egui::Window::new(format!("{} Settings", egui_phosphor::regular::GEAR_SIX))
        .open(open)
        .default_pos([300.0, 100.0])
        .default_size([380.0, 420.0])
        .min_width(300.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(BG))
        .show(ctx, |ui| {
            // ── Audio Section ──
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{} Audio", egui_phosphor::regular::SPEAKER_HIGH))
                    .strong()
                    .size(12.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                show_audio_section(ui, state, config, audio_status, &mut actions);
            });

            ui.add_space(4.0);

            // ── Graphics Section ──
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{} Graphics", egui_phosphor::regular::MONITOR))
                    .strong()
                    .size(12.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                show_graphics_section(ui, state, config, &mut actions);
            });

            ui.add_space(4.0);

            // ── MIDI Section ──
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{} MIDI", egui_phosphor::regular::PIANO_KEYS))
                    .strong()
                    .size(12.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                show_midi_section(ui, state, &mut actions);
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Save / Reset ──
            ui.horizontal(|ui| {
                if ui.button("Save Settings").clicked() {
                    actions.push(SettingsAction::SaveConfig);
                }
                if ui.button("Reset Defaults").clicked() {
                    actions.push(SettingsAction::ResetDefaults);
                }
            });
        });

    actions
}

// ─── Audio Section ───────────────────────────────────────────────────

fn show_audio_section(
    ui: &mut egui::Ui,
    state: &mut SettingsPanelState,
    config: &SolidoConfig,
    audio_status: &AudioStatus,
    actions: &mut Vec<SettingsAction>,
) {
    egui::Frame::NONE
        .fill(SECTION_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            // Status indicator
            let (dot_color, status_text) = match audio_status {
                AudioStatus::Running { sample_rate, channels, device_name } => (
                    egui::Color32::from_rgb(60, 200, 80),
                    format!("Running: {} @ {}Hz {}ch", device_name, sample_rate, channels),
                ),
                AudioStatus::Unavailable => (
                    egui::Color32::from_rgb(200, 60, 60),
                    "No audio device".into(),
                ),
                AudioStatus::Error(msg) => (
                    egui::Color32::from_rgb(200, 60, 60),
                    format!("Error: {msg}"),
                ),
            };
            ui.horizontal(|ui| {
                let (dot_rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 4.0, dot_color);
                ui.label(
                    egui::RichText::new(&status_text)
                        .size(10.0)
                        .color(VALUE),
                );
            });

            ui.add_space(4.0);

            // Device dropdown
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Device:").size(10.0).color(LABEL));
                let current = state
                    .selected_device
                    .as_deref()
                    .unwrap_or("System Default");
                egui::ComboBox::from_id_salt("audio_device")
                    .selected_text(current)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(state.selected_device.is_none(), "System Default")
                            .clicked()
                        {
                            state.selected_device = None;
                        }
                        for name in &state.available_audio_devices {
                            let is_sel = state.selected_device.as_deref() == Some(name.as_str());
                            if ui.selectable_label(is_sel, name).clicked() {
                                state.selected_device = Some(name.clone());
                            }
                        }
                    });
            });

            // Refresh button
            ui.horizontal(|ui| {
                if ui
                    .small_button(format!("{} Refresh", egui_phosphor::regular::ARROWS_CLOCKWISE))
                    .clicked()
                {
                    state.available_audio_devices = AudioSubstrate::enumerate_output_devices();
                }
            });

            // Show if pending change differs from config
            let config_device = config.audio.device_name.as_deref();
            let pending_device = state.selected_device.as_deref();
            if config_device != pending_device {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("Apply (restart required)").size(10.0))
                        .clicked()
                    {
                        actions.push(SettingsAction::SetAudioDevice(
                            state.selected_device.clone(),
                        ));
                    }
                    ui.label(
                        egui::RichText::new("Device change takes effect on next launch")
                            .size(9.0)
                            .color(egui::Color32::from_rgb(220, 180, 60)),
                    );
                });
            }

            // Read-only info from running audio
            if let AudioStatus::Running {
                sample_rate,
                channels,
                ..
            } = audio_status
            {
                ui.add_space(2.0);
                let sr = *sample_rate;
                let ch = *channels;
                let mut label = |name: &str, val: String| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).size(9.0).monospace().color(LABEL));
                        ui.label(egui::RichText::new(val).size(9.0).monospace().color(VALUE));
                    });
                };
                label("Sample Rate:", format!("{sr} Hz"));
                label("Channels:", format!("{ch}"));
            }
        });
}

// ─── Graphics Section ────────────────────────────────────────────────

fn show_graphics_section(
    ui: &mut egui::Ui,
    state: &SettingsPanelState,
    config: &SolidoConfig,
    actions: &mut Vec<SettingsAction>,
) {
    egui::Frame::NONE
        .fill(SECTION_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            // Quality preset buttons
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Quality:").size(10.0).color(LABEL));
                for &preset in QUALITY_PRESETS {
                    let is_active = config.graphics.quality_preset == preset;
                    let btn = ui.selectable_label(
                        is_active,
                        egui::RichText::new(
                            &format!("{}{}", &preset[..1].to_uppercase(), &preset[1..]),
                        )
                        .size(10.0),
                    );
                    if btn.clicked() && !is_active {
                        actions.push(SettingsAction::SetQualityPreset(preset.into()));
                    }
                }
            });

            // Scale info
            let (bio, fluid) = config::quality_scales(&config.graphics.quality_preset);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Biofield: {bio:.2}x  Fluid: {fluid:.2}x"))
                        .size(9.0)
                        .monospace()
                        .color(LABEL),
                );
            });

            ui.add_space(2.0);

            // GPU info (read-only)
            if !state.gpu_info.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("GPU:").size(9.0).monospace().color(LABEL));
                    ui.label(
                        egui::RichText::new(&state.gpu_info)
                            .size(9.0)
                            .monospace()
                            .color(VALUE),
                    );
                });
            }
            if !state.gpu_backend.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Backend:").size(9.0).monospace().color(LABEL));
                    ui.label(
                        egui::RichText::new(&state.gpu_backend)
                            .size(9.0)
                            .monospace()
                            .color(VALUE),
                    );
                });
            }
        });
}

// ─── MIDI Section ────────────────────────────────────────────────────

fn show_midi_section(
    ui: &mut egui::Ui,
    state: &mut SettingsPanelState,
    actions: &mut Vec<SettingsAction>,
) {
    egui::Frame::NONE
        .fill(SECTION_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Input:").size(10.0).color(LABEL));
                let current = if state.available_midi_ports.is_empty() {
                    "No devices"
                } else {
                    "---"
                };
                egui::ComboBox::from_id_salt("midi_input")
                    .selected_text(current)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for name in &state.available_midi_ports {
                            if ui.selectable_label(false, name).clicked() {
                                actions.push(SettingsAction::ConnectMidi(name.clone()));
                            }
                        }
                    });
                if ui
                    .small_button(format!("{}", egui_phosphor::regular::ARROWS_CLOCKWISE))
                    .clicked()
                {
                    state.available_midi_ports = MidiBus::list_ports();
                }
            });
        });
}
