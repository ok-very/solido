//! Groove panel — global grid/swing controls, per-organism rhythm matrix,
//! groove templates, and beat visualizer.
//!
//! Surfaces as a header-tab-toggled floating window.
//! Global changes broadcast via DspCommand; per-organism overrides target
//! individual OrganismModules.

use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{OrganismPanelState, OrganismUiState, hue_to_color32, species_icon};

// ─── Grid Division Constants ─────────────────────────────────────────────

/// Grid division presets: (f32 beat-fraction, display label).
/// 0.0 = free-running (no quantization).
const GRID_PRESETS: &[(f32, &str)] = &[
    (0.0,        "Free"),
    (1.0,        "1/4"),
    (0.5,        "1/8"),
    (1.0 / 3.0,  "1/8T"),
    (0.25,       "1/16"),
    (1.0 / 6.0,  "1/16T"),
    (0.125,      "1/32"),
];

/// Find the display label for a grid division value.
fn grid_label(value: f32) -> &'static str {
    GRID_PRESETS.iter()
        .find(|(v, _)| (*v - value).abs() < 0.01)
        .map(|(_, label)| *label)
        .unwrap_or("?")
}

/// Find the index of a grid division value in GRID_PRESETS.
fn grid_index(value: f32) -> usize {
    GRID_PRESETS.iter()
        .position(|(v, _)| (*v - value).abs() < 0.01)
        .unwrap_or(0)
}

// ─── Groove Templates ────────────────────────────────────────────────────

/// A groove template: named preset of grid + swing + optional tempo_ratio.
struct GrooveTemplate {
    name: &'static str,
    grid: f32,
    swing: f32,
    /// If Some, sets tempo_ratio on the selected organism only.
    tempo_ratio: Option<f32>,
}

const TEMPLATES: &[GrooveTemplate] = &[
    GrooveTemplate { name: "Straight",   grid: 0.25,       swing: 0.5,  tempo_ratio: None },
    GrooveTemplate { name: "Swing 60",   grid: 0.5,        swing: 0.6,  tempo_ratio: None },
    GrooveTemplate { name: "Swing 67",   grid: 0.5,        swing: 0.67, tempo_ratio: None },
    GrooveTemplate { name: "Triplet",    grid: 1.0 / 3.0,  swing: 0.5,  tempo_ratio: None },
    GrooveTemplate { name: "Shuffle",    grid: 0.25,       swing: 0.62, tempo_ratio: None },
    GrooveTemplate { name: "Laid Back",  grid: 0.5,        swing: 0.55, tempo_ratio: None },
    GrooveTemplate { name: "Pushed",     grid: 0.5,        swing: 0.42, tempo_ratio: None },
    GrooveTemplate { name: "Phasing",    grid: 0.25,       swing: 0.5,  tempo_ratio: Some(1.01) },
];

// ─── Rhythm Sync Labels ──────────────────────────────────────────────────

const SYNC_LABELS: &[&str] = &["none", "soft", "hard"];

fn sync_label(mode: u8) -> &'static str {
    SYNC_LABELS.get(mode as usize).unwrap_or(&"?")
}

// ─── Persistent Panel State ──────────────────────────────────────────────

/// State persisted across frames for the groove panel.
pub struct GroovePanelState {
    /// Global grid division (f32 beat-fraction, 0.0 = free).
    pub global_grid: f32,
    /// Global swing [0.0, 1.0]. 0.5 = straight.
    pub global_swing: f32,
    /// Currently selected groove template index (None = manual).
    pub template_idx: Option<usize>,
    /// Per-organism grid overrides. Key = organism_id, value = grid division.
    /// Absent = use global.
    pub org_grid_overrides: std::collections::HashMap<u32, f32>,
    /// Per-organism rhythm sync overrides. Key = organism_id.
    /// These track the runtime state (initialized from DNA, updated by UI).
    pub org_sync: std::collections::HashMap<u32, u8>,
    /// Per-organism tempo ratio. Key = organism_id.
    /// Initialized from DspAnalysis feedback; editable in matrix.
    pub org_tempo_ratio: std::collections::HashMap<u32, f32>,
    /// Per-organism swing. Key = organism_id.
    pub org_swing: std::collections::HashMap<u32, f32>,
    /// Beat phase [0,1] from the selected organism's DspAnalysis.
    pub beat_phase: f32,
}

