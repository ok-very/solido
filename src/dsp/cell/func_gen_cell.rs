//! Multi-minute function generator for glacial modulation curves.
//!
//! Produces long-period control signals (1-600 seconds) for Spiegel-style
//! slowly evolving synthesis. Uses f64 phase accumulation to avoid drift
//! over 10+ minute periods.
//!
//! # Parameters
//! - `period`: Full cycle duration (1-600 seconds)
//! - `depth`: Output amplitude (0-1), bipolar range [-depth, +depth]
//!
//! # String Parameters
//! - `shape`: Waveform (sine, tri, ramp, exp_decay, log_rise, cosine_sum)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "func_gen_cell",
//!   "params": { "period": 120.0, "depth": 0.5 },
//!   "string_params": { "shape": "cosine_sum" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Modulation source: func_gen → filter.cutoff (very slow filter sweeps)
//! - Modulation source: func_gen → osc.freq (glacial pitch drift)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};

/// Valid parameter ranges for func_gen_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("period", 0.1, 1800.0),
    ("depth", 0.0, 1.0),
];
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Function generator waveform shapes.
#[derive(Clone, Copy, Debug)]
enum FuncGenShape {
    Sine,
    Triangle,
    Ramp,
    ExpDecay,
    LogRise,
    CosineSum,
}

impl FuncGenShape {
    /// Parse shape from string param.
    fn from_str(s: &str) -> Self {
        match s {
            "tri" | "triangle" => FuncGenShape::Triangle,
            "ramp" => FuncGenShape::Ramp,
            "exp_decay" => FuncGenShape::ExpDecay,
            "log_rise" => FuncGenShape::LogRise,
            "cosine_sum" => FuncGenShape::CosineSum,
            _ => FuncGenShape::Sine, // Default
        }
    }

    /// Compute waveform output for given phase [0, 1) and depth.
    ///
    /// Returns value in range [-depth, +depth].
    #[inline]
    fn compute(&self, phase: f32, depth: f32) -> f32 {
        match self {
            FuncGenShape::Sine => {
                // sin(phase * 2π) scaled by depth
                depth * (phase * std::f32::consts::TAU).sin()
            }
            FuncGenShape::Triangle => {
                // Triangle: -1 at phase=0, +1 at phase=0.5, -1 at phase=1.0
                // Formula: 4 * |phase - 0.5| - 1
                depth * (4.0 * (phase - 0.5).abs() - 1.0)
            }
            FuncGenShape::Ramp => {
                // Ramp: linear sweep from -depth to +depth over [0, 1)
                depth * (2.0 * phase - 1.0)
            }
            FuncGenShape::ExpDecay => {
                // Exponential decay: e^(-3×phase) normalized to bipolar
                // e^0 = 1, e^(-3) ≈ 0.05 → scale and offset to [-1, 1]
                let decay = (-3.0 * phase).exp();
                // Normalize: decay goes 1→0.05, map to 1→-1
                depth * (2.0 * decay - 1.0)
            }
            FuncGenShape::LogRise => {
                // Logarithmic rise: 1 - e^(-3×phase) normalized to bipolar
                // Inverse of exp_decay
                let rise = 1.0 - (-3.0 * phase).exp();
                // rise goes 0→0.95, map to -1→1
                depth * (2.0 * rise - 1.0)
            }
            FuncGenShape::CosineSum => {
                // Golden ratio multi-frequency: three cosines at incommensurate periods
                // Creates patterns that never exactly repeat
                const PHI: f32 = 1.618; // Golden ratio
                const SQRT2_PLUS_1: f32 = 2.414;

                let tau_phase = phase * std::f32::consts::TAU;
                depth * (
                    0.5 * (tau_phase).sin()
                    + 0.3 * (tau_phase / PHI).sin()
                    + 0.2 * (tau_phase / SQRT2_PLUS_1).sin()
                )
            }
        }
    }
}

/// Multi-minute function generator producing glacial control signals.
///
/// Uses f64 phase accumulation for long-period precision (1-600 seconds).
/// Output is mono bipolar control signal [-depth, +depth].
///
/// # Parameters
/// - `period`: Full cycle duration (1-600 seconds)
/// - `depth`: Output amplitude (0-1)
///
/// # String Parameters
/// - `shape`: Waveform (sine, tri, ramp, exp_decay, log_rise, cosine_sum)
pub struct FuncGenCell {
    phase_seconds: f64,  // Absolute time in seconds (f64 for long-period precision)
    period_handle: Shared,
    depth_handle: Shared,
    shape: FuncGenShape,
    base_values: HashMap<String, f32>,
    sample_rate: f32,
}

