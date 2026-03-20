//! Video feature receiver cell — smooths video CV signals at sample rate.
//!
//! Built-in sensory organ: any organism with this cell in its DNA automatically
//! receives video features via `SetVideoFeatures` command broadcast. No affinity
//! graph routing needed — if you have the cell, you see.
//!
//! Applies one-pole lowpass smoothing to eliminate 30Hz stepping, outputs 2
//! channels with DNA-configurable feature assignment via `ch0`/`ch1` string params.
//!
//! # Parameters
//! - `slew`: Smoothing time in seconds [0.005, 1.0]
//! - `gain`: Output scaling [0, 2.0]
//!
//! # String Parameters
//! - `ch0`: Feature routed to output channel 0 (default: "brightness")
//! - `ch1`: Feature routed to output channel 1 (default: "motion")
//!
//! Valid feature names: "brightness", "warmth", "motion", "edge"
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "video_cv_cell",
//!   "params": { "slew": 0.05, "gain": 1.0 },
//!   "string_params": { "ch0": "brightness", "ch1": "motion" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Modulation source: video_cv_cell ch1 (motion) → seq_cell.chaos
//! - Modulation source: video_cv_cell ch0 (brightness) → filter_cell.cutoff

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for video_cv_cell.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("slew", 0.005, 1.0),
    ("gain", 0.0, 2.0),
];

/// Number of smoothed features (all 4 are smoothed internally).
const NUM_FEATURES: usize = 4;

/// Map feature name string to index into smooth[] array.
fn feature_index(name: &str) -> usize {
    match name {
        "brightness" => 0,
        "warmth" => 1,
        "motion" => 2,
        "edge" => 3,
        _ => 0,
    }
}

/// Video feature receiver with sample-rate smoothing.
///
/// Receives brightness, warmth, motion_energy, and edge_density via
/// `SetVideoFeatures` command (broadcast from app). Smooths all 4 internally,
/// outputs a configurable pair on 2 channels for modulation wiring.
pub struct VideoCvCell {
    // Current feature targets (set by SetVideoFeatures command)
    targets: [f32; NUM_FEATURES],

    // Smoothed state (one-pole LP per feature)
    smooth: [f32; NUM_FEATURES],

    // Config params (externally modulatable via Shared)
    slew_handle: Shared,
    gain_handle: Shared,

    // Output channel mapping: indices into smooth[]
    ch0_feature: usize,
    ch1_feature: usize,

    sample_rate: f32,
    initialized: bool,
    base_values: HashMap<String, f32>,
}

impl VideoCvCell {
    /// Build a video_cv_cell from DNA.
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let slew = param_or(dna, "slew", 0.05);
        let gain = param_or(dna, "gain", 1.0);

        // Output channel feature assignment
        let ch0_name = string_param_or(dna, "ch0", "brightness");
        let ch1_name = string_param_or(dna, "ch1", "motion");
        let ch0_feature = feature_index(ch0_name);
        let ch1_feature = feature_index(ch1_name);

        let slew_handle = shared::shared(slew);
        let gain_handle = shared::shared(gain);

        let handles = vec![
            ("slew".into(), slew_handle.clone()),
            ("gain".into(), gain_handle.clone()),
        ];

