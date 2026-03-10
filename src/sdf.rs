#![allow(dead_code)]
/// Signed distance to axis-aligned box (negative = inside).
///
/// Matches the GPU `sdRoundedBox` with `r = 0`:
///   `length(max(d, 0)) + min(max(d.x, d.y), 0)`
/// where `d = abs(p - center) - half_size`.
///
/// Parameters use top-left + size for caller convenience; internally
/// converts to center + half_size.
pub fn rect(px: f32, py: f32, rx: f32, ry: f32, rw: f32, rh: f32) -> f32 {
    let cx = rx + rw * 0.5;
    let cy = ry + rh * 0.5;
    let hx = rw * 0.5;
    let hy = rh * 0.5;
    sd_box(px, py, cx, cy, hx, hy)
}

/// Signed distance to axis-aligned box given center + half-extents.
///
/// This is the canonical Inigo Quilez box SDF (https://iquilezles.org/articles/distfunctions2d/).
/// Returns negative inside, positive outside, zero on the boundary.
fn sd_box(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32) -> f32 {
    let dx = (px - cx).abs() - hx;
    let dy = (py - cy).abs() - hy;
    // Outside: Euclidean distance to nearest corner/edge
    let outside = (dx.max(0.0) * dx.max(0.0) + dy.max(0.0) * dy.max(0.0)).sqrt();
    // Inside: negative distance to nearest edge (Chebyshev)
    let inside = dx.max(dy).min(0.0);
    outside + inside
}

/// Signed distance to axis-aligned rounded box (negative = inside).
///
/// Exactly matches the GPU shader `sdRoundedBox`:
///   `length(max(d, 0)) + min(max(d.x, d.y), 0) - r`
/// where `d = abs(p - center) - half_size + r`.
///
/// Parameters use top-left + size + corner radius; internally converts
/// to center + half_size.
pub fn rounded_rect(
    px: f32,
    py: f32,
    rx: f32,
    ry: f32,
    rw: f32,
    rh: f32,
    radius: f32,
) -> f32 {
    let cx = rx + rw * 0.5;
    let cy = ry + rh * 0.5;
    let hx = rw * 0.5;
    let hy = rh * 0.5;
    sd_rounded_box(px, py, cx, cy, hx, hy, radius)
}

/// Signed distance to axis-aligned rounded box given center + half-extents + corner radius.
///
/// Directly mirrors the GPU `sdRoundedBox(p, center, half_size, r)`.
/// When `r = 0`, this is identical to `sd_box`.
pub fn sd_rounded_box(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    hx: f32,
    hy: f32,
    r: f32,
) -> f32 {
    let dx = (px - cx).abs() - hx + r;
    let dy = (py - cy).abs() - hy + r;
    let outside = (dx.max(0.0) * dx.max(0.0) + dy.max(0.0) * dy.max(0.0)).sqrt();
    let inside = dx.max(dy).min(0.0);
    outside + inside - r
}

/// Signed distance to circle (negative = inside).
pub fn circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    let dx = px - cx;
    let dy = py - cy;
    (dx * dx + dy * dy).sqrt() - r
}