impl GroovePanelState {
    pub fn new() -> Self {
        Self {
            global_grid: 0.0,
            global_swing: 0.5,
            template_idx: None,
            org_grid_overrides: std::collections::HashMap::new(),
            org_sync: std::collections::HashMap::new(),
            org_tempo_ratio: std::collections::HashMap::new(),
            org_swing: std::collections::HashMap::new(),
            beat_phase: 0.0,
        }
    }
}

// ─── Actions ─────────────────────────────────────────────────────────────

/// Actions returned to app.rs for dispatch.
pub enum GrooveAction {
    /// Broadcast SetGridDivision to all organisms.
    SetGlobalGrid(f32),
    /// Broadcast SetGlobalSwing to all organisms.
    SetGlobalSwing(f32),
    /// Set per-organism rhythm sync mode.
    SetRhythmSync { mod_id: ModuleId, mode: u8 },
    /// Set per-organism tempo ratio (sends SetTempoRatio DspCommand).
    SetTempoRatio { mod_id: ModuleId, ratio: f32 },
    /// Set per-organism grid division override.
    SetOrgGrid { mod_id: ModuleId, grid: f32 },
    /// Set per-organism swing (finds the cell's swing Shared handle).
    SetOrgSwing { organism_id: u32, swing: f32 },
    /// Apply a groove template (global grid + swing, optionally per-org tempo).
    ApplyTemplate { grid: f32, swing: f32, tempo_ratio: Option<f32>, selected_mod_id: Option<ModuleId> },
}

// ─── Main Panel Function ─────────────────────────────────────────────────

/// Draw the groove panel. Returns a Vec of actions for app.rs to dispatch.
pub fn show_groove_panel(
    ctx: &egui::Context,
    open: &mut bool,
    state: &mut GroovePanelState,
    panel: &OrganismPanelState,
) -> Vec<GrooveAction> {
    let mut actions = Vec::new();

    egui::Window::new(format!("{} Groove", egui_phosphor::regular::METRONOME))
        .open(open)
        .default_pos([300.0, 350.0])
        .default_size([520.0, 340.0])
        .min_width(400.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 24)))
        .show(ctx, |ui| {
            // ── Global Section ──
            show_global_controls(ui, state, &mut actions, panel);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // ── Beat Visualizer ──
            show_beat_visualizer(ui, state.beat_phase);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // ── Organism Rhythm Matrix ──
            show_organism_matrix(ui, state, panel, &mut actions);
        });

    actions
}

// ─── Global Controls ─────────────────────────────────────────────────────

