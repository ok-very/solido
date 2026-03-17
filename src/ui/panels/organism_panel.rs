use crate::dsp::shared::Shared;
use crate::module::ModuleId;

/// Per-cell UI state: bypass toggle + all param handles for sliders.
pub struct CellUiState {
    pub cell_type: String,
    pub bypass: Shared,
    /// (param_name, handle) — sorted by name for stable UI ordering.
    pub params: Vec<(String, Shared)>,
    /// (param_name, min, max) — from cell PARAM_RANGES, for slider ranges.
    pub param_ranges: Vec<(String, f32, f32)>,
}

/// Per-organism UI state: organism identity + cells with bypass/param handles.
pub struct OrganismUiState {
    pub name: String,
    pub species: String,
    pub hue: f32,              // DNA render.hue — visual identity color
    pub organism_id: u32,      // OrganismId in registry
    pub mod_id: ModuleId,      // Reactor module ID — used for kill/unregister
    pub mixer_mute: Shared,    // VoiceBus channel mute (0=unmuted, 1=muted)
    #[allow(dead_code)]
    pub mixer_gain: Shared,    // VoiceBus channel gain
    pub cells: Vec<CellUiState>,
    // S12a scaffold — Tala Mandala bead identity
    pub shape_id: u32,         // 0=circle 1=diamond 2=triangle
    /// Reverb send level for this organism (None if no reverb bus).
    pub reverb_send: Option<Shared>,
    /// Tape delay send level for this organism (None if no tape delay bus).
    pub tape_delay_send: Option<Shared>,
    /// Index into the audio callback's organisms Vec (for tombstone on despawn).
    pub audio_idx: usize,
    /// Call-response cell: capture-armed mode (updated from bridge data each frame).
    pub cr_listening: bool,
    /// Call-response cell state: 0=Idle, 1=Listen, 2=Respond (updated from bridge data).
    pub cr_state: u8,
}

/// Kill action returned from the organism panel when the user clicks the kill button.
pub struct KillAction {
    /// Index into OrganismPanelState::organisms.
    pub panel_idx: usize,
    /// Reactor module ID for unregister.
    pub mod_id: ModuleId,
    /// Visual registry organism ID for despawn.
    pub org_id: u32,
    /// Index into audio callback's organisms Vec (for tombstone).
    pub audio_idx: usize,
}

/// Selection action: user clicked an organism row to open its synth detail.
pub struct SelectAction {
    pub panel_idx: usize,
}

/// Actions returned from the organism overview panel.
pub struct OrganismPanelActions {
    pub kill: Option<KillAction>,
    pub select: Option<SelectAction>,
}

// Re-export bus UI state types from effects_panel (they were moved there).
pub use crate::ui::panels::effects_panel::{ReverbBusUiState, TapeDelayBusUiState};

/// Control-thread state for the organism inspector panel.
/// Built from OrganismEndpoint shared handles before they're consumed.
pub struct OrganismPanelState {
    pub organisms: Vec<OrganismUiState>,
    #[allow(dead_code)]
    pub reverb_bus: Option<ReverbBusUiState>,
    #[allow(dead_code)]
    pub tape_delay_bus: Option<TapeDelayBusUiState>,
    /// Currently selected organism index (for synth detail panel).
    pub selected: Option<usize>,
}

