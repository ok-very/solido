//! Short decay envelope for accented steps.
//!
//! Simpler than full ADSR — just peak → decay → zero. Fires on gate rising edge
//! to add punch to filter cutoff or amplitude modulation.
//!
//! # Parameters
//! - `accent_amount`: Peak level (0-1)
//! - `decay`: Decay time in seconds (0.01-1.0)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "accent_env_cell",
//!   "params": { "accent_amount": 0.7, "decay": 0.15 }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Audio input from seq_cell: seq[ch0=gate] → accent_env (trigger on rising edge)
//! - Modulation output to filter: accent_env → filter.cutoff (adds +2kHz punch on accents)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};

/// Valid parameter ranges for accent_env_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("accent_amount", 0.0, 1.0),
    ("decay", 0.001, 2.0),
];
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Short decay envelope for accents — adds punch to filter or amplitude.
///
/// # Parameters
/// - `accent_amount`: Peak level (0-1)
/// - `decay`: Decay time (seconds)
///
/// # Input
/// - Mono gate signal (0.0 or 1.0) from seq_cell
///
/// # Output
/// - Mono envelope value (0.0-accent_amount)
pub struct AccentEnvCell {
    accent_amount_handle: Shared,
    decay_handle: Shared,

    value: f32,
    gate_prev: f32,
    sample_rate: f32,
    base_values: HashMap<String, f32>,
}

impl AccentEnvCell {
    /// Build an accent_env_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell + shared param handles
    /// - `None`: Should never fail (all params have defaults)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let accent_amount = param_or(dna, "accent_amount", 0.7);
        let decay = param_or(dna, "decay", 0.15);

        let accent_amount_handle = shared::shared(accent_amount);
        let decay_handle = shared::shared(decay);

        let mut base_values = HashMap::new();
        base_values.insert("accent_amount".into(), accent_amount);
        base_values.insert("decay".into(), decay);

        let cell = Self {
            accent_amount_handle: accent_amount_handle.clone(),
            decay_handle: decay_handle.clone(),
            value: 0.0,
            gate_prev: 0.0,
            sample_rate: sr,
            base_values,
        };

        let handles = vec![
            ("accent_amount".into(), accent_amount_handle),
            ("decay".into(), decay_handle),
        ];

        Some((Box::new(cell), handles))
    }
}

impl DspCell for AccentEnvCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let gate = if input.is_empty() { 0.0 } else { input[0] };

        // Trigger on rising edge
        if gate > 0.5 && self.gate_prev <= 0.5 {
            self.value = self.accent_amount_handle.value().clamp(0.0, 1.0);
        }
        self.gate_prev = gate;

        // Decay
        if self.value > 0.0 {
            let decay_time = self.decay_handle.value().max(0.01);
            let accent_peak = self.accent_amount_handle.value().clamp(0.0, 1.0);
            // Rate: fall from peak to zero in decay_time
            let rate = accent_peak / (decay_time * self.sample_rate);
            self.value -= rate;
            if self.value < 0.0 {
                self.value = 0.0;
            }
        }

        // Mono output
        if !output.is_empty() {
            output[0] = self.value;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // Accent envelope is a control signal, not audio
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        1 // Mono control signal (not audio)
    }

    fn reset(&mut self) {
        self.value = 0.0;
        self.gate_prev = 0.0;
    }

    fn name(&self) -> &str {
        "accent_env_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_test_dna(accent_amount: f32, decay: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("accent_amount".into(), accent_amount);
        params.insert("decay".into(), decay);

        CellDna {
            cell_type: "accent_env_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn accent_fires_on_trigger() {
        // Rising edge → value jumps to accent_amount
        let dna = make_test_dna(0.8, 0.2);
        let (mut cell, _) = AccentEnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate_low = [0.0f32; 1];
        let gate_high = [1.0f32; 1];

        // Start with gate low
        cell.tick(&gate_low, &mut output);
        assert_eq!(output[0], 0.0, "Should start at zero");

        // Rising edge - should jump to accent_amount
        cell.tick(&gate_high, &mut output);
        assert!(
            (output[0] - 0.8).abs() < 0.01,
            "Should jump to accent_amount={}, got {}",
            0.8,
            output[0]
        );
    }

    #[test]
    fn accent_amount_scales_peak() {
        // Higher amount → higher peak
        let dna_low = make_test_dna(0.3, 0.2);
        let dna_high = make_test_dna(0.9, 0.2);

        let (mut cell_low, _) = AccentEnvCell::new(&dna_low, 44100.0).unwrap();
        let (mut cell_high, _) = AccentEnvCell::new(&dna_high, 44100.0).unwrap();

        let mut output_low = [0.0f32; 1];
        let mut output_high = [0.0f32; 1];
        let gate = [1.0f32; 1];

        // Trigger both
        cell_low.tick(&gate, &mut output_low);
        cell_high.tick(&gate, &mut output_high);

        assert!(
            output_high[0] > output_low[0],
            "Higher accent_amount should produce higher peak: {} vs {}",
            output_high[0],
            output_low[0]
        );

        assert!((output_low[0] - 0.3).abs() < 0.01);
        assert!((output_high[0] - 0.9).abs() < 0.01);
    }

    #[test]
    fn accent_decay_time() {
        // Longer decay → slower fall (measure time to zero)
        let dna = make_test_dna(1.0, 0.1); // 0.1s decay
        let (mut cell, _) = AccentEnvCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 1];
        let gate_high = [1.0f32; 1];
        let gate_low = [0.0f32; 1];

        // Trigger
        cell.tick(&gate_high, &mut output);
        assert!((output[0] - 1.0).abs() < 0.01, "Peak should be 1.0");

        // Gate low, let it decay
        cell.tick(&gate_low, &mut output);

        // After ~half the decay time, should be ~halfway down
        let half_decay_samples = (0.1 * 0.5 * 44100.0) as usize;
        for _ in 0..half_decay_samples {
            cell.tick(&gate_low, &mut output);
        }

        assert!(
            output[0] > 0.4 && output[0] < 0.6,
            "After half decay time, should be ~halfway, got {}",
            output[0]
        );

        // After full decay time + margin, should be near zero
        for _ in 0..(half_decay_samples + 1000) {
            cell.tick(&gate_low, &mut output);
        }

        assert!(
            output[0] < 0.1,
            "After full decay time, should be near zero, got {}",
            output[0]
        );
    }
}