fn show_global_controls(
    ui: &mut egui::Ui,
    state: &mut GroovePanelState,
    actions: &mut Vec<GrooveAction>,
    panel: &OrganismPanelState,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Global")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_gray(180)),
        );
    });

    ui.add_space(2.0);

    ui.horizontal(|ui| {
        // Grid dropdown
        ui.label(egui::RichText::new("Grid").size(10.0));
        let current_label = grid_label(state.global_grid);
        egui::ComboBox::from_id_salt("global_grid")
            .selected_text(current_label)
            .width(60.0)
            .show_ui(ui, |ui| {
                for &(value, label) in GRID_PRESETS {
                    if ui.selectable_label((state.global_grid - value).abs() < 0.01, label).clicked() {
                        state.global_grid = value;
                        state.template_idx = None; // Manual override breaks template
                        actions.push(GrooveAction::SetGlobalGrid(value));
                    }
                }
            });

        ui.add_space(12.0);

        // Swing slider
        ui.label(egui::RichText::new("Swing").size(10.0));
        let mut swing = state.global_swing;
        let slider = egui::Slider::new(&mut swing, 0.0..=1.0)
            .max_decimals(2)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
        if ui.add(slider).changed() {
            state.global_swing = swing;
            state.template_idx = None;
            actions.push(GrooveAction::SetGlobalSwing(swing));
        }
    });

    ui.add_space(2.0);

    // Template dropdown
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Template").size(10.0));
        let template_label = state.template_idx
            .and_then(|i| TEMPLATES.get(i))
            .map(|t| t.name)
            .unwrap_or("Manual");
        egui::ComboBox::from_id_salt("groove_template")
            .selected_text(template_label)
            .width(100.0)
            .show_ui(ui, |ui| {
                // Manual option
                if ui.selectable_label(state.template_idx.is_none(), "Manual").clicked() {
                    state.template_idx = None;
                }
                // Template options
                for (i, t) in TEMPLATES.iter().enumerate() {
                    if ui.selectable_label(state.template_idx == Some(i), t.name).clicked() {
                        state.template_idx = Some(i);
                        state.global_grid = t.grid;
                        state.global_swing = t.swing;

                        let selected_mod_id = panel.selected
                            .and_then(|idx| panel.organisms.get(idx))
                            .map(|org| org.mod_id);

                        actions.push(GrooveAction::ApplyTemplate {
                            grid: t.grid,
                            swing: t.swing,
                            tempo_ratio: t.tempo_ratio,
                            selected_mod_id,
                        });
                    }
                }
            });
    });
}

// ─── Beat Visualizer ─────────────────────────────────────────────────────

fn show_beat_visualizer(ui: &mut egui::Ui, beat_phase: f32) {
    let segments = 16;
    let seg_width = 20.0_f32;
    let seg_height = 12.0_f32;
    let spacing = 2.0;
    let total_width = segments as f32 * (seg_width + spacing) - spacing;

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(total_width, seg_height),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);

    let active_segment = (beat_phase * segments as f32).floor() as usize;

    for i in 0..segments {
        let x = rect.left() + i as f32 * (seg_width + spacing);
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            egui::vec2(seg_width, seg_height),
        );

        let is_active = i == active_segment;
        let is_bar_boundary = i % 4 == 0;

        let fill = if is_active {
            egui::Color32::from_rgb(200, 160, 60) // amber glow
        } else if is_bar_boundary {
            egui::Color32::from_rgb(50, 50, 60) // accent bar boundary
        } else {
            egui::Color32::from_rgb(30, 30, 36) // dim
        };

        painter.rect_filled(seg_rect, 1.5, fill);

        // Bar boundary accent marker (small top line)
        if is_bar_boundary {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x + seg_width, rect.top())],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 100, 120)),
            );
        }
    }
}

// ─── Organism Rhythm Matrix ──────────────────────────────────────────────

fn show_organism_matrix(
    ui: &mut egui::Ui,
    state: &mut GroovePanelState,
    panel: &OrganismPanelState,
    actions: &mut Vec<GrooveAction>,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Organisms")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_gray(180)),
        );
    });
    ui.add_space(2.0);

    // Header row
    let hdr = |s: &str| egui::RichText::new(s).size(9.0).color(egui::Color32::from_gray(100));
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(hdr("Name"));
        ui.add_space(32.0);
        ui.label(hdr("Sync"));
        ui.add_space(20.0);
        ui.label(hdr("Swing"));
        ui.add_space(50.0);
        ui.label(hdr("Tempo\u{00D7}"));
        ui.add_space(30.0);
        ui.label(hdr("Quant"));
        ui.add_space(24.0);
        ui.label(hdr("\u{25CF}")); // beat dot header
    });

    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for org in &panel.organisms {
            show_organism_row(ui, state, org, actions);
            ui.add_space(1.0);
        }
    });
}

