use crate::dsp::shared::Shared;

/// Per-cell UI state: bypass toggle + all param handles for sliders.
pub struct CellUiState {
    pub cell_type: String,
    pub bypass: Shared,
    /// (param_name, handle) — sorted by name for stable UI ordering.
    pub params: Vec<(String, Shared)>,
    /// (param_name, min, max) — from CellRegistry, for slider ranges.
    pub param_ranges: Vec<(String, f32, f32)>,
}

/// Per-organism UI state: organism identity + cells with bypass/param handles.
pub struct OrganismUiState {
    pub name: String,
    pub species: String,
    pub hue: f32,              // DNA render.hue — visual identity color
    pub organism_id: u32,      // OrganismId in registry
    pub mixer_mute: Shared,    // VoiceBus channel mute (0=unmuted, 1=muted)
    pub mixer_gain: Shared,    // VoiceBus channel gain
    pub cells: Vec<CellUiState>,
    // S12a scaffold — Tala Mandala bead identity
    pub shape_id: u32,         // 0=circle 1=diamond 2=triangle
    /// Reverb send level for this organism (None if no reverb bus).
    pub reverb_send: Option<Shared>,
    /// Tape delay send level for this organism (None if no tape delay bus).
    pub tape_delay_send: Option<Shared>,
}

/// Reverb bus UI state (global, not per-organism).
pub struct ReverbBusUiState {
    pub reverb_type: String,
    pub return_level: Shared,
    pub params: Vec<(String, Shared)>,
}

/// Tape delay bus UI state (global, not per-organism).
pub struct TapeDelayBusUiState {
    pub return_level: Shared,
    /// (name, handle) — time, feedback, hf_damp.
    pub params: Vec<(String, Shared)>,
}

/// Control-thread state for the organism inspector panel.
/// Built from OrganismEndpoint shared handles before they're consumed.
pub struct OrganismPanelState {
    pub organisms: Vec<OrganismUiState>,
    pub reverb_bus: Option<ReverbBusUiState>,
    pub tape_delay_bus: Option<TapeDelayBusUiState>,
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

/// Shape glyph for S12a bead identity.
fn shape_glyph(shape_id: u32) -> &'static str {
    match shape_id {
        0 => "\u{25CB}", // ○ circle
        1 => "\u{25C6}", // ◆ diamond
        2 => "\u{25B2}", // ▲ triangle
        _ => "\u{25A1}", // □ square
    }
}

/// Whether a param name should use logarithmic scaling.
fn is_log_param(name: &str) -> bool {
    matches!(name, "root_hz" | "cutoff" | "lfo_rate")
}

/// Draw the organism inspector panel with per-organism identity, per-cell param sliders,
/// reverb send controls, and reverb bus controls.
pub fn show_organism_panel(
    ctx: &egui::Context,
    open: &mut bool,
    state: &OrganismPanelState,
) {
    egui::Window::new(format!(
        "{} Organisms",
        egui_phosphor::regular::DNA
    ))
    .open(open)
    .default_pos([600.0, 60.0])
    .default_width(280.0)
    .resizable(true)
    .collapsible(true)
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for org in &state.organisms {
                show_organism(ui, org);
                ui.add_space(4.0);
            }

            // Reverb bus module (global)
            if let Some(ref bus) = state.reverb_bus {
                show_reverb_bus(ui, bus);
            }

            // Tape delay bus module (global)
            if let Some(ref bus) = state.tape_delay_bus {
                show_tape_delay_bus(ui, bus);
            }
        });
    });
}

/// Render one organism as a collapsible rack of cell modules.
fn show_organism(ui: &mut egui::Ui, org: &OrganismUiState) {
    let is_muted = org.mixer_mute.value() > 0.5;

    egui::CollapsingHeader::new(
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
    )
    .default_open(true)
    .show(ui, |ui| {
        // Dim content when muted
        if is_muted {
            ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(120));
        }

        // Organism info row
        ui.horizontal(|ui| {
            // Hue swatch
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(12.0, 12.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, hue_to_color32(org.hue));

            ui.label(
                egui::RichText::new(format!(
                    "species: {} | shape: {}",
                    org.species,
                    shape_glyph(org.shape_id),
                ))
                .small()
                .weak(),
            );
        });

        // Mute toggle
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
        });

        ui.add_space(4.0);

        // Per-cell module panels
        for cell in &org.cells {
            show_cell_module(ui, cell);
            ui.add_space(2.0);
        }

        // Reverb send slider (per-organism)
        if let Some(ref send) = org.reverb_send {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(28, 28, 35))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} REVERB SEND",
                            egui_phosphor::regular::ARROW_BEND_UP_RIGHT
                        ))
                        .small()
                        .strong(),
                    );
                    let mut val = send.value();
                    if ui
                        .add(
                            egui::Slider::new(&mut val, 0.0..=1.0)
                                .text("send")
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        )
                        .changed()
                    {
                        send.set(val);
                    }
                });
        }

        // Tape delay send slider (per-organism)
        if let Some(ref send) = org.tape_delay_send {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(28, 35, 28))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} TAPE SEND",
                            egui_phosphor::regular::ARROW_BEND_UP_RIGHT
                        ))
                        .small()
                        .strong(),
                    );
                    let mut val = send.value();
                    if ui
                        .add(
                            egui::Slider::new(&mut val, 0.0..=1.0)
                                .text("send")
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        )
                        .changed()
                    {
                        send.set(val);
                    }
                });
        }

        // Reset dimming
        if is_muted {
            ui.visuals_mut().override_text_color = None;
        }
    });
}

