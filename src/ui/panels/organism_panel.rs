use fundsp::prelude32::Shared;

/// Per-cell UI state: bypass toggle + all param handles for future sliders.
pub struct CellUiState {
    pub cell_type: String,
    pub bypass: Shared,
    /// (param_name, handle) — sorted by name for stable UI ordering.
    pub params: Vec<(String, Shared)>,
}

/// Per-organism UI state: cells with bypass/param handles.
pub struct OrganismUiState {
    pub name: String,
    pub species: String,
    pub cells: Vec<CellUiState>,
}

/// Control-thread state for the organism inspector panel.
/// Built from OrganismEndpoint shared handles before they're consumed.
pub struct OrganismPanelState {
    pub organisms: Vec<OrganismUiState>,
}

/// Draw the organism inspector panel with per-cell bypass toggles.
///
/// Future: per-cell param sliders below each cell heading.
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
                let header = format!(
                    "{} {}",
                    species_icon(&org.species),
                    org.name,
                );
                egui::CollapsingHeader::new(
                    egui::RichText::new(&header).strong(),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("species: {}", org.species))
                            .small()
                            .weak(),
                    );
                    ui.add_space(2.0);

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
                });

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
