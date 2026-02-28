//! Low-frequency oscillator (LFO) producing bipolar control signals.
//!
//! Pure control signal generator with manual phase accumulation. No FunDSP
//! AudioUnit overhead — just fast waveform generation for modulation.
//!
//! Output is MONO (1 channel) with bipolar range: -depth to +depth.
//! Phase advances at `rate` Hz, wraps at 1.0.
//!
//! # Parameters
//! - `rate`: LFO frequency (0.01-10 Hz)
//! - `depth`: Modulation depth (0-1), output range is [-depth, +depth]
//!
//! # String Parameters
//! - `shape`: Waveform shape (sine, tri, square, saw)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "lfo_cell",
//!   "params": { "rate": 0.07, "depth": 0.3 },
//!   "string_params": { "shape": "sine" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Modulation source: lfo_cell → filter_cell.cutoff (modulation wire, gain=400)
//! - Common targets: cutoff, resonance, pitch, gain

use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// LFO waveform shapes.
#[derive(Clone, Copy, Debug)]
enum LfoShape {
    Sine,
    Triangle,
    Square,
    Saw,
}

impl LfoShape {
    /// Parse shape from string param.
    fn from_str(s: &str) -> Self {
        match s {
            "tri" | "triangle" => LfoShape::Triangle,
            "square" | "sqr" => LfoShape::Square,
            "saw" => LfoShape::Saw,
            _ => LfoShape::Sine, // Default
        }
    }

    /// Compute waveform output for given phase [0, 1) and depth.
    ///
    /// Returns value in range [-depth, +depth].
    #[inline]
    fn compute(&self, phase: f32, depth: f32) -> f32 {
        match self {
            LfoShape::Sine => {
                // sin(phase * 2π) scaled by depth
                depth * (phase * std::f32::consts::TAU).sin()
            }
            LfoShape::Triangle => {
                // Triangle: -1 at phase=0, +1 at phase=0.5, -1 at phase=1.0
                // Formula: 4 * |phase - 0.5| - 1
                depth * (4.0 * (phase - 0.5).abs() - 1.0)
            }
            LfoShape::Square => {
                // Square: +depth for phase < 0.5, -depth for phase >= 0.5
                if phase < 0.5 {
                    depth
                } else {
                    -depth
                }
            }
            LfoShape::Saw => {
                // Saw: ramp from -depth to +depth over [0, 1)
                depth * (2.0 * phase - 1.0)
            }
        }
    }
}

/// Low-frequency oscillator producing bipolar control signals.
///
/// Manual phase accumulation, no AudioUnit. Mono output in range [-depth, +depth].
///
/// # Parameters
/// - `rate`: LFO frequency (0.01-10 Hz)
/// - `depth`: Modulation depth (0-1)
///
/// # String Parameters
/// - `shape`: Waveform (sine, tri, square, saw)
pub struct LfoCell {
    phase: f32,
    rate_handle: Shared,
    depth_handle: Shared,
    shape: LfoShape,
    base_values: HashMap<String, f32>,
    sample_rate: f32,
}

impl LfoCell {
    /// Build an lfo_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let rate = param_or(dna, "rate", 0.07);
        let depth = param_or(dna, "depth", 0.3);

        // Read waveform shape string param
        let shape_str = string_param_or(dna, "shape", "sine");
        let shape = LfoShape::from_str(shape_str);

        // Our Shared handles for the control thread
        let rate_handle = shared::shared(rate);
        let depth_handle = shared::shared(depth);