/// Convert a hue [0,1] to an egui Color32 (HSL with S=0.7, L=0.55).
pub fn hue_to_color32(hue: f32) -> egui::Color32 {
    let h = hue.fract().abs() * 360.0;
    let s = 0.7_f32;
    let l = 0.55_f32;

    // HSL → RGB conversion
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    egui::Color32::from_rgb(
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

/// Map species to a Phosphor icon.
pub fn species_icon(species: &str) -> &'static str {
    match species {
        "tblk" => egui_phosphor::regular::PULSE,
        "dron" => egui_phosphor::regular::WAVE_SINE,
        "melo" => egui_phosphor::regular::MUSIC_NOTES,
        _ => egui_phosphor::regular::CIRCLE,
    }
}

/// Draw the organism overview panel: compact rows with name/mute/kill/sends.
/// Click an organism row to open its synth detail.
///
/// Returns actions for kill and selection.
pub fn show_organism_panel(
    ctx: &egui::Context,
    open: &mut bool,
    state: &OrganismPanelState,
    midi_armed_idx: Option<usize>,
) -> OrganismPanelActions {
    let mut actions = OrganismPanelActions {
        kill: None,
        select: None,
    };
    egui::Window::new(format!(
        "{} Organisms",
        egui_phosphor::regular::DNA
    ))
    .open(open)
    .default_pos([600.0, 60.0])
    .default_width(260.0)
    .resizable(true)
    .collapsible(true)
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, org) in state.organisms.iter().enumerate() {
                let is_selected = state.selected == Some(idx);
                let is_armed = midi_armed_idx == Some(idx);
                let result = show_organism_row(ui, org, idx, is_selected, is_armed);
                if let Some(kill) = result.kill {
                    actions.kill = Some(kill);
                }
                if result.clicked {
                    actions.select = Some(SelectAction { panel_idx: idx });
                }
                ui.add_space(2.0);
            }
        });
    });
    actions
}

struct RowResult {
    kill: Option<KillAction>,
    clicked: bool,
}

/// Render one organism as a compact overview row.
fn show_organism_row(
    ui: &mut egui::Ui,
    org: &OrganismUiState,
    panel_idx: usize,
    is_selected: bool,
    is_armed: bool,
) -> RowResult {
    let mut result = RowResult {
        kill: None,
        clicked: false,
    };
    let is_muted = org.mixer_mute.value() > 0.5;

    // Subtle highlight for selected organism
    let frame = egui::Frame::group(ui.style())
        .fill(if is_selected {
            egui::Color32::from_rgb(40, 40, 55)
        } else {
            egui::Color32::from_rgb(28, 28, 28)
        })
        .stroke(if is_selected {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 140))
        } else {
            egui::Stroke::NONE
        });

    let frame_response = frame.show(ui, |ui| {
        if is_muted {
            ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(120));
        }

        // Header row: icon + name (clickable area)
        ui.horizontal(|ui| {
            // Hue swatch
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(10.0, 10.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, hue_to_color32(org.hue));

            // MIDI armed badge — small "M" indicator before the name
            if is_armed {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::KEYBOARD)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(255, 160, 40)),
                );
            }

            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    species_icon(&org.species),
                    org.name,
                ))
                .strong()
                .color(if is_muted {
                    egui::Color32::GRAY
                } else {
                    ui.visuals().text_color()
                }),
            );
        });

        // Mute + kill row
        ui.horizontal(|ui| {
            let mut unmuted = !is_muted;
            if ui
                .checkbox(
                    &mut unmuted,
                    egui::RichText::new(format!(
                        "{} active",
                        egui_phosphor::regular::SPEAKER_HIGH,
                    ))
                    .small(),
                )
                .changed()
            {
                org.mixer_mute.set(if unmuted { 0.0 } else { 1.0 });
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let kill_btn = ui.add(
                    egui::Button::new(
                        egui::RichText::new(egui_phosphor::regular::TRASH)
                            .size(12.0)
                            .color(egui::Color32::from_rgb(180, 80, 80)),
                    )
                    .small(),
                );
                if kill_btn.on_hover_text("Kill organism").clicked() {
                    result.kill = Some(KillAction {
                        panel_idx,
                        mod_id: org.mod_id,
                        org_id: org.organism_id,
                        audio_idx: org.audio_idx,
                    });
                }
            });
        });

        // Send sliders (compact)
        if let Some(ref send) = org.reverb_send {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Reverb").small().weak());
                let mut val = send.value();
                if ui.add(
                    egui::Slider::new(&mut val, 0.0..=1.0)
                        .show_value(false)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                ).changed() {
                    send.set(val);
                }
            });
        }

        if let Some(ref send) = org.tape_delay_send {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Tape").small().weak());
                let mut val = send.value();
                if ui.add(
                    egui::Slider::new(&mut val, 0.0..=1.0)
                        .show_value(false)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                ).changed() {
                    send.set(val);
                }
            });
        }

        if is_muted {
            ui.visuals_mut().override_text_color = None;
        }
    });

    // Detect click on the frame for selection (but not on interactive widgets)
    if frame_response.response.interact(egui::Sense::click()).clicked() {
        result.clicked = true;
    }

    result
}