fn show_organism_row(
    ui: &mut egui::Ui,
    state: &mut GroovePanelState,
    org: &OrganismUiState,
    actions: &mut Vec<GrooveAction>,
) {
    let is_selected = false; // could wire to panel.selected later
    let color = hue_to_color32(org.hue);
    let oid = org.organism_id;

    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(26, 26, 30))
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Name (fixed width)
                let short = if org.name.len() > 5 { &org.name[..5] } else { &org.name };
                ui.label(
                    egui::RichText::new(format!("{} {}", species_icon(&org.species), short))
                        .size(10.0)
                        .color(color),
                );

                // Pad to align columns
                let avail = ui.available_width();
                if avail < 300.0 { return; }

                // Sync dropdown
                let current_sync = *state.org_sync.get(&oid).unwrap_or(&0);
                egui::ComboBox::from_id_salt(format!("sync_{}", oid))
                    .selected_text(sync_label(current_sync))
                    .width(48.0)
                    .show_ui(ui, |ui| {
                        for mode in 0..=2u8 {
                            if ui.selectable_label(current_sync == mode, sync_label(mode)).clicked() {
                                state.org_sync.insert(oid, mode);
                                actions.push(GrooveAction::SetRhythmSync {
                                    mod_id: org.mod_id,
                                    mode,
                                });
                            }
                        }
                    });

                // Swing slider
                let mut swing = *state.org_swing.get(&oid).unwrap_or(&0.5);
                let slider = egui::Slider::new(&mut swing, 0.0..=1.0)
                    .max_decimals(2)
                    .show_value(false);
                if ui.add_sized([70.0, 16.0], slider).changed() {
                    state.org_swing.insert(oid, swing);
                    actions.push(GrooveAction::SetOrgSwing {
                        organism_id: oid,
                        swing,
                    });
                }

                // Tempo× slider
                let mut tempo = *state.org_tempo_ratio.get(&oid).unwrap_or(&1.0);
                let t_slider = egui::Slider::new(&mut tempo, 0.5..=2.0)
                    .max_decimals(2)
                    .custom_formatter(|v, _| format!("{:.2}\u{00D7}", v));
                if ui.add_sized([70.0, 16.0], t_slider).changed() {
                    state.org_tempo_ratio.insert(oid, tempo);
                    actions.push(GrooveAction::SetTempoRatio {
                        mod_id: org.mod_id,
                        ratio: tempo,
                    });
                }

                // Quant dropdown (per-organism grid override)
                let has_override = state.org_grid_overrides.contains_key(&oid);
                let current_grid = state.org_grid_overrides.get(&oid).copied()
                    .unwrap_or(state.global_grid);
                let quant_label = if has_override {
                    grid_label(current_grid)
                } else {
                    "global"
                };
                egui::ComboBox::from_id_salt(format!("quant_{}", oid))
                    .selected_text(quant_label)
                    .width(52.0)
                    .show_ui(ui, |ui| {
                        // "global" option: remove override
                        if ui.selectable_label(!has_override, "global").clicked() {
                            state.org_grid_overrides.remove(&oid);
                            actions.push(GrooveAction::SetOrgGrid {
                                mod_id: org.mod_id,
                                grid: state.global_grid,
                            });
                        }
                        for &(value, label) in GRID_PRESETS {
                            let is_current = has_override && (current_grid - value).abs() < 0.01;
                            if ui.selectable_label(is_current, label).clicked() {
                                state.org_grid_overrides.insert(oid, value);
                                actions.push(GrooveAction::SetOrgGrid {
                                    mod_id: org.mod_id,
                                    grid: value,
                                });
                            }
                        }
                    });

                // Pulsing beat dot
                let dot_phase = state.beat_phase;
                let pulse = (dot_phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                let brightness = (80.0 + pulse * 175.0) as u8;
                let radius = 3.0 + pulse * 2.0;
                let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                let center = dot_rect.center();
                ui.painter().circle_filled(
                    center,
                    radius,
                    egui::Color32::from_rgb(brightness, (brightness as f32 * 0.8) as u8, 40),
                );
            });
        });
}
