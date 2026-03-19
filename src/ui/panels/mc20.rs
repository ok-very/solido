//! MC20 Controller — schematic mapping interface.
//!
//! Blueprint-style view of the selected organism's architecture:
//! cell blocks showing param names/values with CC mapping overlays,
//! patch bay for signal routing, and CC learn workflow.
//!
//! This is a REFERENCE + MAPPING view — parameter editing is done
//! in the synth detail panel (rotary knobs). The MC20 shows the big
//! picture: what's connected to what, which CCs control which params.

use crate::audio::midi_bus::CcLearnState;
use crate::module::ModuleId;
use crate::ui::panels::organism_panel::{
    CellUiState, OrganismPanelState, OrganismUiState, hue_to_color32, species_icon,
};

// ─── Actions ─────────────────────────────────────────────────────────

pub enum Mc20Action {
    SelectOrganism(usize),
    StartCcLearn { param_key: String, range: (f32, f32) },
    RemoveCcMapping(usize),
    ToggleCrListening(ModuleId),
}

// ─── Patch Bay State ─────────────────────────────────────────────────

pub struct PatchBayState {
    pub selected_output: Option<usize>,
    pub connections: Vec<(usize, usize)>,
}

impl PatchBayState {
    pub fn new() -> Self {
        Self {
            selected_output: None,
            connections: Vec::new(),
        }
    }
}

// ─── Constants ───────────────────────────────────────────────────────

const BG: egui::Color32 = egui::Color32::from_rgb(12, 12, 18);
const BLOCK_BG: egui::Color32 = egui::Color32::from_rgb(18, 18, 26);
const DIM: egui::Color32 = egui::Color32::from_gray(55);
const PARAM_NAME: egui::Color32 = egui::Color32::from_gray(95);
const PARAM_VAL: egui::Color32 = egui::Color32::from_gray(170);
const CC_BADGE: egui::Color32 = egui::Color32::from_rgb(255, 180, 50);
const LEARN_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 180, 50);

const OUTPUT_JACKS: &[&str] = &[
    "PITCH", "GATE", "VEL", "VALENCE", "AROUSAL", "CONSON", "CHAOS",
    "SEQ", "LFO", "EG", "CONTOUR", "DENSITY", "BEAT.PH",
];
const INPUT_JACKS: &[&str] = &[
    "FREQ.CV", "CUTOFF", "GAIN", "PW", "DRIVE", "CALM", "LINK.B",
    "RATE.CV", "DEPTH", "TRIG", "SWING", "CHAOS.IN", "FIELD", "AVOID.B",
];
const CABLE_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(220, 90, 90),
    egui::Color32::from_rgb(90, 200, 110),
    egui::Color32::from_rgb(90, 140, 220),
    egui::Color32::from_rgb(220, 180, 70),
    egui::Color32::from_rgb(180, 100, 220),
    egui::Color32::from_rgb(80, 200, 200),
    egui::Color32::from_rgb(220, 130, 70),
    egui::Color32::from_rgb(180, 180, 200),
];

// ─── Main ────────────────────────────────────────────────────────────

pub fn show_mc20(
    ctx: &egui::Context,
    open: &mut bool,
    panel: &OrganismPanelState,
    midi_armed_idx: Option<usize>,
    cc_learn: &CcLearnState,
    patch_bay: &mut PatchBayState,
    midi_port: Option<&str>,
) -> Vec<Mc20Action> {
    let mut actions = Vec::new();

    egui::Window::new(format!("{} MC20", egui_phosphor::regular::SLIDERS))
        .open(open)
        .default_pos([80.0, 40.0])
        .default_size([720.0, 480.0])
        .min_width(400.0)
        .min_height(240.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(BG))
        .show(ctx, |ui| {
            // Header
            show_header(ui, panel, midi_armed_idx, midi_port, cc_learn, &mut actions);
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(4.0);

            // Schematic
            let patch_h = 90.0;
            let grid_h = (ui.available_height() - patch_h).max(80.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), grid_h), |ui| {
                if let Some(idx) = panel.selected {
                    if let Some(org) = panel.organisms.get(idx) {
                        show_schematic(ui, org, idx, cc_learn, &mut actions);
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(if panel.organisms.is_empty() {
                            "No organisms"
                        } else {
                            "Select an organism"
                        }).size(12.0).color(DIM));
                    });
                }
            });

            ui.separator();
            ui.add_space(2.0);

            // Patch bay + CC map
            show_patch_bay(ui, patch_bay, cc_learn, &mut actions);
        });

    actions
}

