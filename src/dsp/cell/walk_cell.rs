//! Pitch walker cell — slow random-walk pitch generator for drone organisms.
//!
//! Outputs a wandering pitch in Hz, constrained by step_size and quantized
//! toward scale degrees by gravity. Designed for DRON's slow harmonic drift.
//!
//! # Parameters
//! - `step_rate`: Walk rate in Hz (0.01-2.0) — how often a new step is taken
//! - `step_size`: Maximum interval in semitones (0.5-12.0)
//! - `gravity`: Quantization pull toward scale degrees (0-1)
//! - `center_hz`: Center frequency the walker orbits (20-2000 Hz)
//! - `range_semitones`: Max distance from center in semitones (1-24)
//!
//! # Output
//! - ch0: pitch (Hz) — the wandering frequency
//! - ch1: gate (always 1.0 — drone is always on)
//!
//! # Wire Graph Pattern
//! - Modulation wire to osc_cell: walk[ch0] → osc.freq (Replace mode)

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for walk_cell.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("step_rate", 0.01, 2.0),
    ("step_size", 0.5, 12.0),
    ("gravity", 0.0, 1.0),
    ("center_hz", 20.0, 2000.0),
    ("range_semitones", 1.0, 24.0),
];

/// Inline xorshift32 PRNG — RT-safe, no allocation.
#[inline]
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Convert xorshift output to [-1.0, 1.0) float.
#[inline]
fn rand_bipolar(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32) * 2.0 - 1.0
}

/// Pitch walker — outputs slowly-wandering pitch for drone/sample organisms.
pub struct WalkCell {
    /// Current position in semitones relative to center.
    position: f32,
    /// Phase accumulator for step timing.
    phase: f32,

    step_rate_handle: Shared,
    step_size_handle: Shared,
    gravity_handle: Shared,
    center_hz_handle: Shared,
    range_semitones_handle: Shared,

    sample_rate: f32,
    rng_state: u32,
    base_values: HashMap<String, f32>,

    /// Scale weights for gravity quantization — received via SetScaleWeights.
    scale_weights: [f32; 12],

    /// If true, ch0 outputs semitones-from-center instead of Hz.
    /// Used by RECH (sample_cell tune is in semitones) vs DRON (osc freq is in Hz).
    semitone_mode: bool,
}

impl WalkCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let step_rate = param_or(dna, "step_rate", 0.1);
        let step_size = param_or(dna, "step_size", 2.0);
        let gravity = param_or(dna, "gravity", 0.5);
        let center_hz = param_or(dna, "center_hz", 110.0);
        let range_semitones = param_or(dna, "range_semitones", 12.0);
        let semitone_mode = string_param_or(dna, "output_mode", "hz") == "semitones";

        let step_rate_handle = shared::shared(step_rate);
        let step_size_handle = shared::shared(step_size);
        let gravity_handle = shared::shared(gravity);
        let center_hz_handle = shared::shared(center_hz);
        let range_semitones_handle = shared::shared(range_semitones);

        // Seed PRNG from center_hz bits
        let rng_state = center_hz.to_bits().wrapping_mul(2654435761) | 1;

        let mut base_values = HashMap::new();
        base_values.insert("step_rate".into(), step_rate);
        base_values.insert("step_size".into(), step_size);
        base_values.insert("gravity".into(), gravity);
        base_values.insert("center_hz".into(), center_hz);
        base_values.insert("range_semitones".into(), range_semitones);

        let handles = vec![
            ("step_rate".into(), step_rate_handle.clone()),
            ("step_size".into(), step_size_handle.clone()),
            ("gravity".into(), gravity_handle.clone()),
            ("center_hz".into(), center_hz_handle.clone()),
            ("range_semitones".into(), range_semitones_handle.clone()),
        ];

        let cell = Self {
            position: 0.0,
            phase: 0.0,
            step_rate_handle,
            step_size_handle,
            gravity_handle,
            center_hz_handle,
            range_semitones_handle,
            sample_rate: sr,
            rng_state,
            base_values,
            scale_weights: [0.0; 12],
            semitone_mode,
        };

        Some((Box::new(cell), handles))
    }

    /// Quantize a semitone position toward the nearest active scale degree.
    /// Returns the quantized semitone position, blended by gravity.
    fn quantize_gravity(&self, semitones: f32, gravity: f32) -> f32 {
        if gravity < 0.01 {
            return semitones;
        }

        // Find nearest active degree within octave
        let octave_pos = ((semitones % 12.0) + 12.0) % 12.0;
        let octave_base = semitones - octave_pos;

        let mut best_degree = octave_pos;
        let mut best_dist = f32::MAX;
        for i in 0..12 {
            if self.scale_weights[i] < 0.1 {
                continue;
            }
            let d = i as f32;
            let dist = (octave_pos - d).abs().min(12.0 - (octave_pos - d).abs());
            let weighted_dist = dist / self.scale_weights[i];
            if weighted_dist < best_dist {
                best_dist = weighted_dist;
                best_degree = d;
            }
        }

        let quantized = octave_base + best_degree;
        semitones * (1.0 - gravity) + quantized * gravity
    }
}