        let base_values = [("slew", slew), ("gain", gain)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();

        let cell = Self {
            targets: [0.0; NUM_FEATURES],
            smooth: [0.0; NUM_FEATURES],
            slew_handle,
            gain_handle,
            ch0_feature,
            ch1_feature,
            sample_rate: sr,
            initialized: false,
            base_values,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for VideoCvCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        if !self.initialized {
            // Latch to current targets on first tick (no ramp from 0)
            self.smooth = self.targets;
            self.initialized = true;
        }

        // One-pole lowpass: coeff = e^(-1 / (slew * sr))
        let slew_time = self.slew_handle.value().clamp(0.005, 1.0);
        let coeff = (-1.0 / (slew_time * self.sample_rate)).exp();
        let one_minus_coeff = 1.0 - coeff;

        let gain = self.gain_handle.value().clamp(0.0, 2.0);

        // Smooth all 4 features
        for i in 0..NUM_FEATURES {
            self.smooth[i] = self.smooth[i] * coeff + self.targets[i] * one_minus_coeff;
        }

        // Output the configured pair
        if !output.is_empty() {
            output[0] = self.smooth[self.ch0_feature] * gain;
        }
        if output.len() > 1 {
            output[1] = self.smooth[self.ch1_feature] * gain;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::SetVideoFeatures {
                brightness,
                warmth,
                motion,
                edge,
            } => {
                self.targets[0] = *brightness;
                self.targets[1] = *warmth;
                self.targets[2] = *motion;
                self.targets[3] = *edge;
            }
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.targets = [0.0; NUM_FEATURES];
        self.smooth = [0.0; NUM_FEATURES];
        self.initialized = false;
    }

    fn name(&self) -> &str {
        "video_cv_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] {
        PARAM_RANGES
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 48000.0;

    fn make_dna(slew: f32, gain: f32, ch0: &str, ch1: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("slew".into(), slew);
        params.insert("gain".into(), gain);

        let mut string_params = BTreeMap::new();
        string_params.insert("ch0".into(), ch0.into());
        string_params.insert("ch1".into(), ch1.into());

        CellDna {
            cell_type: "video_cv_cell".into(),
            params,
            string_params,
        }
    }

    fn video_cmd(brightness: f32, warmth: f32, motion: f32, edge: f32) -> DspCommand {
        DspCommand::SetVideoFeatures {
            brightness,
            warmth,
            motion,
            edge,
        }
    }

    #[test]
    fn first_sample_latches_to_target() {
        let dna = make_dna(0.05, 1.0, "brightness", "motion");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        // Set features before first tick
        cell.handle_command(&video_cmd(0.8, 0.0, 0.5, 0.0));

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);

        // ch0=brightness should latch to 0.8
        assert!(
            (output[0] - 0.8).abs() < 0.01,
            "First tick should latch brightness, got {}",
            output[0]
        );
        // ch1=motion should latch to 0.5
        assert!(
            (output[1] - 0.5).abs() < 0.01,
            "First tick should latch motion, got {}",
            output[1]
        );
    }

    #[test]
    fn smoothing_convergence() {
        let slew = 0.05;
        let dna = make_dna(slew, 1.0, "brightness", "motion");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        // Initialize at 0
        cell.tick(&[], &mut output);

        // Step brightness to 1.0
        cell.handle_command(&video_cmd(1.0, 0.0, 0.0, 0.0));

        let samples = (slew * 3.0 * SR) as usize;
        for _ in 0..samples {
            cell.tick(&[], &mut output);
        }

        assert!(
            output[0] > 0.9,
            "Should reach >90% after 3x slew time, got {}",
            output[0]
        );
    }

    #[test]
    fn gain_scales_output() {
        let dna = make_dna(0.005, 1.5, "brightness", "motion");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        cell.handle_command(&video_cmd(0.0, 0.0, 0.6, 0.0));

        let mut output = [0.0f32; 2];
        for _ in 0..2000 {
            cell.tick(&[], &mut output);
        }

        // ch1=motion, 0.6 * 1.5 = 0.9
        assert!(
            (output[1] - 0.9).abs() < 0.05,
            "Motion * gain should be ~0.9, got {}",
            output[1]
        );
    }

    #[test]
    fn channel_mapping_configurable() {
        let dna = make_dna(0.005, 1.0, "warmth", "edge");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        cell.handle_command(&video_cmd(0.0, -0.5, 0.0, 0.7));

        let mut output = [0.0f32; 2];
        for _ in 0..2000 {
            cell.tick(&[], &mut output);
        }

        assert!(
            (output[0] - (-0.5)).abs() < 0.05,
            "ch0 should be warmth (-0.5), got {}",
            output[0]
        );
        assert!(
            (output[1] - 0.7).abs() < 0.05,
            "ch1 should be edge (0.7), got {}",
            output[1]
        );
    }

    #[test]
    fn reset_clears_state() {
        let dna = make_dna(0.05, 1.0, "brightness", "motion");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        cell.handle_command(&video_cmd(1.0, 0.0, 0.0, 0.0));

        let mut output = [0.0f32; 2];
        for _ in 0..5000 {
            cell.tick(&[], &mut output);
        }
        assert!(output[0] > 0.5, "Should have converged before reset");

        cell.reset();
        cell.handle_command(&video_cmd(0.3, 0.0, 0.0, 0.0));
        cell.tick(&[], &mut output);
        assert!(
            (output[0] - 0.3).abs() < 0.01,
            "After reset, should re-latch, got {}",
            output[0]
        );
    }

    #[test]
    fn no_video_outputs_zero() {
        let dna = make_dna(0.05, 1.0, "brightness", "motion");
        let (mut cell, _) = VideoCvCell::new(&dna, SR).unwrap();

        // No SetVideoFeatures sent — targets stay at 0
        let mut output = [0.0f32; 2];
        for _ in 0..1000 {
            cell.tick(&[], &mut output);
        }

        assert!(
            output[0].abs() < 0.001 && output[1].abs() < 0.001,
            "No video = zero output, got [{}, {}]",
            output[0],
            output[1]
        );
    }
}