// ─── Header ──────────────────────────────────────────────────────────

fn show_header(
    ui: &mut egui::Ui,
    panel: &OrganismPanelState,
    midi_armed_idx: Option<usize>,
    midi_port: Option<&str>,
    cc_learn: &CcLearnState,
    actions: &mut Vec<Mc20Action>,
) {
    ui.horizontal(|ui| {
        for (i, org) in panel.organisms.iter().enumerate() {
            let sel = panel.selected == Some(i);
            let armed = midi_armed_idx == Some(i);
            let color = hue_to_color32(org.hue);
            let short = if org.name.len() > 6 { &org.name[..6] } else { &org.name };
            let label = if armed {
                format!("{} {} {}", egui_phosphor::regular::KEYBOARD, species_icon(&org.species), short)
            } else {
                format!("{} {}", species_icon(&org.species), short)
            };
            let text_c = if sel { egui::Color32::WHITE } else { egui::Color32::from_gray(120) };
            let bg = if sel { egui::Color32::from_rgb(24, 24, 34) } else { egui::Color32::TRANSPARENT };
            let border = if sel { color } else { egui::Color32::from_gray(28) };

            if ui.add(
                egui::Button::new(egui::RichText::new(&label).size(10.0).color(text_c))
                    .fill(bg)
                    .stroke(egui::Stroke::new(if sel { 1.0 } else { 0.5 }, border))
                    .corner_radius(3.0),
            ).clicked() {
                actions.push(Mc20Action::SelectOrganism(i));
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (cc_text, cc_color) = if cc_learn.learning {
                ("CC:LEARN".to_string(), LEARN_COLOR)
            } else {
                (format!("CC:{}", cc_learn.mappings.len()), DIM)
            };
            ui.label(egui::RichText::new(cc_text).size(9.0).monospace().color(cc_color));
            ui.separator();
            ui.label(egui::RichText::new(format!("MIDI:{}", midi_port.unwrap_or("---")))
                .size(9.0).monospace().color(DIM));
        });
    });
}

// ─── Schematic ───────────────────────────────────────────────────────

fn show_schematic(
    ui: &mut egui::Ui,
    org: &OrganismUiState,
    panel_idx: usize,
    cc_learn: &CcLearnState,
    actions: &mut Vec<Mc20Action>,
) {
    let prefix = format!("{}_{}", org.species.to_uppercase(), panel_idx);

    // Organism name
    let color = hue_to_color32(org.hue);
    ui.label(
        egui::RichText::new(format!("{} {}", species_icon(&org.species), org.name))
            .size(11.0).strong().color(color),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let avail_w = ui.available_width();
        let col_count = if avail_w > 600.0 { 3 } else if avail_w > 380.0 { 2 } else { 1 };

        // Sort cells by category for synth-panel grouping
        let mut indexed: Vec<(usize, &CellUiState)> = org.cells.iter().enumerate().collect();
        indexed.sort_by_key(|(_, c)| cell_category(&c.cell_type));

        ui.columns(col_count, |columns| {
            for (ci, (cell_idx, cell)) in indexed.iter().enumerate() {
                let col = ci % col_count;
                show_cell_block(&mut columns[col], cell, *cell_idx, &prefix, cc_learn, actions);
                columns[col].add_space(5.0);
            }
        });
    });
}

// ─── Cell Block (read-only schematic) ────────────────────────────────

