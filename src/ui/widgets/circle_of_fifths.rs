//! Circle of Fifths widget — constellation scale visualization.
//!
//! Scales are **constellations**: star patterns on the circle where active scale
//! degrees are stars connected by lines. A central rotary dial navigates key
//! selection. The inner ring glows with the constellation pattern.
//!
//! Astral navigation for harmonic space.

use super::rotary_dial;
use crate::tuning::gravity_well::transpose_weights;

/// Circle-of-fifths pitch class ordering (clockwise from top).
/// Each value is the chromatic pitch class (0=C, 7=G, 2=D, ...).
const FIFTHS_ORDER: [u8; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];

const FIFTHS_LABELS: [&str; 12] = [
    "C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F",
];

// ─── Mapping ────────────────────────────────────────────────────────────

/// Convert chromatic pitch class (0-11) to circle-of-fifths position (0-11).
pub fn chromatic_to_fifths(pc: u8) -> usize {
    FIFTHS_ORDER.iter().position(|&x| x == pc % 12).unwrap_or(0)
}

/// Convert circle-of-fifths position (0-11) to chromatic pitch class (0-11).
pub fn fifths_to_chromatic(pos: usize) -> u8 {
    FIFTHS_ORDER[pos % 12]
}

// ─── State & Response ───────────────────────────────────────────────────

/// Input state for the Circle of Fifths widget.
pub struct CircleOfFifthsState {
    /// Currently selected key (0=C, 1=C#, ..., 11=B) — chromatic index.
    pub key: u8,
    /// Current scale's gravity weights (C-rooted, pre-transposition).
    pub gravity_weights: [f32; 12],
    /// Current scale's hue [0, 360) for constellation coloring.
    pub scale_hue: f32,
}

/// Response from the Circle of Fifths widget.
pub struct CircleOfFifthsResponse {
    pub key_changed: bool,
    pub key: u8,
}

// ─── Colors ─────────────────────────────────────────────────────────────

const ROOT_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 180, 60);   // amber
const FIFTH_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 200, 220); // silver
const DIM_DOT: egui::Color32 = egui::Color32::from_rgb(28, 28, 32);
const DARK_BG: egui::Color32 = egui::Color32::from_rgb(14, 14, 18);

fn hue_color(hue: f32, alpha: f32) -> egui::Color32 {
    let hsva = egui::ecolor::Hsva::new(hue / 360.0, 0.65, 0.75, alpha);
    egui::Color32::from(hsva)
}

// ─── Main Widget ────────────────────────────────────────────────────────

/// Draw the Circle of Fifths with constellation overlay.
pub fn show_circle_of_fifths(
    ui: &mut egui::Ui,
    state: &mut CircleOfFifthsState,
) -> CircleOfFifthsResponse {
    let avail = ui.available_width().min(280.0).max(180.0);
    let size = egui::vec2(avail, avail);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let r = avail / 2.0 - 2.0;

    // Zone radii
    let label_r = r - 12.0;
    let constellation_r = r * 0.62;
    let dial_r = r * 0.35;

    // Current key in fifths-order
    let key_fifths_pos = chromatic_to_fifths(state.key);

    // ── Background ──
    painter.circle_filled(center, r, DARK_BG);
    painter.circle_stroke(center, r, egui::Stroke::new(0.5, egui::Color32::from_gray(30)));

    // ── Constellation ──
    let transposed = transpose_weights(&state.gravity_weights, state.key);
    paint_constellation(
        &painter, center, constellation_r, &transposed,
        state.key, state.scale_hue,
    );

    // ── Outer Labels ──
    let label_click = paint_labels(
        &painter, center, label_r, key_fifths_pos, &response,
    );

    // ── Center Dial ──
    rotary_dial::paint_dial(
        &painter, center, dial_r,
        key_fifths_pos, 12,
        ROOT_COLOR,
    );

    // ── Interaction ──
    let mut key = state.key;
    let mut changed = false;

    if let Some(clicked_fifths) = label_click {
        key = fifths_to_chromatic(clicked_fifths);
        changed = key != state.key;
    } else if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let dist = ((pointer.x - center.x).powi(2) + (pointer.y - center.y).powi(2)).sqrt();
            // Drag within dial zone → rotate key
            if dist < dial_r * 1.5 && dist > dial_r * 0.1 {
                let angle = rotary_dial::pointer_angle(center, pointer);
                let new_fifths = rotary_dial::angle_to_position(angle, 12);
                let new_key = fifths_to_chromatic(new_fifths);
                if new_key != state.key {
                    key = new_key;
                    changed = true;
                }
            }
        }
    }

    CircleOfFifthsResponse { key_changed: changed, key }
}

// ─── Constellation Rendering ────────────────────────────────────────────

