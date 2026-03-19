//! Parameter knob — rotary control for continuous synthesizer parameters.
//!
//! 270° sweep arc with bioluminescent glow, vertical drag to adjust.
//! Shift+drag for fine control. Double-click resets to midpoint.
//! Right-click returns a flag for CC learn initiation.

use std::f32::consts::PI;

// ─── Geometry ────────────────────────────────────────────────────────

/// Start angle: 7:30 position (bottom-left in screen coords).
const ARC_START: f32 = 0.75 * PI;
/// Sweep: 270° clockwise.
const ARC_SWEEP: f32 = 1.5 * PI;
/// Line segments per arc.
const ARC_SEGMENTS: usize = 24;

// ─── Colors ──────────────────────────────────────────────────────────

const TRACK_COLOR: egui::Color32 = egui::Color32::from_rgb(35, 35, 42);
const VALUE_COLOR: egui::Color32 = egui::Color32::from_rgb(190, 190, 210);
const LABEL_COLOR: egui::Color32 = egui::Color32::from_rgb(85, 85, 100);
const LEARN_GLOW: egui::Color32 = egui::Color32::from_rgb(255, 180, 50);

// ─── Response ────────────────────────────────────────────────────────

pub struct ParamKnobResponse {
    pub changed: bool,
    pub value: f32,
    /// True when the user right-clicked — start CC learn for this param.
    pub right_clicked: bool,
}

// ─── Widget ──────────────────────────────────────────────────────────

/// Draw an interactive parameter knob.
///
/// Returns the new value and interaction flags. The caller is responsible
/// for writing the value back to the Shared handle if `changed` is true.
pub fn show(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    range: (f32, f32),
    logarithmic: bool,
    accent: egui::Color32,
    learning: bool,
) -> ParamKnobResponse {
    let r: f32 = 18.0;
    let w = r * 2.0 + 10.0;
    let h = r * 2.0 + 24.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click_and_drag());

    let center = egui::pos2(rect.center().x, rect.top() + r + 2.0);
    let arc_r = r * 0.82;
    let (min, max) = range;
    let norm = normalize(value, min, max, logarithmic);

    // ── Interaction ──────────────────────────────────────────────
    let mut new_val = value;
    let mut changed = false;

    if response.dragged() {
        let dy = -response.drag_delta().y;
        let sens = if ui.input(|i| i.modifiers.shift) { 0.001 } else { 0.004 };
        let n = (norm + dy * sens).clamp(0.0, 1.0);
        new_val = denormalize(n, min, max, logarithmic);
        changed = true;
    }

    if response.double_clicked() {
        new_val = denormalize(0.5, min, max, logarithmic);
        changed = true;
    }

    let final_norm = if changed {
        normalize(new_val, min, max, logarithmic)
    } else {
        norm
    };

    // ── Paint ────────────────────────────────────────────────────
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // CC learn amber halo
        if learning {
            let g = egui::Color32::from_rgba_unmultiplied(255, 180, 50, 18);
            painter.circle_filled(center, r + 2.0, g);
            painter.circle_stroke(center, r, egui::Stroke::new(1.0, LEARN_GLOW));
        }

        // Track arc (background groove)
        paint_arc(
            &painter,
            center,
            arc_r,
            ARC_START,
            ARC_SWEEP,
            egui::Stroke::new(3.0, TRACK_COLOR),
        );

        // Value arc + soft glow
        let sweep = final_norm * ARC_SWEEP;
        if sweep > 0.01 {
            let glow = egui::Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                18,
            );
            paint_arc(&painter, center, arc_r, ARC_START, sweep, egui::Stroke::new(7.0, glow));
            paint_arc(
                &painter,
                center,
                arc_r,
                ARC_START,
                sweep,
                egui::Stroke::new(2.5, accent),
            );
        }

        // Indicator dot at current position
        let a = ARC_START + final_norm * ARC_SWEEP;
        let dot = egui::pos2(center.x + arc_r * a.cos(), center.y + arc_r * a.sin());
        painter.circle_filled(dot, 3.0, accent);

        // Center pip
        painter.circle_filled(center, 1.5, egui::Color32::from_gray(45));

        // Value text (below knob)
        painter.text(
            egui::pos2(rect.center().x, center.y + r + 1.0),
            egui::Align2::CENTER_TOP,
            format_value(new_val),
            egui::FontId::monospace(9.0),
            VALUE_COLOR,
        );

        // Label (below value)
        painter.text(
            egui::pos2(rect.center().x, center.y + r + 12.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(8.0),
            LABEL_COLOR,
        );
    }

    let right_clicked = response.secondary_clicked();
    response.on_hover_text(format!("{label}: {new_val:.4}"));

    ParamKnobResponse {
        changed,
        value: new_val,
        right_clicked,
    }
}

// ─── Internals ───────────────────────────────────────────────────────

fn paint_arc(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    start: f32,
    sweep: f32,
    stroke: egui::Stroke,
) {
    let pts: Vec<egui::Pos2> = (0..=ARC_SEGMENTS)
        .map(|i| {
            let t = i as f32 / ARC_SEGMENTS as f32;
            let a = start + t * sweep;
            egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, stroke));
}

fn normalize(val: f32, min: f32, max: f32, log: bool) -> f32 {
    if log {
        let lo = min.max(0.001).ln();
        let hi = max.max(0.002).ln();
        if (hi - lo).abs() < f32::EPSILON {
            return 0.5;
        }
        ((val.max(0.001).ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        if (max - min).abs() < f32::EPSILON {
            return 0.5;
        }
        ((val - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn denormalize(n: f32, min: f32, max: f32, log: bool) -> f32 {
    if log {
        let lo = min.max(0.001).ln();
        let hi = max.max(0.002).ln();
        (lo + n * (hi - lo)).exp()
    } else {
        min + n * (max - min)
    }
}

fn format_value(val: f32) -> String {
    let abs = val.abs();
    if abs >= 1000.0 {
        format!("{:.1}k", val / 1000.0)
    } else if abs >= 100.0 {
        format!("{:.0}", val)
    } else if abs >= 10.0 {
        format!("{:.1}", val)
    } else {
        format!("{:.2}", val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_roundtrip() {
        for v in [0.0f32, 25.0, 50.0, 100.0] {
            let n = normalize(v, 0.0, 100.0, false);
            let d = denormalize(n, 0.0, 100.0, false);
            assert!((v - d).abs() < 0.01, "failed for {v}");
        }
    }

    #[test]
    fn log_roundtrip() {
        for v in [20.0f32, 200.0, 2000.0, 20000.0] {
            let n = normalize(v, 20.0, 20000.0, true);
            let d = denormalize(n, 20.0, 20000.0, true);
            assert!((v - d).abs() / v < 0.01, "failed for {v}: got {d}");
        }
    }

    #[test]
    fn normalize_edge_cases() {
        assert_eq!(normalize(5.0, 5.0, 5.0, false), 0.5);
        assert_eq!(normalize(-1.0, 0.0, 100.0, false), 0.0);
        assert_eq!(normalize(200.0, 0.0, 100.0, false), 1.0);
    }

    #[test]
    fn format_ranges() {
        assert_eq!(format_value(0.42), "0.42");
        assert_eq!(format_value(5.3), "5.30");
        assert_eq!(format_value(130.0), "130");
        assert_eq!(format_value(2500.0), "2.5k");
    }
}
