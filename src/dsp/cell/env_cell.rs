//! ADSR envelope generator triggered by gate signal.
//!
//! Detects gate rising edge (0→1) to trigger attack, and falling edge (1→0)
//! to trigger release. Classic analog-style envelope with 5 stages:
//! Idle → Attack → Decay → Sustain → Release → Idle.
//!
//! # Parameters
//! - `attack`: Attack time in seconds (0.001-2.0)
//! - `decay`: Decay time in seconds (0.001-2.0)
//! - `sustain`: Sustain level 0.0-1.0
//! - `release`: Release time in seconds (0.001-5.0)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "env_cell",
//!   "params": { "attack": 0.01, "decay": 0.1, "sustain": 0.7, "release": 0.3 }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Audio input from seq_cell: seq[ch0=gate] → env (reads gate signal)
//! - Modulation output to osc_cell: env → osc.gain (VCA envelope)
//! - Modulation output to filter_cell: env → filter.cutoff (VCF envelope)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// ADSR envelope stage.
#[derive(Clone, Copy, PartialEq, Debug)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR envelope generator with gate-driven triggering.
///
/// # Parameters
/// - `attack`, `decay`, `sustain`, `release`: ADSR envelope shape
///
/// # Input
/// - Mono gate signal (0.0 or 1.0) from seq_cell or similar
///
/// # Output
/// - Mono envelope value (0.0-1.0)
pub struct EnvCell {
    attack_handle: Shared,
    decay_handle: Shared,
    sustain_handle: Shared,
    release_handle: Shared,

    stage: EnvStage,
    value: f32,     // Current envelope value 0.0-1.0
    gate_prev: f32, // Previous gate input (for edge detection)

    sample_rate: f32,
    base_values: HashMap<String, f32>,
}

impl EnvCell {
    /// Build an env_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell + shared param handles
    /// - `None`: Should never fail (all params have defaults)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let attack = param_or(dna, "attack", 0.01);
        let decay = param_or(dna, "decay", 0.1);
        let sustain = param_or(dna, "sustain", 0.7);
        let release = param_or(dna, "release", 0.3);

        let attack_handle = shared::shared(attack);
        let decay_handle = shared::shared(decay);
        let sustain_handle = shared::shared(sustain);
        let release_handle = shared::shared(release);

        let mut base_values = HashMap::new();
        base_values.insert("attack".into(), attack);
        base_values.insert("decay".into(), decay);
        base_values.insert("sustain".into(), sustain);
        base_values.insert("release".into(), release);

        let cell = Self {
            attack_handle: attack_handle.clone(),
            decay_handle: decay_handle.clone(),
            sustain_handle: sustain_handle.clone(),
            release_handle: release_handle.clone(),
            stage: EnvStage::Idle,
            value: 0.0,
            gate_prev: 0.0,
            sample_rate: sr,
            base_values,
        };

        let handles = vec![
            ("attack".into(), attack_handle),
            ("decay".into(), decay_handle),
            ("sustain".into(), sustain_handle),
            ("release".into(), release_handle),
        ];

        Some((Box::new(cell), handles))
    }
}

