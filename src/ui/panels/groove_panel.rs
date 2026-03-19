//! Groove panel — global grid/swing controls, per-organism rhythm matrix,
//! groove templates, and beat visualizer.

use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{
    hue_to_color32, OrganismPanelState, OrganismUiState, species_icon,
};

// ─── Grid Division Constants ─────────────────────────────────────────────

const GRID_PRESETS: &[(f32, &str)] = &[
    (0.0, "Free"),
    (1.0, "1/4"),
    (0.5, "1/8"),
    (1.0 / 3.0, "1/8T"),
    (0.25, "1/16"),
    (1.0 / 6.0, "1/16T"),
    (0.125, "1/32"),
];

fn grid_label(value: f32) -> &'static str {
    GRID_PRESETS
        .iter()
        .find(|(v, _)| (*v - value).abs() < 0.01)
        .map(|(_, label)| *label)
        .unwrap_or("?")
}

// ─── Groove Templates ────────────────────────────────────────────────────

struct GrooveTemplate {
    name: &'static str,
    grid: f32,
    swing: f32,
    tempo_ratio: Option<f32>,
}

const TEMPLATES: &[GrooveTemplate] = &[
    GrooveTemplate { name: "Straight",  grid: 0.25,      swing: 0.0,  tempo_ratio: None },
    GrooveTemplate { name: "Swing 60",  grid: 0.5,       swing: 0.6,  tempo_ratio: None },
    GrooveTemplate { name: "Swing 67",  grid: 0.5,       swing: 0.67, tempo_ratio: None },
    GrooveTemplate { name: "Triplet",   grid: 1.0 / 3.0, swing: 0.0,  tempo_ratio: None },
    GrooveTemplate { name: "Shuffle",   grid: 0.25,      swing: 0.62, tempo_ratio: None },
    GrooveTemplate { name: "Laid Back", grid: 0.5,       swing: 0.15, tempo_ratio: None },
    GrooveTemplate { name: "Pushed",    grid: 0.5,       swing: 0.0,  tempo_ratio: None },
    GrooveTemplate { name: "Phasing",   grid: 0.25,      swing: 0.0,  tempo_ratio: Some(1.01) },
];

const SYNC_LABELS: &[&str] = &["none", "soft", "hard"];

fn sync_label(mode: u8) -> &'static str {
    SYNC_LABELS.get(mode as usize).unwrap_or(&"?")
}

// ─── Persistent Panel State ──────────────────────────────────────────────

pub struct GroovePanelState {
    pub global_grid: f32,
    pub global_swing: f32,
    pub template_idx: Option<usize>,
    pub org_grid_overrides: std::collections::HashMap<u32, f32>,
    pub org_sync: std::collections::HashMap<u32, u8>,
    pub org_tempo_ratio: std::collections::HashMap<u32, f32>,
    pub org_swing: std::collections::HashMap<u32, f32>,
    pub beat_phase: f32,
}

impl GroovePanelState {
    pub fn new() -> Self {
        Self {
            global_grid: 0.0,
            global_swing: 0.0,
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

pub enum GrooveAction {
    SetGlobalGrid(f32),
    SetGlobalSwing(f32),
    SetRhythmSync { mod_id: ModuleId, mode: u8 },
    SetTempoRatio { mod_id: ModuleId, ratio: f32 },
    SetOrgGrid { mod_id: ModuleId, grid: f32 },
    SetOrgSwing { organism_id: u32, swing: f32 },
    ApplyTemplate {
        grid: f32,
        swing: f32,
        tempo_ratio: Option<f32>,
        selected_mod_id: Option<ModuleId>,
    },
}

// ─── Main ────────────────────────────────────────────────────────────────

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
        .min_width(360.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgb(20, 20, 24)))
        .show(ctx, |ui| {
            show_global_controls(ui, state, &mut actions, panel);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            show_beat_visualizer(ui, state.beat_phase);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

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
    ui.label(
        egui::RichText::new("Global")
            .strong()
            .size(11.0)
            .color(egui::Color32::from_gray(180)),
    );
    ui.add_space(2.0);

    egui::Grid::new("groove_global_grid")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            // Row 1: Grid + Swing
            ui.label(egui::RichText::new("Grid").size(10.0));
            let current_label = grid_label(state.global_grid);
            egui::ComboBox::from_id_salt("global_grid")
                .selected_text(current_label)
                .width(60.0)
                .show_ui(ui, |ui| {
                    for &(value, label) in GRID_PRESETS {
                        if ui
                            .selectable_label(
                                (state.global_grid - value).abs() < 0.01,
                                label,
                            )
                            .clicked()
                        {
                            state.global_grid = value;
                            state.template_idx = None;
                            actions.push(GrooveAction::SetGlobalGrid(value));
                        }
                    }
                });

            ui.label(egui::RichText::new("Swing").size(10.0));
            let mut swing = state.global_swing;
            if ui
                .add(
                    egui::Slider::new(&mut swing, 0.0..=1.0)
                        .max_decimals(2)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                state.global_swing = swing;
                state.template_idx = None;
                actions.push(GrooveAction::SetGlobalSwing(swing));
            }
            ui.end_row();

            // Row 2: Template
            ui.label(egui::RichText::new("Template").size(10.0));
            let tpl_label = state
                .template_idx
                .and_then(|i| TEMPLATES.get(i))
                .map(|t| t.name)
                .unwrap_or("Manual");
            egui::ComboBox::from_id_salt("groove_template")
                .selected_text(tpl_label)
                .width(100.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(state.template_idx.is_none(), "Manual")
                        .clicked()
                    {
                        state.template_idx = None;
                    }
                    for (i, t) in TEMPLATES.iter().enumerate() {
                        if ui
                            .selectable_label(state.template_idx == Some(i), t.name)
                            .clicked()
                        {
                            state.template_idx = Some(i);
                            state.global_grid = t.grid;
                            state.global_swing = t.swing;
                            let sel_mod = panel
                                .selected
                                .and_then(|idx| panel.organisms.get(idx))
                                .map(|org| org.mod_id);
                            actions.push(GrooveAction::ApplyTemplate {
                                grid: t.grid,
                                swing: t.swing,
                                tempo_ratio: t.tempo_ratio,
                                selected_mod_id: sel_mod,
                            });
                        }
                    }
                });
            ui.end_row();
        });
}

