//! Reusable rotary dial widget — first entry in the Solido UI component library.
//!
//! N discrete positions, angular drag to rotate, snap to nearest.
//! Bioluminescent aesthetic: dark body, glow rings, bright indicator line.

use std::f32::consts::{FRAC_PI_2, TAU};

/// Configuration for a RotaryDial widget.
pub struct RotaryDial {
    /// Number of discrete positions (e.g. 12 for circle of fifths).
    pub positions: usize,
    /// Current selected position [0, positions).
    pub value: usize,
    /// Outer radius of the dial body in pixels.
    pub radius: f32,
    /// Color for the indicator glow.
    pub accent: egui::Color32,
}

/// Response from a RotaryDial interaction.
pub struct RotaryDialResponse {
    pub changed: bool,
    pub value: usize,
    pub response: egui::Response,
}

// ─── Pure Math (reusable by CircleOfFifths) ─────────────────────────────

/// Angle from center to pointer, in radians. 0 = right, increases counter-clockwise.
pub fn pointer_angle(center: egui::Pos2, pointer: egui::Pos2) -> f32 {
    let dx = pointer.x - center.x;
    let dy = pointer.y - center.y;
    dy.atan2(dx)
}

/// Convert a raw angle (atan2 convention) to a discrete position index.
/// Position 0 is at top (12 o'clock), increasing clockwise.
pub fn angle_to_position(angle: f32, positions: usize) -> usize {
    // Shift so 0 is at top: subtract -PI/2 (top), then negate for clockwise
    let normalized = (angle + FRAC_PI_2).rem_euclid(TAU);
    let sector = normalized / (TAU / positions as f32);
    (sector.round() as usize) % positions
}

/// Convert a discrete position to an angle in radians.
/// Position 0 = top (12 o'clock), clockwise.
pub fn position_to_angle(position: usize, positions: usize) -> f32 {
    let frac = position as f32 / positions as f32;
    frac * TAU - FRAC_PI_2
}

/// Point on a circle at the given angle and radius from center.
pub fn point_on_circle(center: egui::Pos2, radius: f32, angle: f32) -> egui::Pos2 {
    egui::pos2(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}

// ─── Render-only (called by CircleOfFifths with an existing painter) ────

/// Paint the rotary dial body + indicator at a given center.
/// Does NOT handle interaction — just rendering.
pub fn paint_dial(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    value: usize,
    positions: usize,
    accent: egui::Color32,
) {
    // Dark body
    painter.circle_filled(center, radius, egui::Color32::from_rgb(18, 18, 22));

    // Glow rings (bioluminescent)
    let glow = |alpha: u8, r_frac: f32| {
        let c = egui::Color32::from_rgba_unmultiplied(
            accent.r(), accent.g(), accent.b(), alpha,
        );
        painter.circle_stroke(center, radius * r_frac, egui::Stroke::new(1.0, c));
    };
    glow(8, 0.30);
    glow(5, 0.60);
    glow(3, 0.85);

    // Outer ring
    painter.circle_stroke(
        center, radius,
        egui::Stroke::new(1.0, egui::Color32::from_gray(35)),
    );

    // Tick marks at each position
    for i in 0..positions {
        let angle = position_to_angle(i, positions);
        let dot_pos = point_on_circle(center, radius * 0.88, angle);
        let dot_color = if i == value {
            accent
        } else {
            egui::Color32::from_gray(40)
        };
        painter.circle_filled(dot_pos, 1.5, dot_color);
    }

    // Indicator line (glow behind + bright on top)
    let ind_angle = position_to_angle(value, positions);
    let inner = point_on_circle(center, radius * 0.15, ind_angle);
    let outer = point_on_circle(center, radius * 0.82, ind_angle);

    // Glow
    let glow_color = egui::Color32::from_rgba_unmultiplied(
        accent.r(), accent.g(), accent.b(), 30,
    );
    painter.line_segment([inner, outer], egui::Stroke::new(5.0, glow_color));
    // Bright line
    painter.line_segment([inner, outer], egui::Stroke::new(1.5, accent));
}

// ─── Standalone Widget ──────────────────────────────────────────────────

/// Show a standalone RotaryDial widget. Allocates space, handles interaction.
pub fn show(dial: &RotaryDial, ui: &mut egui::Ui) -> RotaryDialResponse {
    let size = egui::vec2(dial.radius * 2.0, dial.radius * 2.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let center = rect.center();

    let mut value = dial.value;
    let mut changed = false;

    // Drag or click → compute angle, snap to position
    if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let dist = ((pointer.x - center.x).powi(2) + (pointer.y - center.y).powi(2)).sqrt();
            if dist > dial.radius * 0.1 {
                let angle = pointer_angle(center, pointer);
                let new_pos = angle_to_position(angle, dial.positions);
                if new_pos != value {
                    value = new_pos;
                    changed = true;
                }
            }
        }
    }

    // Paint
    let painter = ui.painter_at(rect);
    paint_dial(&painter, center, dial.radius, value, dial.positions, dial.accent);

    RotaryDialResponse { changed, value, response }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_0_is_top() {
        let angle = position_to_angle(0, 12);
        // Top = -PI/2 radians
        assert!((angle - (-FRAC_PI_2)).abs() < 0.01);
    }

    #[test]
    fn angle_to_position_roundtrip() {
        for pos in 0..12 {
            let angle = position_to_angle(pos, 12);
            let recovered = angle_to_position(angle, 12);
            assert_eq!(recovered, pos, "roundtrip failed for position {}", pos);
        }
    }

    #[test]
    fn angle_to_position_wraps() {
        // Angle just past position 11 should wrap to 0
        let angle_11 = position_to_angle(11, 12);
        let nudged = angle_11 + 0.3; // past the last sector
        let pos = angle_to_position(nudged, 12);
        assert!(pos == 0 || pos == 11, "should wrap near boundary, got {}", pos);
    }

    #[test]
    fn position_to_angle_spacing() {
        // Adjacent positions should be TAU/12 apart
        let a0 = position_to_angle(0, 12);
        let a1 = position_to_angle(1, 12);
        let diff = (a1 - a0).abs();
        let expected = TAU / 12.0;
        assert!((diff - expected).abs() < 0.01, "spacing should be TAU/12, got {}", diff);
    }

    #[test]
    fn angle_to_position_all_sectors() {
        // Sample angles around the circle, each should map to a unique position
        let mut seen = std::collections::HashSet::new();
        for i in 0..12 {
            let angle = position_to_angle(i, 12);
            let pos = angle_to_position(angle, 12);
            seen.insert(pos);
        }
        assert_eq!(seen.len(), 12, "all 12 positions should be reachable");
    }
}