impl DspCell for EnvCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let gate = if input.is_empty() { 0.0 } else { input[0] };

        // Edge detection
        let rising_edge = gate > 0.5 && self.gate_prev <= 0.5;
        let falling_edge = gate <= 0.5 && self.gate_prev > 0.5;
        self.gate_prev = gate;

        // Trigger stage transitions
        if rising_edge {
            self.stage = EnvStage::Attack;
        } else if falling_edge && self.stage != EnvStage::Idle {
            self.stage = EnvStage::Release;
        }

        // Process envelope stage
        match self.stage {
            EnvStage::Attack => {
                let attack_time = self.attack_handle.value().max(0.001);
                let rate = 1.0 / (attack_time * self.sample_rate);
                self.value += rate;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                let decay_time = self.decay_handle.value().max(0.001);
                let target = self.sustain_handle.value().clamp(0.0, 1.0);
                let rate = (1.0 - target) / (decay_time * self.sample_rate);
                self.value -= rate;
                if self.value <= target {
                    self.value = target;
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => {
                // Hold at sustain level while gate is high
                self.value = self.sustain_handle.value().clamp(0.0, 1.0);
            }
            EnvStage::Release => {
                let release_time = self.release_handle.value().max(0.001);
                let rate = self.value / (release_time * self.sample_rate);
                self.value -= rate;
                if self.value <= 0.0 {
                    self.value = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
            EnvStage::Idle => {
                self.value = 0.0;
            }
        }

        // Mono output
        if !output.is_empty() {
            output[0] = self.value;
        }
    }

    fn handle_command(&mut self, _cmd: &DspCommand) {
        // Envelope is gate-driven, ignores note commands
    }

    fn analysis(&self) -> DspAnalysis {
        // Envelope is a control signal, not audio
        DspAnalysis {
            rms: 0.0,
            peak: 0.0,
        }
    }

    fn output_channels(&self) -> usize {
        1 // Mono control signal (not audio)
    }

    fn reset(&mut self) {
        self.stage = EnvStage::Idle;
        self.value = 0.0;
        self.gate_prev = 0.0;
    }

    fn name(&self) -> &str {
        "env_cell"
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_test_dna(attack: f32, decay: f32, sustain: f32, release: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("attack".into(), attack);
        params.insert("decay".into(), decay);
        params.insert("sustain".into(), sustain);
        params.insert("release".into(), release);

        CellDna {
            cell_type: "env_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn env_attack_rises() {
        // Value should ramp from 0 to 1 during attack phase
        let dna = make_test_dna(0.1, 0.1, 0.7, 0.3); // 0.1s attack
        let (mut cell, _) = EnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate = [1.0f32; 1]; // Gate high

        // Trigger attack with rising edge
        cell.tick(&gate, &mut output);

        let first_value = output[0];
        assert!(first_value > 0.0, "Should start rising immediately");

        // Continue attack for ~1000 samples
        for _ in 0..1000 {
            cell.tick(&gate, &mut output);
        }

        let mid_attack_value = output[0];
        assert!(
            mid_attack_value > first_value && mid_attack_value < 1.0,
            "Should be ramping up during attack, got {}",
            mid_attack_value
        );

        // Complete attack (0.1s = 4410 samples, add margin)
        for _ in 0..4500 {
            cell.tick(&gate, &mut output);
        }

        assert!(
            output[0] >= 0.90,
            "Should reach near-peak after attack time, got {}",
            output[0]
        );
    }

    #[test]
    fn env_sustain_holds() {
        // Value should stay steady at sustain level during gate-high
        let dna = make_test_dna(0.01, 0.05, 0.6, 0.3);
        let (mut cell, _) = EnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate = [1.0f32; 1];

        // Trigger and complete attack+decay (0.01 + 0.05 = 0.06s = 2646 samples)
        for _ in 0..3000 {
            cell.tick(&gate, &mut output);
        }

        // Should be in sustain stage now
        let sustain_value = output[0];
        assert!(
            (sustain_value - 0.6).abs() < 0.05,
            "Should be at sustain level ~0.6, got {}",
            sustain_value
        );

        // Continue for many samples - should stay steady
        for _ in 0..5000 {
            cell.tick(&gate, &mut output);
        }

        assert!(
            (output[0] - sustain_value).abs() < 0.01,
            "Sustain level should hold steady"
        );
    }

    #[test]
    fn env_release_falls() {
        // Value should ramp down after gate-off
        let dna = make_test_dna(0.01, 0.05, 0.6, 0.1); // 0.1s release
        let (mut cell, _) = EnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate_high = [1.0f32; 1];
        let gate_low = [0.0f32; 1];

        // Trigger and reach sustain
        for _ in 0..3000 {
            cell.tick(&gate_high, &mut output);
        }

        let sustain_value = output[0];

        // Gate off - trigger release
        cell.tick(&gate_low, &mut output);

        // Should start falling
        for _ in 0..1000 {
            cell.tick(&gate_low, &mut output);
        }

        let mid_release_value = output[0];
        assert!(
            mid_release_value < sustain_value && mid_release_value > 0.0,
            "Should be falling during release, was {} now {}",
            sustain_value,
            mid_release_value
        );

        // Complete release (0.1s = 4410 samples + margin)
        for _ in 0..6000 {
            cell.tick(&gate_low, &mut output);
        }

        assert!(
            output[0] < 0.15,
            "Should reach near-zero after release time, got {}",
            output[0]
        );
    }

    #[test]
    fn env_retrigger() {
        // New gate during sustain should restart attack immediately
        let dna = make_test_dna(0.05, 0.05, 0.6, 0.3);
        let (mut cell, _) = EnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate_high = [1.0f32; 1];
        let gate_low = [0.0f32; 1];

        // First trigger - reach sustain
        for _ in 0..5000 {
            cell.tick(&gate_high, &mut output);
        }

        let sustain_value = output[0];
        assert!((sustain_value - 0.6).abs() < 0.05);

        // Gate off briefly
        for _ in 0..100 {
            cell.tick(&gate_low, &mut output);
        }

        // Gate back on - should retrigger
        cell.tick(&gate_high, &mut output);

        // After a few samples of attack, value should be increasing
        for _ in 0..100 {
            cell.tick(&gate_high, &mut output);
        }

        // Should be in attack stage, ramping toward peak
        assert!(
            output[0] > 0.1,
            "Retriggered envelope should be attacking, got {}",
            output[0]
        );
    }

    #[test]
    fn env_zero_attack_instant() {
        // attack=0.001 should reach peak in ~44 samples (minimum time)
        let dna = make_test_dna(0.001, 0.1, 0.7, 0.3);
        let (mut cell, _) = EnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate = [1.0f32; 1];

        // Trigger
        cell.tick(&gate, &mut output);

        // After 50 samples, should be at or near peak
        for _ in 0..50 {
            cell.tick(&gate, &mut output);
        }

        assert!(
            output[0] > 0.9,
            "Very fast attack should reach peak quickly, got {}",
            output[0]
        );
    }
}