        let handles = vec![
            ("rate".into(), rate_handle.clone()),
            ("depth".into(), depth_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let base_values = [
            ("rate", rate),
            ("depth", depth),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        let cell = Self {
            phase: 0.0,
            rate_handle,
            depth_handle,
            shape,
            base_values,
            sample_rate: sr,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for LfoCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Read params
        let rate = self.rate_handle.value().clamp(0.01, 10.0);
        let depth = self.depth_handle.value().clamp(0.0, 1.0);

        // Compute waveform output
        let value = self.shape.compute(self.phase, depth);

        // Advance phase
        self.phase += rate / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Output mono (bipolar control signal)
        output[0] = value;
        // LFO is mono, but replicate to stereo if needed
        if output.len() > 1 {
            output[1] = value;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {} // NoteOn/NoteOff not relevant for LFOs
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // LFO doesn't accumulate RMS/peak (it's a control signal, not audio)
        DspAnalysis { rms: 0.0, peak: 0.0 }
    }

    fn output_channels(&self) -> usize { 1 } // Control signal (not audio)

    fn reset(&mut self) {
        self.phase = 0.0;
    }

    fn name(&self) -> &str { "lfo_cell" }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna(rate: f32, depth: f32, shape: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("rate".into(), rate);
        params.insert("depth".into(), depth);

        let mut string_params = BTreeMap::new();
        string_params.insert("shape".into(), shape.into());

        CellDna {
            cell_type: "lfo_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn lfo_produces_bipolar() {
        let dna = make_dna(1.0, 0.5, "sine");
        let (mut cell, _handles) = LfoCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;

        // Collect samples over 1 second (1 complete cycle at 1 Hz)
        for _ in 0..SR as usize {
            cell.tick(&[], &mut output);
            min_val = min_val.min(output[0]);
            max_val = max_val.max(output[0]);
        }

        // Should swing from approximately -0.5 to +0.5 (depth=0.5)
        assert!(
            min_val < -0.4 && max_val > 0.4,
            "Expected bipolar swing from -0.5 to +0.5, got range [{}, {}]",
            min_val,
            max_val
        );
    }

    #[test]
    fn lfo_rate_changes_speed() {
        // Test two different rates: 1 Hz and 4 Hz
        let dna_slow = make_dna(1.0, 0.5, "sine");
        let (mut cell_slow, _) = LfoCell::new(&dna_slow, SR).unwrap();

        let dna_fast = make_dna(4.0, 0.5, "sine");
        let (mut cell_fast, _) = LfoCell::new(&dna_fast, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Count zero crossings over 1 second
        let mut crossings_slow = 0;
        let mut crossings_fast = 0;
        let mut last_slow = 0.0f32;
        let mut last_fast = 0.0f32;

        for _ in 0..SR as usize {
            cell_slow.tick(&[], &mut output);
            if last_slow < 0.0 && output[0] >= 0.0 {
                crossings_slow += 1;
            }
            last_slow = output[0];

            cell_fast.tick(&[], &mut output);
            if last_fast < 0.0 && output[0] >= 0.0 {
                crossings_fast += 1;
            }
            last_fast = output[0];
        }

        // 4 Hz should have roughly 4× as many crossings as 1 Hz
        assert!(
            crossings_fast as f32 > crossings_slow as f32 * 3.0,
            "Expected 4Hz to have ~4× crossings of 1Hz, got {} vs {}",
            crossings_fast,
            crossings_slow
        );
    }

    #[test]
    fn lfo_shapes_differ() {
        // Verify different shapes produce different waveforms
        let shapes = ["sine", "tri", "square", "saw"];
        let mut waveforms = Vec::new();

        for shape in &shapes {
            let dna = make_dna(1.0, 0.5, shape);
            let (mut cell, _) = LfoCell::new(&dna, SR).unwrap();

            let mut output = [0.0f32; 2];
            let mut samples = Vec::new();

            // Collect 100 samples
            for _ in 0..100 {
                cell.tick(&[], &mut output);
                samples.push(output[0]);
            }

            waveforms.push((*shape, samples));
        }

        // Compare sine vs square (should be very different)
        let sine_samples = &waveforms.iter().find(|(s, _)| *s == "sine").unwrap().1;
        let square_samples = &waveforms.iter().find(|(s, _)| *s == "square").unwrap().1;

        let mut diff_count = 0;
        for (s1, s2) in sine_samples.iter().zip(square_samples.iter()) {
            if (s1 - s2).abs() > 0.1 {
                diff_count += 1;
            }
        }

        assert!(
            diff_count > 50,
            "Expected sine and square to differ significantly, only {} / 100 samples differ",
            diff_count
        );
    }

    #[test]
    fn lfo_depth_zero_is_silent() {
        let dna = make_dna(1.0, 0.0, "sine");
        let (mut cell, _handles) = LfoCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // All samples should be zero with depth=0
        for _ in 0..1000 {
            cell.tick(&[], &mut output);
            assert!(
                output[0].abs() < 0.001,
                "Expected depth=0 to produce silence, got {}",
                output[0]
            );
        }
    }
}