// ─── Beat Visualizer ─────────────────────────────────────────────────────

fn show_beat_visualizer(ui: &mut egui::Ui, beat_phase: f32) {
    let segments = 16;
    let seg_w = 20.0_f32;
    let seg_h = 12.0_f32;
    let gap = 2.0;
    let total_w = segments as f32 * (seg_w + gap) - gap;

    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(total_w, seg_h), egui::Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let active = (beat_phase * segments as f32).floor() as usize;

    for i in 0..segments {
        let x = rect.left() + i as f32 * (seg_w + gap);
        let seg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(seg_w, seg_h));
        let fill = if i == active {
            egui::Color32::from_rgb(200, 160, 60)
        } else if i % 4 == 0 {
            egui::Color32::from_rgb(50, 50, 60)
        } else {
            egui::Color32::from_rgb(30, 30, 36)
        };
        painter.rect_filled(seg, 1.5, fill);
        if i % 4 == 0 {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x + seg_w, rect.top())],
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
    ui.label(
        egui::RichText::new("Organisms")
            .strong()
            .size(11.0)
            .color(egui::Color32::from_gray(180)),
    );
    ui.add_space(2.0);

    if panel.organisms.is_empty() {
        ui.label(
            egui::RichText::new("No organisms")
                .size(10.0)
                .color(egui::Color32::from_gray(60)),
        );
        return;
    }

    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        egui::Grid::new("organism_rhythm_grid")
            .num_columns(6)
            .spacing([6.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                // Header
                let hdr =
                    |s: &str| egui::RichText::new(s).size(9.0).color(egui::Color32::from_gray(100));
                ui.label(hdr("Name"));
                ui.label(hdr("Sync"));
                ui.label(hdr("Swing"));
                ui.label(hdr("Tempo x"));
                ui.label(hdr("Quant"));
                ui.label(hdr("")); // beat dot
                ui.end_row();

                // Organism rows
                for org in &panel.organisms {
                    show_organism_row(ui, state, org, actions);
                }
            });
    });
}

fn show_organism_row(
    ui: &mut egui::Ui,
    state: &mut GroovePanelState,
    org: &OrganismUiState,
    actions: &mut Vec<GrooveAction>,
) {
    let color = hue_to_color32(org.hue);
    let oid = org.organism_id;

    // Name
    let short = if org.name.len() > 6 {
        &org.name[..6]
    } else {
        &org.name
    };
    ui.label(
        egui::RichText::new(format!("{} {}", species_icon(&org.species), short))
            .size(10.0)
            .color(color),
    );

    // Sync dropdown
    let current_sync = *state.org_sync.get(&oid).unwrap_or(&0);
    egui::ComboBox::from_id_salt(format!("sync_{oid}"))
        .selected_text(sync_label(current_sync))
        .width(48.0)
        .show_ui(ui, |ui| {
            for mode in 0..=2u8 {
                if ui
                    .selectable_label(current_sync == mode, sync_label(mode))
                    .clicked()
                {
                    state.org_sync.insert(oid, mode);
                    actions.push(GrooveAction::SetRhythmSync {
                        mod_id: org.mod_id,
                        mode,
                    });
                }
            }
        });

    // Swing slider
    let mut swing = *state.org_swing.get(&oid).unwrap_or(&0.0);
    if ui
        .add_sized(
            [70.0, 16.0],
            egui::Slider::new(&mut swing, 0.0..=1.0)
                .max_decimals(2)
                .show_value(false),
        )
        .changed()
    {
        state.org_swing.insert(oid, swing);
        actions.push(GrooveAction::SetOrgSwing {
            organism_id: oid,
            swing,
        });
    }

    // Tempo x slider
    let mut tempo = *state.org_tempo_ratio.get(&oid).unwrap_or(&1.0);
    if ui
        .add_sized(
            [70.0, 16.0],
            egui::Slider::new(&mut tempo, 0.5..=2.0)
                .max_decimals(2)
                .custom_formatter(|v, _| format!("{v:.2}x")),
        )
        .changed()
    {
        state.org_tempo_ratio.insert(oid, tempo);
        actions.push(GrooveAction::SetTempoRatio {
            mod_id: org.mod_id,
            ratio: tempo,
        });
    }

    // Quant dropdown
    let has_override = state.org_grid_overrides.contains_key(&oid);
    let current_grid = state
        .org_grid_overrides
        .get(&oid)
        .copied()
        .unwrap_or(state.global_grid);
    let quant_label = if has_override {
        grid_label(current_grid)
    } else {
        "global"
    };
    egui::ComboBox::from_id_salt(format!("quant_{oid}"))
        .selected_text(quant_label)
        .width(52.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(!has_override, "global")
                .clicked()
            {
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

    // Pulsing beat dot (painted, not Unicode text)
    let pulse = (state.beat_phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let brightness = (80.0 + pulse * 175.0) as u8;
    let r = 3.0 + pulse * 2.0;
    let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    if ui.is_rect_visible(dot_rect) {
        ui.painter().circle_filled(
            dot_rect.center(),
            r,
            egui::Color32::from_rgb(brightness, (brightness as f32 * 0.8) as u8, 40),
        );
    }

    ui.end_row();
}