/// Render a single cell as a framed module panel with param sliders.
fn show_cell_module(ui: &mut egui::Ui, cell: &CellUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(30, 30, 30))
        .show(ui, |ui| {
            // Header: cell type + bypass toggle
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        cell_icon(&cell.cell_type),
                        cell.cell_type.to_uppercase().replace('_', " "),
                    ))
                    .strong()
                    .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = cell.bypass.value() < 0.5;
                    if ui
                        .checkbox(&mut active, egui::RichText::new("on").small())
                        .changed()
                    {
                        cell.bypass.set(if active { 0.0 } else { 1.0 });
                    }
                });
            });

            ui.add_space(2.0);

            // Param sliders
            for (name, handle) in &cell.params {
                let (min, max) = cell
                    .param_ranges
                    .iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, mn, mx)| (*mn, *mx))
                    .unwrap_or((0.0, 1.0));

                let mut val = handle.value();
                let log = is_log_param(name);

                let slider = egui::Slider::new(&mut val, min..=max)
                    .text(name)
                    .logarithmic(log)
                    .max_decimals(if log { 1 } else { 3 });

                if ui.add(slider).changed() {
                    handle.set(val);
                }
            }
        });
}

/// Render the global reverb bus module.
fn show_reverb_bus(ui: &mut egui::Ui, bus: &ReverbBusUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(25, 25, 35))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} REVERB BUS [{}]",
                    egui_phosphor::regular::SPARKLE,
                    bus.reverb_type,
                ))
                .strong()
                .size(12.0),
            );

            ui.add_space(2.0);

            // Return level
            let mut ret = bus.return_level.value();
            if ui
                .add(
                    egui::Slider::new(&mut ret, 0.0..=1.0)
                        .text("return")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                bus.return_level.set(ret);
            }

            // Reverb params (size, dcy, damp)
            for (name, handle) in &bus.params {
                let mut val = handle.value();
                if ui
                    .add(egui::Slider::new(&mut val, 0.0..=1.0).text(name))
                    .changed()
                {
                    handle.set(val);
                }
            }
        });
}

/// Render the global tape delay bus module.
fn show_tape_delay_bus(ui: &mut egui::Ui, bus: &TapeDelayBusUiState) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(25, 35, 25))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} TAPE DELAY BUS",
                    egui_phosphor::regular::WAVES,
                ))
                .strong()
                .size(12.0),
            );

            ui.add_space(2.0);

            // Return level
            let mut ret = bus.return_level.value();
            if ui
                .add(
                    egui::Slider::new(&mut ret, 0.0..=1.0)
                        .text("return")
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                bus.return_level.set(ret);
            }

            // Tape delay params (time, feedback, hf_damp)
            for (name, handle) in &bus.params {
                let (min, max) = tape_delay_param_range(name);
                let mut val = handle.value();
                if ui
                    .add(egui::Slider::new(&mut val, min..=max).text(name))
                    .changed()
                {
                    handle.set(val);
                }
            }
        });
}

/// Valid UI range for a tape delay param.
fn tape_delay_param_range(name: &str) -> (f32, f32) {
    match name {
        "time"     => (0.05, 2.0),
        "feedback" => (0.0, 0.95),
        _          => (0.0, 1.0),
    }
}

/// Map species to a Phosphor icon.
fn species_icon(species: &str) -> &'static str {
    match species {
        "tblk" => egui_phosphor::regular::PULSE,
        "dron" => egui_phosphor::regular::WAVE_SINE,
        "melo" => egui_phosphor::regular::MUSIC_NOTES,
        _ => egui_phosphor::regular::CIRCLE,
    }
}

/// Map cell type to a Phosphor icon.
fn cell_icon(cell_type: &str) -> &'static str {
    match cell_type {
        _ => egui_phosphor::regular::CUBE,
    }
}