impl FuncGenCell {
    /// Build a func_gen_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let period = param_or(dna, "period", 120.0);
        let depth = param_or(dna, "depth", 0.5);

        // Read waveform shape string param
        let shape_str = string_param_or(dna, "shape", "sine");
        let shape = FuncGenShape::from_str(shape_str);

        // Our Shared handles for the control thread
        let period_handle = shared::shared(period);
        let depth_handle = shared::shared(depth);

        let handles = vec![
            ("period".into(), period_handle.clone()),
            ("depth".into(), depth_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let base_values = [
            ("period", period),
            ("depth", depth),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        let cell = Self {
            phase_seconds: 0.0,
            period_handle,
            depth_handle,
            shape,
            base_values,
            sample_rate: sr,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for FuncGenCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Read params
        let period = self.period_handle.value().clamp(1.0, 600.0);
        let depth = self.depth_handle.value().clamp(0.0, 1.0);

        // Compute normalized phase [0, 1) from absolute time
        let phase = ((self.phase_seconds / period as f64) % 1.0) as f32;

        // Compute waveform output
        let value = self.shape.compute(phase, depth);

        // Advance absolute time (f64 precision for long periods)
        self.phase_seconds += 1.0 / self.sample_rate as f64;

        // Output mono (bipolar control signal)
        output[0] = value;
        // func_gen is mono, but replicate to stereo if needed for consistency
        if output.len() > 1 {
            output[1] = value;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {} // NoteOn/NoteOff not relevant for function generators
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // func_gen is a control signal, not audio
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize { 1 } // Control signal (not audio)

    fn reset(&mut self) {
        self.phase_seconds = 0.0;
    }

    fn name(&self) -> &str { "func_gen_cell" }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna(period: f32, depth: f32, shape: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("period".into(), period);
        params.insert("depth".into(), depth);

        let mut string_params = BTreeMap::new();
        string_params.insert("shape".into(), shape.into());

        CellDna {
            cell_type: "func_gen_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn func_gen_sine_period() {
        // 10s period should complete one cycle in ~441,000 samples
        let dna = make_dna(10.0, 0.5, "sine");
        let (mut cell, _handles) = FuncGenCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let samples_per_period = (10.0 * SR) as usize;

        // Sample at start (phase=0)
        cell.tick(&[], &mut output);
        let start_value = output[0];

        // Sample at quarter period (phase=0.25)
        for _ in 0..(samples_per_period / 4) {
            cell.tick(&[], &mut output);
        }
        let quarter_value = output[0];

        // Sample at half period (phase=0.5)
        for _ in 0..(samples_per_period / 4) {
            cell.tick(&[], &mut output);
        }
        let half_value = output[0];

        // Sine wave characteristics:
        // phase=0 → sin(0) = 0
        // phase=0.25 → sin(π/2) = 1 → scaled = 0.5
        // phase=0.5 → sin(π) = 0
        assert!(start_value.abs() < 0.01, "Start should be near zero, got {}", start_value);
        assert!((quarter_value - 0.5).abs() < 0.05, "Quarter period should be near 0.5, got {}", quarter_value);
        assert!(half_value.abs() < 0.01, "Half period should be near zero, got {}", half_value);
    }

    #[test]
    fn func_gen_depth_scales() {
        // depth=0 → silent, depth=1 → full ±1.0 range (for sine wave)
        let dna_zero = make_dna(10.0, 0.0, "sine");
        let dna_full = make_dna(10.0, 1.0, "sine");

        let (mut cell_zero, _) = FuncGenCell::new(&dna_zero, SR).unwrap();
        let (mut cell_full, _) = FuncGenCell::new(&dna_full, SR).unwrap();

        let mut output_zero = [0.0f32; 2];
        let mut output_full = [0.0f32; 2];

        // Collect samples over full quarter period to reach peak
        let quarter_period_samples = (2.5 * SR) as usize; // 2.5s = quarter of 10s period
        let mut max_zero = 0.0f32;
        let mut max_full = 0.0f32;

        for _ in 0..quarter_period_samples {
            cell_zero.tick(&[], &mut output_zero);
            cell_full.tick(&[], &mut output_full);
            max_zero = max_zero.max(output_zero[0].abs());
            max_full = max_full.max(output_full[0].abs());
        }

        assert!(max_zero < 0.01, "depth=0 should be silent, got max={}", max_zero);
        assert!(max_full > 0.95, "depth=1 should reach near 1.0, got max={}", max_full);
    }

    #[test]
    fn func_gen_cosine_sum_complex() {
        // cosine_sum should produce different waveform from plain sine
        // Test at a specific phase where we know they differ
        let dna_sine = make_dna(10.0, 0.5, "sine");
        let dna_cosine_sum = make_dna(10.0, 0.5, "cosine_sum");

        let (mut cell_sine, _) = FuncGenCell::new(&dna_sine, SR).unwrap();
        let (mut cell_cosine_sum, _) = FuncGenCell::new(&dna_cosine_sum, SR).unwrap();

        let mut output_sine = [0.0f32; 2];
        let mut output_cosine_sum = [0.0f32; 2];

        // Advance to quarter period (phase = 0.25)
        let quarter_samples = (2.5 * SR) as usize;
        for _ in 0..quarter_samples {
            cell_sine.tick(&[], &mut output_sine);
            cell_cosine_sum.tick(&[], &mut output_cosine_sum);
        }

        // At quarter period: sine should be at peak (0.5)
        // cosine_sum should be different due to multi-frequency components
        let sine_value = output_sine[0];
        let cosine_sum_value = output_cosine_sum[0];

        // Check that sine is near expected peak
        assert!(
            (sine_value - 0.5).abs() < 0.01,
            "Sine should be near 0.5 at quarter period, got {}",
            sine_value
        );

        // Check that cosine_sum is noticeably different
        assert!(
            (sine_value - cosine_sum_value).abs() > 0.05,
            "cosine_sum should differ from sine by >0.05, got sine={} cosine_sum={} diff={}",
            sine_value,
            cosine_sum_value,
            (sine_value - cosine_sum_value).abs()
        );
    }

    #[test]
    fn func_gen_ramp_monotonic() {
        // Ramp shape should increase monotonically over full period
        // Ramp goes from -depth to +depth, so midpoint is 0
        let dna = make_dna(10.0, 0.5, "ramp");
        let (mut cell, _) = FuncGenCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Sample at start (phase=0)
        cell.tick(&[], &mut output);
        let start_value = output[0];

        // Check monotonic increase over full period
        let full_period_samples = (10.0 * SR) as usize;
        let mut last_value = start_value;

        for _ in 1..full_period_samples {
            cell.tick(&[], &mut output);
            assert!(
                output[0] >= last_value - 0.001, // Allow tiny floating point error
                "Ramp should increase monotonically, got {} after {}",
                output[0], last_value
            );
            last_value = output[0];
        }

        // Start should be near -depth (-0.5)
        assert!(
            (start_value - (-0.5)).abs() < 0.01,
            "Ramp should start near -0.5, got {}",
            start_value
        );

        // End should be near +depth (0.5)
        assert!(
            (last_value - 0.5).abs() < 0.01,
            "Ramp should end near 0.5, got {}",
            last_value
        );
    }

    #[test]
    fn func_gen_long_period() {
        // 600s period should advance linearly without drift
        let dna = make_dna(600.0, 1.0, "ramp");
        let (mut cell, _) = FuncGenCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Sample at start
        cell.tick(&[], &mut output);
        let start_value = output[0];

        // Advance 60 seconds (1/10 of period)
        let sixty_seconds = (60.0 * SR) as usize;
        for _ in 0..sixty_seconds {
            cell.tick(&[], &mut output);
        }
        let sixty_sec_value = output[0];

        // Ramp over 600s: starts at -1, ends at +1
        // At 60s (1/10 period): should be -1 + (2 * 0.1) = -0.8
        let expected = -1.0 + (2.0 * (60.0 / 600.0));
        assert!(
            (sixty_sec_value - expected).abs() < 0.01,
            "After 60s of 600s period, expected {}, got {}",
            expected,
            sixty_sec_value
        );

        // Verify start was near -1
        assert!(
            (start_value - (-1.0)).abs() < 0.01,
            "Start should be near -1, got {}",
            start_value
        );
    }
}
