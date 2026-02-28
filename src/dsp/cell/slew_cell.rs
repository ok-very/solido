//! Slew rate limiter for portamento/glide effects.
//!
//! Smooths step changes in control signals with asymmetric rise/fall times.
//! Essential for HOSO's microtonal pitch slides and DRON's slow pitch drift.
//!
//! # Parameters
//! - `rise`: Rise time in seconds (0.001-2.0) — slew rate for increasing values
//! - `fall`: Fall time in seconds (0.001-2.0) — slew rate for decreasing values
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "slew_cell",
//!   "params": { "rise": 0.05, "fall": 0.05 }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Audio input from seq_cell: seq[ch1=pitch] → slew (pitch smoothing)
//! - Modulation output to osc_cell: slew → osc.freq (apply smoothed pitch)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Portamento/glide — smooths pitch transitions with asymmetric slew rates.
///
/// # Parameters
/// - `rise`: Rise time (seconds)
/// - `fall`: Fall time (seconds)
///
/// # Input
/// - Mono or stereo control signal (if stereo, reads ch1 for pitch from seq_cell)
///
/// # Output
/// - Mono smoothed value
pub struct SlewCell {
    rise_handle: Shared,
    fall_handle: Shared,

    current_value: f32,
    sample_rate: f32,
    base_values: HashMap<String, f32>,
}

impl SlewCell {
    /// Build a slew_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell + shared param handles
    /// - `None`: Should never fail (all params have defaults)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let rise = param_or(dna, "rise", 0.05);
        let fall = param_or(dna, "fall", 0.05);

        let rise_handle = shared::shared(rise);
        let fall_handle = shared::shared(fall);

        let mut base_values = HashMap::new();
        base_values.insert("rise".into(), rise);
        base_values.insert("fall".into(), fall);

        let cell = Self {
            rise_handle: rise_handle.clone(),
            fall_handle: fall_handle.clone(),
            current_value: 0.0,
            sample_rate: sr,
            base_values,
        };

        let handles = vec![("rise".into(), rise_handle), ("fall".into(), fall_handle)];

        Some((Box::new(cell), handles))
    }
}

impl DspCell for SlewCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        // Read target value from input
        // If stereo input (from seq_cell), read ch1 (pitch), else ch0
        let target = if input.is_empty() {
            0.0
        } else if input.len() > 1 {
            input[1] // Read ch1 (pitch) if stereo input from seq_cell
        } else {
            input[0] // Mono input
        };

        let diff = target - self.current_value;

        if diff.abs() < 0.001 {
            // Close enough, snap to target (prevents infinite convergence)
            self.current_value = target;
        } else if diff > 0.0 {
            // Rising: use rise time
            let rise_time = self.rise_handle.value().max(0.001);
            // Maximum change per sample: full range / (time * sample_rate)
            // But we want to limit rate proportional to current diff
            let max_rate_per_sample = 1.0 / (rise_time * self.sample_rate);
            let delta = diff * max_rate_per_sample;
            self.current_value += delta;
        } else {
            // Falling: use fall time
            let fall_time = self.fall_handle.value().max(0.001);
            let max_rate_per_sample = 1.0 / (fall_time * self.sample_rate);
            let delta = diff * max_rate_per_sample;
            self.current_value += delta;
        }

        // Mono output
        if !output.is_empty() {
            output[0] = self.current_value;
        }
    }

    fn handle_command(&mut self, _cmd: &DspCommand) {
        // Slew is purely signal-driven, ignores commands
    }

    fn analysis(&self) -> DspAnalysis {
        // Slew is a control signal, not audio
        DspAnalysis {
            rms: 0.0,
            peak: 0.0,
        }
    }

    fn output_channels(&self) -> usize {
        1 // Mono control signal (not audio)
    }

    fn reset(&mut self) {
        self.current_value = 0.0;
    }

    fn name(&self) -> &str {
        "slew_cell"
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_test_dna(rise: f32, fall: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("rise".into(), rise);
        params.insert("fall".into(), fall);

        CellDna {
            cell_type: "slew_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn slew_smooths_step() {
        // Step input → gradual output ramp
        let dna = make_test_dna(0.1, 0.1); // 0.1s slew time
        let (mut cell, _) = SlewCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];

        // Start at 0, slew to 100
        let target = [100.0f32; 1];

        // First sample should start moving toward target
        cell.tick(&target, &mut output);
        let first_value = output[0];
        assert!(
            first_value > 0.0 && first_value < 100.0,
            "Should start slewing, got {}",
            first_value
        );

        // After many samples, should approach target
        for _ in 0..10000 {
            cell.tick(&target, &mut output);
        }

        assert!(
            output[0] > 80.0,
            "Should approach target after slew time, got {}",
            output[0]
        );
    }

    #[test]
    fn slew_rise_fall_asymmetric() {
        // Different rise/fall times produce different curves
        let dna = make_test_dna(0.01, 0.1); // Fast rise, slow fall
        let (mut cell, _) = SlewCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let target_high = [100.0f32; 1];
        let target_low = [0.0f32; 1];

        // Rising phase (fast)
        for _ in 0..500 {
            cell.tick(&target_high, &mut output);
        }
        let rise_value = output[0];

        // Should rise quickly
        assert!(rise_value > 50.0, "Fast rise should reach ~halfway in 500 samples");

        // Wait for full rise
        for _ in 0..1000 {
            cell.tick(&target_high, &mut output);
        }

        // Now fall (slow)
        cell.tick(&target_low, &mut output); // Start falling

        for _ in 0..500 {
            cell.tick(&target_low, &mut output);
        }
        let fall_value = output[0];

        // Should fall slower than it rose
        assert!(
            fall_value > 50.0,
            "Slow fall should be higher after same sample count, got {}",
            fall_value
        );
    }

    #[test]
    fn slew_tracks_constant() {
        // Constant input → output converges
        let dna = make_test_dna(0.05, 0.05);
        let (mut cell, _) = SlewCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let target = [50.0f32; 1];

        // Slew for 3x the slew time (should converge)
        let samples = (0.05 * 3.0 * 44100.0) as usize;
        for _ in 0..samples {
            cell.tick(&target, &mut output);
        }

        assert!(
            (output[0] - 50.0).abs() < 10.0,
            "Should converge toward target within 3x slew time, got {}",
            output[0]
        );
    }
}