/// Smooth minimum (polynomial / quadratic) -- Inigo Quilez's smin.
///
/// Reference: https://iquilezles.org/articles/smin/
///
/// C1 continuous.  Equivalent to the GPU `smoothMin`:
///   `mix(b, a, h) - k * h * (1 - h)`
/// where `h = clamp(0.5 + 0.5 * (b - a) / k, 0, 1)`.
///
/// When `k <= 0` (or near zero), degrades gracefully to `min(d1, d2)`.
pub fn smooth_min(d1: f32, d2: f32, k: f32) -> f32 {
    if k <= 1e-6 {
        return d1.min(d2);
    }
    let h = (0.5 + 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
    d2 + (d1 - d2) * h - k * h * (1.0 - h)
}

/// Smooth union of multiple distances.
pub fn smooth_union(distances: &[f32], blend_radius: f32) -> f32 {
    match distances.len() {
        0 => f32::INFINITY,
        1 => distances[0],
        _ => {
            let mut result = distances[0];
            for &d in &distances[1..] {
                result = smooth_min(result, d, blend_radius);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_center_is_negative() {
        // Center of a 10x10 box at origin should be -5.0
        let d = rect(5.0, 5.0, 0.0, 0.0, 10.0, 10.0);
        assert!((d - (-5.0)).abs() < 1e-5, "expected -5.0, got {d}");
    }

    #[test]
    fn rect_outside_is_positive() {
        // Point clearly outside
        let d = rect(20.0, 5.0, 0.0, 0.0, 10.0, 10.0);
        assert!(d > 0.0, "expected positive, got {d}");
        assert!((d - 10.0).abs() < 1e-5, "expected 10.0, got {d}");
    }

    #[test]
    fn rect_on_edge_is_zero() {
        let d = rect(10.0, 5.0, 0.0, 0.0, 10.0, 10.0);
        assert!(d.abs() < 1e-5, "expected ~0.0, got {d}");
    }

    #[test]
    fn rect_corner_euclidean() {
        // Point at (12, 12) relative to box (0,0,10,10) -> dx=2, dy=2 -> sqrt(8)
        let d = rect(12.0, 12.0, 0.0, 0.0, 10.0, 10.0);
        let expected = (2.0_f32 * 2.0 + 2.0 * 2.0).sqrt();
        assert!((d - expected).abs() < 1e-5, "expected {expected}, got {d}");
    }

    #[test]
    fn rounded_rect_matches_rect_when_r_zero() {
        let d_rect = rect(3.0, 4.0, 0.0, 0.0, 10.0, 10.0);
        let d_rounded = rounded_rect(3.0, 4.0, 0.0, 0.0, 10.0, 10.0, 0.0);
        assert!(
            (d_rect - d_rounded).abs() < 1e-5,
            "rect={d_rect}, rounded={d_rounded}"
        );
    }

    #[test]
    fn rounded_rect_shrinks_corners() {
        // At the sharp corner (10, 10) of box (0,0,10,10), the sharp box gives 0.
        // With rounding r=2, the corner is cut away -> positive distance.
        let d_sharp = rect(10.0, 10.0, 0.0, 0.0, 10.0, 10.0);
        let d_round = rounded_rect(10.0, 10.0, 0.0, 0.0, 10.0, 10.0, 2.0);
        assert!(d_sharp.abs() < 1e-5);
        assert!(d_round > 0.0, "expected positive (outside rounded corner), got {d_round}");
    }

    #[test]
    fn smooth_min_degenerates_to_min_when_k_zero() {
        let a = 3.0_f32;
        let b = 7.0_f32;
        let result = smooth_min(a, b, 0.0);
        assert!((result - 3.0).abs() < 1e-5, "expected 3.0, got {result}");
    }

    #[test]
    fn smooth_min_symmetric() {
        let a = 2.0_f32;
        let b = 5.0_f32;
        let k = 1.0_f32;
        let ab = smooth_min(a, b, k);
        let ba = smooth_min(b, a, k);
        assert!((ab - ba).abs() < 1e-5, "smin(a,b)={ab} != smin(b,a)={ba}");
    }

    #[test]
    fn smooth_min_is_less_than_min() {
        let a = 2.0_f32;
        let b = 2.5_f32;
        let k = 1.0_f32;
        let result = smooth_min(a, b, k);
        assert!(result < a.min(b), "smin should be <= min(a,b) in blend region");
    }

    #[test]
    fn smooth_min_far_apart_equals_min() {
        // When |a - b| >> k, smin should equal min
        let a = 1.0_f32;
        let b = 100.0_f32;
        let k = 2.0_f32;
        let result = smooth_min(a, b, k);
        assert!((result - 1.0).abs() < 1e-5, "expected ~1.0, got {result}");
    }
}