fn show_cell_block(
    ui: &mut egui::Ui,
    cell: &CellUiState,
    cell_idx: usize,
    prefix: &str,
    cc_learn: &CcLearnState,
    actions: &mut Vec<Mc20Action>,
) {
    let accent = cell_accent(&cell.cell_type);
    let bypassed = cell.bypass.value() >= 0.5;
    let border_alpha: u8 = if bypassed { 20 } else { 60 };
    let border = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), border_alpha);

    egui::Frame::NONE
        .fill(BLOCK_BG)
        .stroke(egui::Stroke::new(if bypassed { 0.3 } else { 0.8 }, border))
        .corner_radius(2.0)
        .inner_margin(egui::Margin::symmetric(5, 3))
        .show(ui, |ui| {
            // Header
            let hdr_color = if bypassed { DIM } else { accent };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        cell_icon(&cell.cell_type),
                        short_cell_name(&cell.cell_type),
                    ))
                    .size(9.0)
                    .strong()
                    .monospace()
                    .color(hdr_color),
                );
                if bypassed {
                    ui.label(egui::RichText::new("OFF").size(8.0).color(egui::Color32::from_gray(45)));
                }
            });

            // Param rows: name | value | CC badge
            // Right-click row → CC learn
            for (name, handle) in &cell.params {
                let val = handle.value();
                let param_key = format!("{}.cell{}.{}", prefix, cell_idx, name);
                let cc_num = find_cc(&cc_learn.mappings, &param_key);
                let learning = cc_learn.learn_target.as_ref().map_or(false, |t| *t == param_key);

                let row = ui.horizontal(|ui| {
                    // Name
                    ui.label(
                        egui::RichText::new(name.as_str())
                            .size(9.0)
                            .monospace()
                            .color(if bypassed { egui::Color32::from_gray(50) } else { PARAM_NAME }),
                    );
                    // Value
                    ui.label(
                        egui::RichText::new(format_val(val))
                            .size(9.0)
                            .monospace()
                            .color(if bypassed { egui::Color32::from_gray(40) } else { PARAM_VAL }),
                    );
                    // CC badge or learn indicator
                    if learning {
                        ui.label(
                            egui::RichText::new("LEARN")
                                .size(8.0)
                                .monospace()
                                .strong()
                                .color(LEARN_COLOR),
                        );
                    } else if let Some(cc) = cc_num {
                        ui.label(
                            egui::RichText::new(format!("CC{cc}"))
                                .size(8.0)
                                .monospace()
                                .color(CC_BADGE),
                        );
                    }
                });

                // Right-click param row → start CC learn
                if row.response.interact(egui::Sense::click()).secondary_clicked() {
                    let (min, max) = cell.param_ranges.iter()
                        .find(|(n, _, _)| n == name)
                        .map(|(_, mn, mx)| (*mn, *mx))
                        .unwrap_or((0.0, 1.0));
                    actions.push(Mc20Action::StartCcLearn { param_key, range: (min, max) });
                }
            }
        });
}

// ─── Patch Bay ───────────────────────────────────────────────────────