fn paint_constellation(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    transposed_weights: &[f32; 12],
    key: u8,
    scale_hue: f32,
) {
    let scale_color = hue_color(scale_hue, 1.0);
    let line_color = hue_color(scale_hue, 0.35);

    // Collect active star positions in fifths order for line drawing
    let mut active_stars: Vec<(usize, egui::Pos2, f32)> = Vec::new(); // (fifths_pos, point, weight)

    for pc in 0..12u8 {
        let fifths_pos = chromatic_to_fifths(pc);
        let angle = rotary_dial::position_to_angle(fifths_pos, 12);
        let pt = rotary_dial::point_on_circle(center, radius, angle);
        let weight = transposed_weights[pc as usize];

        if weight > 0.1 {
            active_stars.push((fifths_pos, pt, weight));
        } else {
            // Inactive: dim dot
            painter.circle_filled(pt, 1.5, DIM_DOT);
        }
    }

    // Sort by fifths position for constellation line connections
    active_stars.sort_by_key(|(pos, _, _)| *pos);

    // Constellation lines: connect adjacent active stars, wrap last→first
    if active_stars.len() >= 2 {
        for i in 0..active_stars.len() {
            let j = (i + 1) % active_stars.len();
            painter.line_segment(
                [active_stars[i].1, active_stars[j].1],
                egui::Stroke::new(1.0, line_color),
            );
        }
    }

    // Stars (painted on top of lines)
    let fifth_pc = (key + 7) % 12;
    for &(_, pt, weight) in &active_stars {
        let star_radius = (weight.sqrt() * 3.5).clamp(2.0, 6.0);

        // Determine which pitch class this star represents
        // (recover from position by checking which PC maps to this point)
        let pc = recover_pc_from_point(center, radius, pt);
        let is_root = pc == key;
        let is_fifth = pc == fifth_pc;

        let color = if is_root {
            ROOT_COLOR
        } else if is_fifth {
            FIFTH_COLOR
        } else {
            scale_color
        };

        // Glow halo
        let glow = egui::Color32::from_rgba_unmultiplied(
            color.r(), color.g(), color.b(), 50,
        );
        painter.circle_filled(pt, star_radius + 2.5, glow);
        // Star body
        painter.circle_filled(pt, star_radius, color);
    }
}

/// Recover chromatic pitch class from a point on the constellation circle.
fn recover_pc_from_point(center: egui::Pos2, radius: f32, pt: egui::Pos2) -> u8 {
    let angle = rotary_dial::pointer_angle(center, pt);
    let fifths_pos = rotary_dial::angle_to_position(angle, 12);
    fifths_to_chromatic(fifths_pos)
}

// ─── Label Rendering ────────────────────────────────────────────────────

/// Paint the 12 key labels around the outer ring. Returns Some(fifths_pos) if clicked.
fn paint_labels(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    current_fifths_pos: usize,
    response: &egui::Response,
) -> Option<usize> {
    let font = egui::FontId::monospace(11.0);
    let mut clicked = None;

    for i in 0..12 {
        let angle = rotary_dial::position_to_angle(i, 12);
        let pt = rotary_dial::point_on_circle(center, radius, angle);

        let is_current = i == current_fifths_pos;
        let color = if is_current {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(90)
        };

        painter.text(
            pt,
            egui::Align2::CENTER_CENTER,
            FIFTHS_LABELS[i],
            font.clone(),
            color,
        );

        // Click detection: check if pointer is near this label
        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let dist = ((pointer.x - pt.x).powi(2) + (pointer.y - pt.y).powi(2)).sqrt();
                if dist < 14.0 {
                    clicked = Some(i);
                }
            }
        }
    }

    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromatic_to_fifths_roundtrip() {
        for pc in 0..12u8 {
            let fifths = chromatic_to_fifths(pc);
            let recovered = fifths_to_chromatic(fifths);
            assert_eq!(recovered, pc, "roundtrip failed for pc={}", pc);
        }
    }

    #[test]
    fn fifths_order_is_correct() {
        // C=0 at top, G=7 next clockwise, D=2, A=9, E=4, B=11, F#=6, ...
        assert_eq!(fifths_to_chromatic(0), 0);  // C
        assert_eq!(fifths_to_chromatic(1), 7);  // G
        assert_eq!(fifths_to_chromatic(2), 2);  // D
        assert_eq!(fifths_to_chromatic(3), 9);  // A
        assert_eq!(fifths_to_chromatic(4), 4);  // E
        assert_eq!(fifths_to_chromatic(5), 11); // B
        assert_eq!(fifths_to_chromatic(6), 6);  // F#
        assert_eq!(fifths_to_chromatic(11), 5); // F
    }

    #[test]
    fn all_twelve_pcs_reachable() {
        let mut seen = [false; 12];
        for pos in 0..12 {
            let pc = fifths_to_chromatic(pos);
            seen[pc as usize] = true;
        }
        for (i, &s) in seen.iter().enumerate() {
            assert!(s, "pitch class {} not reachable from fifths order", i);
        }
    }

    #[test]
    fn c_major_constellation_has_7_stars() {
        let weights = [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0];
        let active = weights.iter().filter(|w| **w > 0.1).count();
        assert_eq!(active, 7);
    }
}