impl DspCell for WalkCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let step_rate = clamp_param(PARAM_RANGES, "step_rate", self.step_rate_handle.value());
        let step_size = clamp_param(PARAM_RANGES, "step_size", self.step_size_handle.value());
        let gravity = clamp_param(PARAM_RANGES, "gravity", self.gravity_handle.value());
        let center_hz = clamp_param(PARAM_RANGES, "center_hz", self.center_hz_handle.value());
        let range = clamp_param(PARAM_RANGES, "range_semitones", self.range_semitones_handle.value());

        // Phase accumulator for step timing
        self.phase += step_rate / self.sample_rate;

        if self.phase >= 1.0 {
            self.phase -= 1.0;

            // Random walk step
            let delta = rand_bipolar(&mut self.rng_state) * step_size;
            self.position += delta;

            // Soft clamp to range (spring force toward center)
            if self.position.abs() > range {
                let overshoot = self.position.abs() - range;
                self.position -= self.position.signum() * overshoot * 0.5;
            }

            // Gravity quantization toward scale degrees
            self.position = self.quantize_gravity(self.position, gravity);
        }

        if self.semitone_mode {
            // Output semitones-from-center directly (for sample_cell tune modulation)
            if output.len() >= 2 {
                output[0] = self.position;
                output[1] = 1.0;
            } else if !output.is_empty() {
                output[0] = self.position;
            }
        } else {
            // Output Hz (for osc_cell freq modulation — default, backward compat)
            let output_hz = center_hz * 2.0f32.powf(self.position / 12.0);
            if output.len() >= 2 {
                output[0] = output_hz.clamp(20.0, 20000.0);
                output[1] = 1.0;
            } else if !output.is_empty() {
                output[0] = output_hz.clamp(20.0, 20000.0);
            }
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            DspCommand::SetScaleWeights(weights, _blend) => {
                self.scale_weights = *weights;
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        2 // ch0=pitch (Hz), ch1=gate (always 1)
    }

    fn reset(&mut self) {
        self.position = 0.0;
        self.phase = 0.0;
    }

    fn name(&self) -> &str {
        "walk_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_walk_dna(center_hz: f32, step_rate: f32, step_size: f32, gravity: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("center_hz".into(), center_hz);
        params.insert("step_rate".into(), step_rate);
        params.insert("step_size".into(), step_size);
        params.insert("gravity".into(), gravity);
        params.insert("range_semitones".into(), 12.0);

        CellDna {
            cell_type: "walk_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn walk_initial_output_is_center() {
        let dna = make_walk_dna(110.0, 0.1, 2.0, 0.0);
        let (mut cell, _) = WalkCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);

        // Initial pitch should be center_hz
        assert!(
            (output[0] - 110.0).abs() < 0.1,
            "initial pitch should be center_hz (110), got {}",
            output[0]
        );
        // Gate should be always on
        assert!((output[1] - 1.0).abs() < 0.01, "gate should be 1.0");
    }

    #[test]
    fn walk_drifts_over_time() {
        let dna = make_walk_dna(110.0, 10.0, 4.0, 0.0); // Fast rate for testing
        let (mut cell, _) = WalkCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut min_pitch = f32::MAX;
        let mut max_pitch = 0.0f32;

        // Run for 2 seconds
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut output);
            min_pitch = min_pitch.min(output[0]);
            max_pitch = max_pitch.max(output[0]);
        }

        // Should have drifted away from center
        let range = max_pitch - min_pitch;
        assert!(
            range > 5.0,
            "walker should drift: range={:.1} Hz ({:.1} - {:.1})",
            range, min_pitch, max_pitch
        );
    }

    #[test]
    fn walk_stays_in_range() {
        let dna = make_walk_dna(440.0, 10.0, 6.0, 0.0);
        let (mut cell, _) = WalkCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        for _ in 0..(SR as usize * 5) {
            cell.tick(&[], &mut output);
            assert!(output[0] >= 20.0, "pitch should not go below 20 Hz");
            assert!(output[0] <= 20000.0, "pitch should not exceed 20 kHz");
        }
    }

    #[test]
    fn walk_gravity_quantizes() {
        let dna = make_walk_dna(261.63, 10.0, 3.0, 1.0); // Strong gravity
        let (mut cell, _) = WalkCell::new(&dna, SR).unwrap();

        // Set C major scale weights
        let mut weights = [0.0f32; 12];
        weights[0] = 1.0; // C
        weights[2] = 1.0; // D
        weights[4] = 1.0; // E
        weights[5] = 1.0; // F
        weights[7] = 1.0; // G
        weights[9] = 1.0; // A
        weights[11] = 1.0; // B
        cell.handle_command(&DspCommand::SetScaleWeights(weights, 1.0));

        let mut output = [0.0f32; 2];

        // Run for a while
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut output);
        }

        // Output should be valid Hz
        assert!(output[0] > 20.0 && output[0] < 20000.0);
    }
}