fn show_patch_bay(
    ui: &mut egui::Ui,
    patch_bay: &mut PatchBayState,
    cc_learn: &CcLearnState,
    actions: &mut Vec<Mc20Action>,
) {
    let mut out_pos: Vec<egui::Pos2> = Vec::with_capacity(OUTPUT_JACKS.len());
    let mut in_pos: Vec<egui::Pos2> = Vec::with_capacity(INPUT_JACKS.len());

    // Outputs
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("OUT").size(9.0).monospace().color(DIM));
        for (i, name) in OUTPUT_JACKS.iter().enumerate() {
            let sel = patch_bay.selected_output == Some(i);
            let conn = patch_bay.connections.iter().any(|(o, _)| *o == i);
            let color = if sel { CC_BADGE } else if conn { egui::Color32::from_rgb(140,170,200) } else { egui::Color32::from_rgb(100,100,120) };
            let bg = if sel { egui::Color32::from_rgb(36,32,16) } else { egui::Color32::from_rgb(20,20,26) };
            let r = ui.add(egui::Button::new(egui::RichText::new(format!("\u{25ce}{name}").as_str()).size(9.0).monospace().color(color)).fill(bg).corner_radius(2.0));
            out_pos.push(r.rect.center());
            if r.clicked() { patch_bay.selected_output = if sel { None } else { Some(i) }; }
            r.on_hover_text(format!("Output: {name}"));
        }
    });

    ui.add_space(1.0);

    // Inputs
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("IN ").size(9.0).monospace().color(DIM));
        for (i, name) in INPUT_JACKS.iter().enumerate() {
            let conn = patch_bay.connections.iter().any(|(_, inp)| *inp == i);
            let compat = patch_bay.selected_output.is_some();
            let color = if compat && !conn { egui::Color32::from_rgb(90,200,100) } else if conn { egui::Color32::from_rgb(100,150,210) } else { egui::Color32::from_rgb(85,85,105) };
            let r = ui.add(egui::Button::new(egui::RichText::new(format!("\u{25cf}{name}").as_str()).size(9.0).monospace().color(color)).fill(egui::Color32::from_rgb(20,20,26)).corner_radius(2.0));
            in_pos.push(r.rect.center());
            if r.clicked() {
                if let Some(out_idx) = patch_bay.selected_output {
                    if let Some(pos) = patch_bay.connections.iter().position(|(o, inp)| *o == out_idx && *inp == i) {
                        patch_bay.connections.remove(pos);
                    } else {
                        patch_bay.connections.push((out_idx, i));
                    }
                    patch_bay.selected_output = None;
                }
            }
            r.on_hover_text(format!("Input: {name}"));
        }
    });

    // Cables
    if !patch_bay.connections.is_empty() {
        let painter = ui.painter();
        for (ci, &(oi, ii)) in patch_bay.connections.iter().enumerate() {
            if let (Some(&from), Some(&to)) = (out_pos.get(oi), in_pos.get(ii)) {
                paint_cable(painter, from, to, CABLE_COLORS[ci % CABLE_COLORS.len()]);
            }
        }
    }

    ui.add_space(2.0);
    ui.separator();

    // CC map strip
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("CC").size(9.0).monospace().strong().color(egui::Color32::from_gray(80)));
        ui.add_space(4.0);
        let mut remove = None;
        for (i, m) in cc_learn.mappings.iter().enumerate() {
            let short = m.target.rsplit('.').next().unwrap_or(&m.target);
            ui.label(egui::RichText::new(format!("{}\u{2192}{short}", m.cc)).size(9.0).monospace().color(egui::Color32::from_rgb(140,160,190)));
            if ui.add(egui::Button::new(egui::RichText::new("\u{00d7}").size(9.0).color(egui::Color32::from_rgb(180,80,80))).fill(egui::Color32::TRANSPARENT).frame(false)).clicked() {
                remove = Some(i);
            }
            ui.add_space(3.0);
        }
        if let Some(i) = remove { actions.push(Mc20Action::RemoveCcMapping(i)); }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if cc_learn.learning {
                ui.label(egui::RichText::new("LEARNING...").size(9.0).monospace().color(LEARN_COLOR));
            } else {
                ui.label(egui::RichText::new("right-click param to map CC").size(8.0).color(DIM));
            }
        });
    });
}

// ─── Cable Rendering ─────────────────────────────────────────────────

fn paint_cable(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32) {
    let n = 20;
    let sag = 18.0;
    let pts: Vec<egui::Pos2> = (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        egui::pos2(
            from.x + t * (to.x - from.x),
            from.y + t * (to.y - from.y) + sag * 4.0 * t * (1.0 - t),
        )
    }).collect();
    let glow = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 20);
    painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(4.0, glow)));
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, color)));
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn find_cc(mappings: &[crate::audio::midi_bus::CcMapping], key: &str) -> Option<u8> {
    mappings.iter().find(|m| m.target == key).map(|m| m.cc)
}

fn format_val(val: f32) -> String {
    let abs = val.abs();
    if abs >= 1000.0 { format!("{:.1}k", val / 1000.0) }
    else if abs >= 100.0 { format!("{:.0}", val) }
    else if abs >= 10.0 { format!("{:.1}", val) }
    else { format!("{:.2}", val) }
}

