//! Terminal stereo mixer with gain and equal-power panning.
//!
//! Processes stereo audio input from upstream cells and applies gain scaling with
//! optional stereo panning. Uses equal-power pan law to maintain constant perceived
//! loudness across the stereo field.
//!
//! This is a terminal cell (no outgoing audio wires) that typically feeds directly
//! to the organism's final stereo output.
//!
//! # Parameters
//! - `gain`: Output level (0-1)
//! - `pan`: Stereo position (-1.0 = full left, 0.0 = center, 1.0 = full right)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "mixer_cell",
//!   "params": { "gain": 0.8, "pan": 0.0 }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Terminal cell: filter_cell → mixer_cell (no outgoing wires)
//! - Multi-source mix: osc_cell[0] + osc_cell[1] → mixer_cell (audio wires)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};

/// Valid parameter ranges for mixer_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("gain", 0.0, 2.0),
    ("pan", -1.0, 1.0),
];
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Terminal stereo mixer with gain and equal-power panning.
///
/// Processes stereo input with gain scaling and optional pan positioning.
/// Uses equal-power pan law: left_gain = sqrt((1-pan)/2) × gain
///
/// # Parameters
/// - `gain`: Output level (0-1)
/// - `pan`: Stereo position (-1.0 = full left, 0.0 = center, 1.0 = full right)
pub struct MixerCell {
    gain_handle: Shared,
    pan_handle: Shared,
    base_values: HashMap<String, f32>,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl MixerCell {
    /// Build a mixer_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, _sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let gain = param_or(dna, "gain", 0.8);
        let pan = param_or(dna, "pan", 0.0);

        // Our Shared handles for the control thread
        let gain_handle = shared::shared(gain);
        let pan_handle = shared::shared(pan);

        let handles = vec![
            ("gain".into(), gain_handle.clone()),
            ("pan".into(), pan_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let base_values = [
            ("gain", gain),
            ("pan", pan),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        let cell = Self {
            gain_handle,
            pan_handle,
            base_values,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for MixerCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        // Read input (stereo or mono)
        let input_l = if !input.is_empty() { input[0] } else { 0.0 };
        let input_r = if input.len() > 1 { input[1] } else { input_l };

        // Read params
        let gain = self.gain_handle.value().clamp(0.0, 1.0);
        let pan = self.pan_handle.value().clamp(-1.0, 1.0);

        // Equal-power pan law
        // For pan ∈ [-1, 1]:
        //   left_gain  = sqrt((1 - pan) / 2) × gain
        //   right_gain = sqrt((1 + pan) / 2) × gain
        // This maintains constant perceived power across the stereo field.
        let left_gain = ((1.0 - pan) / 2.0).sqrt() * gain;
        let right_gain = ((1.0 + pan) / 2.0).sqrt() * gain;

        let left = input_l * left_gain;
        let right = input_r * right_gain;

        // Analysis accumulation
        let sample_avg = (left + right) * 0.5;
        self.rms_acc += sample_avg * sample_avg;
        self.peak = self.peak.max(left.abs()).max(right.abs());
        self.sample_count += 1;

        // Output stereo
        output[0] = left;
        if output.len() > 1 {
            output[1] = right;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {} // NoteOn/NoteOff not relevant for mixers
        }
    }

    fn analysis(&self) -> DspAnalysis {
        if self.sample_count > 0 {
            DspAnalysis::new(
                (self.rms_acc / self.sample_count as f32).sqrt(),
                self.peak,
            )
        } else {
            DspAnalysis::new(0.0, 0.0)
        }
    }

    fn output_channels(&self) -> usize { 2 }

    fn reset(&mut self) {
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "mixer_cell" }

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

    fn make_dna(gain: f32, pan: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("gain".into(), gain);
        params.insert("pan".into(), pan);

        CellDna {
            cell_type: "mixer_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    /// Generate a simple test signal (sine wave)
    fn generate_test_signal(samples: usize, freq: f32) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                let phase = (i as f32 / SR) * freq * std::f32::consts::TAU;
                phase.sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn mixer_passes_audio() {
        let dna = make_dna(0.8, 0.0);
        let (mut cell, _handles) = MixerCell::new(&dna, SR).unwrap();

        let input_signal = generate_test_signal(1000, 440.0);
        let mut output = [0.0f32; 2];

        let mut any_nonzero = false;
        for sample in input_signal.iter() {
            cell.tick(&[*sample, *sample], &mut output);
            if output[0].abs() > 0.001 || output[1].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }

        assert!(any_nonzero, "Expected mixer to pass audio signal");
    }

    #[test]
    fn mixer_gain_scales() {
        // Test gain=0 produces silence
        let dna_zero = make_dna(0.0, 0.0);
        let (mut cell_zero, _) = MixerCell::new(&dna_zero, SR).unwrap();

        // Test gain=0.5 produces half amplitude
        let dna_half = make_dna(0.5, 0.0);
        let (mut cell_half, _) = MixerCell::new(&dna_half, SR).unwrap();

        // Test gain=1.0 produces full amplitude
        let dna_full = make_dna(1.0, 0.0);
        let (mut cell_full, _) = MixerCell::new(&dna_full, SR).unwrap();

        let input_signal = generate_test_signal(1000, 440.0);
        let mut output = [0.0f32; 2];

        // Measure peak over 1000 samples
        let mut peak_zero = 0.0f32;
        let mut peak_half = 0.0f32;
        let mut peak_full = 0.0f32;

        for sample in input_signal.iter() {
            cell_zero.tick(&[*sample, *sample], &mut output);
            peak_zero = peak_zero.max(output[0].abs());

            cell_half.tick(&[*sample, *sample], &mut output);
            peak_half = peak_half.max(output[0].abs());

            cell_full.tick(&[*sample, *sample], &mut output);
            peak_full = peak_full.max(output[0].abs());
        }

        // gain=0 should be effectively silent
        assert!(peak_zero < 0.001, "Expected gain=0 to be silent, got peak {}", peak_zero);

        // gain=1.0 should be roughly 2× gain=0.5
        assert!(
            peak_full > peak_half * 1.8,
            "Expected gain=1.0 to produce ~2× output of gain=0.5, got {} vs {}",
            peak_full,
            peak_half
        );
    }

    #[test]
    fn mixer_pan_left() {
        // pan=-1.0 (full left) should attenuate right channel significantly
        let dna = make_dna(1.0, -1.0);
        let (mut cell, _handles) = MixerCell::new(&dna, SR).unwrap();

        let input_signal = generate_test_signal(1000, 440.0);
        let mut output = [0.0f32; 2];

        let mut peak_left = 0.0f32;
        let mut peak_right = 0.0f32;

        for sample in input_signal.iter() {
            cell.tick(&[*sample, *sample], &mut output);
            peak_left = peak_left.max(output[0].abs());
            peak_right = peak_right.max(output[1].abs());
        }

        // Right channel should be at least 20dB below left (-20dB = 0.1× amplitude)
        assert!(
            peak_right < peak_left * 0.15,
            "Expected pan=-1.0 to heavily attenuate right channel, got L={}, R={}",
            peak_left,
            peak_right
        );
    }

    #[test]
    fn mixer_pan_right() {
        // pan=1.0 (full right) should attenuate left channel significantly
        let dna = make_dna(1.0, 1.0);
        let (mut cell, _handles) = MixerCell::new(&dna, SR).unwrap();

        let input_signal = generate_test_signal(1000, 440.0);
        let mut output = [0.0f32; 2];

        let mut peak_left = 0.0f32;
        let mut peak_right = 0.0f32;

        for sample in input_signal.iter() {
            cell.tick(&[*sample, *sample], &mut output);
            peak_left = peak_left.max(output[0].abs());
            peak_right = peak_right.max(output[1].abs());
        }

        // Left channel should be at least 20dB below right
        assert!(
            peak_left < peak_right * 0.15,
            "Expected pan=1.0 to heavily attenuate left channel, got L={}, R={}",
            peak_left,
            peak_right
        );
    }

    #[test]
    fn mixer_pan_center() {
        // pan=0.0 (center) should produce equal L/R amplitude
        let dna = make_dna(1.0, 0.0);
        let (mut cell, _handles) = MixerCell::new(&dna, SR).unwrap();

        let input_signal = generate_test_signal(1000, 440.0);
        let mut output = [0.0f32; 2];

        let mut peak_left = 0.0f32;
        let mut peak_right = 0.0f32;

        for sample in input_signal.iter() {
            cell.tick(&[*sample, *sample], &mut output);
            peak_left = peak_left.max(output[0].abs());
            peak_right = peak_right.max(output[1].abs());
        }

        // Left and right peaks should be within 5% of each other
        let diff_ratio = (peak_left - peak_right).abs() / peak_left.max(peak_right);
        assert!(
            diff_ratio < 0.05,
            "Expected pan=0.0 to produce equal L/R, got L={}, R={} (diff ratio: {})",
            peak_left,
            peak_right,
            diff_ratio
        );
    }
}
