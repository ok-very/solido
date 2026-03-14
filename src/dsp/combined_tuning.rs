/// Merge threshold: micro degrees within this many cents of a 12-TET degree
/// replace the 12-TET position (micro wins position, weight stacks).
pub(crate) const MICRO_MERGE_TOLERANCE: f32 = 20.0;

/// Combined tuning: 12-TET base + up to 12 microtonal overlay degrees merged.
#[derive(Clone, Debug)]
pub(crate) struct CombinedTuning {
    pub cents: [f32; 24],
    pub weights: [f32; 24],
    pub count: u8,
}

impl CombinedTuning {
    pub fn new() -> Self {
        CombinedTuning {
            cents: [0.0; 24],
            weights: [0.0; 24],
            count: 0,
        }
    }
}

/// Circular distance in cents within one octave (1200 cents period).
#[inline]
pub(crate) fn cents_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 1200.0;
    d.min(1200.0 - d)
}

/// RT-safe cents-based quantization: snap Hz to nearest degree in CombinedTuning.
/// Works in cents space for microtonal accuracy. Pure f32 math — no alloc, no locks.
#[inline]
pub(crate) fn quantize_to_tuning(raw_hz: f32, tuning: &CombinedTuning, blend: f32) -> f32 {
    if blend < 0.01 || raw_hz < 20.0 || tuning.count == 0 {
        return raw_hz;
    }
    // Convert Hz → cents from C4 (261.63 Hz)
    let raw_cents = 1200.0 * (raw_hz / 261.63).log2();
    // Reduce to [0, 1200) within octave
    let octave_cents = ((raw_cents % 1200.0) + 1200.0) % 1200.0;
    let octave_base = raw_cents - octave_cents;

    let mut best_cents = octave_cents;
    let mut best_dist = f32::MAX;
    for i in 0..tuning.count as usize {
        let w = tuning.weights[i];
        if w < 0.01 { continue; }
        let dist = cents_distance(octave_cents, tuning.cents[i]);
        let weighted_dist = dist / w;
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_cents = tuning.cents[i];
        }
    }

    let quantized_cents = octave_base + best_cents;
    let quantized_hz = 261.63 * 2.0f32.powf(quantized_cents / 1200.0);
    raw_hz * (1.0 - blend) + quantized_hz * blend
}

/// RT-safe scale quantization: snap Hz to nearest active scale degree, blended.
/// Pure f32 math — no alloc, no locks. 12-iteration loop is O(1).
/// Kept for backward compatibility in non-DSP contexts (e.g. pitch module).
#[inline]
#[allow(dead_code)]
pub(crate) fn quantize_to_scale_fast(raw_hz: f32, gravity: &[f32; 12], blend: f32) -> f32 {
    if blend < 0.01 || raw_hz < 20.0 {
        return raw_hz;
    }
    let midi = 12.0 * (raw_hz / 440.0).log2() + 69.0;
    let octave = (midi / 12.0).floor();
    let degree = midi - octave * 12.0;
    let mut best_degree = degree;
    let mut best_dist = f32::MAX;
    for i in 0..12 {
        if gravity[i] < 0.1 {
            continue;
        }
        let d = i as f32;
        let dist = (degree - d).abs().min(12.0 - (degree - d).abs());
        let weighted_dist = dist / gravity[i];
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_degree = d;
        }
    }
    let quantized_midi = octave * 12.0 + best_degree;
    let quantized_hz = 440.0 * 2.0f32.powf((quantized_midi - 69.0) / 12.0);
    raw_hz * (1.0 - blend) + quantized_hz * blend
}
