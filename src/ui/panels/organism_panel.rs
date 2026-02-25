use fundsp::prelude32::Shared;

/// Per-cell UI state: bypass toggle + all param handles for future sliders.
pub struct CellUiState {
    pub cell_type: String,
    pub bypass: Shared,
    /// (param_name, handle) — sorted by name for stable UI ordering.
    pub params: Vec<(String, Shared)>,
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
}

/// Control-thread state for the organism inspector panel.
/// Built from OrganismEndpoint shared handles before they're consumed.
pub struct OrganismPanelState {
    pub organisms: Vec<OrganismUiState>,
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

/// Draw the organism inspector panel with per-organism identity and per-cell bypass toggles.
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
    .default_width(240.0)
    .resizable(true)
    .collapsible(true)
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for org in &state.organisms {
                let is_muted = org.mixer_mute.value() > 0.5;

                // Organism header row: [hue swatch] [species icon] name [mute]
                let header_response = egui::CollapsingHeader::new(
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

                    // Organism info row: hue swatch + species + shape
                    ui.horizontal(|ui| {
                        // Hue swatch (12x12 colored rect)
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(12.0, 12.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            hue_to_color32(org.hue),
                        );

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

                    ui.add_space(2.0);

                    // Per-cell bypass toggles
                    for cell in &org.cells {
                        let mut active = cell.bypass.value() < 0.5;
                        let label = format!(
                            "{} {}",
                            cell_icon(&cell.cell_type),
                            cell.cell_type,
                        );
                        if ui.checkbox(&mut active, label).changed() {
                            cell.bypass.set(if active { 0.0 } else { 1.0 });
                        }
                    }

                    // Reset dimming
                    if is_muted {
                        ui.visuals_mut().override_text_color = None;
                    }
                });

                let _ = header_response;
                ui.add_space(4.0);
            }
        });
    });
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
        "pattern_gen" => egui_phosphor::regular::METRONOME,
        "strike_voice" => egui_phosphor::regular::HAND_FIST,
        "harmonic_bed" => egui_phosphor::regular::WAVES,
        "shimmer_layer" => egui_phosphor::regular::SPARKLE,
        "arpeggiator" => egui_phosphor::regular::STAIRS,
        "timbre_voice" => egui_phosphor::regular::WAVEFORM,
        "mod_matrix" => egui_phosphor::regular::GRAPH,
        _ => egui_phosphor::regular::CUBE,
    }
}