fn cell_category(cell_type: &str) -> u8 {
    match cell_type {
        "seq_cell" | "logic_seq_cell" | "melodic_cell" | "walk_cell" => 0,
        "osc_cell" | "saw_bank_cell" | "sample_cell" | "drum_voice_cell"
            | "strike_voice_cell" | "noise_burst_cell" => 1,
        "filter_cell" | "diode_filter_cell" => 2,
        "lfo_cell" | "env_cell" | "accent_env_cell" | "func_gen_cell" | "slew_cell" => 3,
        "mixer_cell" => 4,
        _ => 5,
    }
}

fn cell_accent(cell_type: &str) -> egui::Color32 {
    match cell_type {
        "osc_cell" | "saw_bank_cell" => egui::Color32::from_rgb(210, 160, 60),
        "filter_cell" | "diode_filter_cell" => egui::Color32::from_rgb(70, 140, 200),
        "seq_cell" => egui::Color32::from_rgb(80, 180, 100),
        "lfo_cell" => egui::Color32::from_rgb(150, 100, 200),
        "env_cell" | "accent_env_cell" => egui::Color32::from_rgb(80, 180, 180),
        "sample_cell" => egui::Color32::from_rgb(200, 110, 70),
        "melodic_cell" | "walk_cell" => egui::Color32::from_rgb(200, 170, 70),
        "mixer_cell" => egui::Color32::from_rgb(140, 140, 155),
        "xy_pad_cell" => egui::Color32::from_rgb(140, 130, 200),
        "slew_cell" => egui::Color32::from_rgb(120, 160, 120),
        "func_gen_cell" => egui::Color32::from_rgb(170, 120, 150),
        "logic_seq_cell" => egui::Color32::from_rgb(100, 200, 140),
        "call_response_cell" => egui::Color32::from_rgb(200, 160, 100),
        "noise_burst_cell" => egui::Color32::from_rgb(170, 170, 180),
        "drum_voice_cell" | "strike_voice_cell" => egui::Color32::from_rgb(200, 130, 100),
        _ => egui::Color32::from_rgb(120, 120, 135),
    }
}

fn cell_icon(cell_type: &str) -> &'static str {
    match cell_type {
        "osc_cell" | "saw_bank_cell" => egui_phosphor::regular::WAVE_SINE,
        "filter_cell" | "diode_filter_cell" => egui_phosphor::regular::FUNNEL,
        "seq_cell" | "logic_seq_cell" => egui_phosphor::regular::LIST_NUMBERS,
        "lfo_cell" => egui_phosphor::regular::WAVE_TRIANGLE,
        "env_cell" | "accent_env_cell" => egui_phosphor::regular::CHART_LINE_UP,
        "sample_cell" => egui_phosphor::regular::SPEAKER_HIGH,
        "melodic_cell" | "walk_cell" => egui_phosphor::regular::MUSIC_NOTES,
        "mixer_cell" => egui_phosphor::regular::SLIDERS_HORIZONTAL,
        "xy_pad_cell" => egui_phosphor::regular::CROSSHAIR,
        "slew_cell" => egui_phosphor::regular::ARROW_BEND_RIGHT_DOWN,
        "func_gen_cell" => egui_phosphor::regular::FUNCTION,
        "call_response_cell" => egui_phosphor::regular::CHAT_CIRCLE,
        "noise_burst_cell" => egui_phosphor::regular::BROADCAST,
        _ => egui_phosphor::regular::CUBE,
    }
}

fn short_cell_name(cell_type: &str) -> &'static str {
    match cell_type {
        "osc_cell" => "OSC",
        "saw_bank_cell" => "SAW BANK",
        "filter_cell" => "FILTER",
        "diode_filter_cell" => "DIODE",
        "seq_cell" => "SEQ",
        "logic_seq_cell" => "LOGIC SEQ",
        "lfo_cell" => "LFO",
        "env_cell" => "ENV",
        "accent_env_cell" => "ACCENT",
        "sample_cell" => "SAMPLE",
        "melodic_cell" => "MELODIC",
        "walk_cell" => "WALK",
        "mixer_cell" => "MIXER",
        "slew_cell" => "SLEW",
        "func_gen_cell" => "FUNC GEN",
        "noise_burst_cell" => "NOISE",
        "drum_voice_cell" => "DRUM",
        "strike_voice_cell" => "STRIKE",
        _ => "CELL",
    }
}
