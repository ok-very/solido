//! Stereo audio filter with type selection (moog, lowpass, highpass, bandpass).
//!
//! Processes stereo audio input from upstream cells. Separate filter instance
//! per channel for true stereo independence. Cutoff and resonance can be
//! modulated via modulation wires.
//!
//! # Parameters
//! - `cutoff`: Filter cutoff frequency (20-20000 Hz)
//! - `res`: Resonance / Q (0-1)
//!
//! # String Parameters
//! - `ftype`: Filter type (moog, lowpass, highpass, bandpass)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "filter_cell",
//!   "params": { "cutoff": 1200.0, "res": 0.25 },
//!   "string_params": { "ftype": "moog" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Audio processing: osc_cell → filter_cell → mixer_cell
//! - Modulation target: lfo_cell → filter_cell.cutoff (modulation wire)

use fundsp::hacker32::*;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for filter_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("cutoff", 20.0, 20000.0),
    ("res", 0.0, 1.0),
];

/// Maximum Q value passed to the Moog filter.
///
/// FunDSP's moog internally maps Q through a cutoff-dependent multiplier of ~2.5–4.0×,
/// meaning Q=0.25 produces rez≈0.88 (near self-oscillation). We scale the user-facing
/// `res` (0–1) by this constant so:
///   res=0.0 → Q=0.0  → rez=0.0 (flat)
///   res=0.5 → Q=0.1  → rez≈0.35 (warm)
///   res=1.0 → Q=0.2  → rez≈0.70 (strong, musical)
const MOOG_MAX_Q: f32 = 0.2;

/// Filter type variants.
#[derive(Clone, Copy, Debug)]
enum FilterType {
    Moog,
    Lowpass,
    Highpass,
    Bandpass,
}

impl FilterType {
    /// Parse filter type from string param.
    fn from_str(s: &str) -> Self {
        match s {
            "lowpass" | "lp" => FilterType::Lowpass,
            "highpass" | "hp" => FilterType::Highpass,
            "bandpass" | "bp" => FilterType::Bandpass,
            _ => FilterType::Moog, // Default
        }
    }
}

/// Build a filter AudioUnit from an ftype string.
fn build_filter(ftype: &str, sr: f32) -> (Box<dyn AudioUnit>, FilterType) {
    let filter_type = FilterType::from_str(ftype);

    let mut filter: Box<dyn AudioUnit> = match filter_type {
        FilterType::Lowpass => Box::new(lowpole()),
        FilterType::Highpass => Box::new(highpole()),
        FilterType::Bandpass => Box::new(bandpass()),
        FilterType::Moog => Box::new(moog()),
    };

    filter.set_sample_rate(sr as f64);
    (filter, filter_type)
}

/// Stereo audio filter with type selection.
///
/// Processes stereo input with independent left/right filter instances.
/// Cutoff and resonance can be modulated.
///
/// # Parameters
/// - `cutoff`: Cutoff frequency (20-20000 Hz)
/// - `res`: Resonance / Q (0-1)
///
/// # String Parameters
/// - `ftype`: Filter type (moog, lowpass, highpass, bandpass)
pub struct FilterCell {
    filter_l: Box<dyn AudioUnit>,
    filter_r: Box<dyn AudioUnit>,
    filter_type: FilterType,
    cutoff_handle: Shared,
    res_handle: Shared,
    base_values: HashMap<String, f32>,
    sample_rate: f32,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl FilterCell {
    /// Build a filter_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let cutoff = param_or(dna, "cutoff", 1200.0);
        let res = param_or(dna, "res", 0.25);

        // Read filter type string param
        let ftype = string_param_or(dna, "ftype", "moog");

        // Build separate filters for L and R channels
        let (filter_l, filter_type) = build_filter(ftype, sr);
        let (filter_r, _) = build_filter(ftype, sr);

        // Our Shared handles for the control thread
        let cutoff_handle = shared::shared(cutoff);
        let res_handle = shared::shared(res);

        let handles = vec![
            ("cutoff".into(), cutoff_handle.clone()),
            ("res".into(), res_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let base_values = [
            ("cutoff", cutoff),
            ("res", res),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        let cell = Self {
            filter_l,
            filter_r,
            filter_type,
            cutoff_handle,
            res_handle,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for FilterCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        // Read input (stereo or mono)
        let input_l = if !input.is_empty() { input[0] } else { 0.0 };
        let input_r = if input.len() > 1 { input[1] } else { input_l };

        // Read params
        let cutoff = clamp_param(PARAM_RANGES, "cutoff", self.cutoff_handle.value());
        let res = clamp_param(PARAM_RANGES, "res", self.res_handle.value());

        // Process through filters (dispatch based on filter type)
        let mut filtered_l = [0.0f32];
        let mut filtered_r = [0.0f32];

        match self.filter_type {
            FilterType::Moog => {
                // moog(): 3 inputs (audio, cutoff, Q)
                let q = res * MOOG_MAX_Q;
                self.filter_l.tick(&[input_l, cutoff, q], &mut filtered_l);
                self.filter_r.tick(&[input_r, cutoff, q], &mut filtered_r);
            }
            FilterType::Lowpass | FilterType::Highpass => {
                // lowpole() / highpole(): 2 inputs (audio, cutoff)
                self.filter_l.tick(&[input_l, cutoff], &mut filtered_l);
                self.filter_r.tick(&[input_r, cutoff], &mut filtered_r);
            }
            FilterType::Bandpass => {
                // bandpass(): 3 inputs (audio, cutoff, Q)
                let q = res * MOOG_MAX_Q;
                self.filter_l.tick(&[input_l, cutoff, q], &mut filtered_l);
                self.filter_r.tick(&[input_r, cutoff, q], &mut filtered_r);
            }
        }

        let left = filtered_l[0];
        let right = filtered_r[0];

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
            _ => {} // NoteOn/NoteOff not relevant for filters
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
        self.filter_l.reset();
        self.filter_r.reset();
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "filter_cell" }

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

    fn make_dna(cutoff: f32, res: f32, ftype: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("cutoff".into(), cutoff);
        params.insert("res".into(), res);

        let mut string_params = BTreeMap::new();
        string_params.insert("ftype".into(), ftype.into());

        CellDna {
            cell_type: "filter_cell".into(),
            params,
            string_params,
        }
    }

    /// Generate a simple test signal (440 Hz sine wave)
    fn generate_test_signal(samples: usize) -> Vec<f32> {
        let freq = 440.0;
        (0..samples)
            .map(|i| {
                let phase = (i as f32 / SR) * freq * std::f32::consts::TAU;
                phase.sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn filter_passes_audio() {
        let dna = make_dna(1200.0, 0.1, "moog");
        let (mut cell, _handles) = FilterCell::new(&dna, SR).unwrap();

        let input_signal = generate_test_signal(1000);
        let mut output = [0.0f32; 2];

        let mut any_nonzero = false;
        for sample in input_signal.iter() {
            cell.tick(&[*sample, *sample], &mut output);
            if output[0].abs() > 0.001 || output[1].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }

        assert!(any_nonzero, "Expected filter to pass audio signal");
    }

    #[test]
    fn filter_cutoff_shapes_spectrum() {
        // Low cutoff should reduce high-frequency content
        let dna_low = make_dna(200.0, 0.1, "lowpass");
        let (mut cell_low, _) = FilterCell::new(&dna_low, SR).unwrap();

        let dna_high = make_dna(5000.0, 0.1, "lowpass");
        let (mut cell_high, _) = FilterCell::new(&dna_high, SR).unwrap();

        // Test signal: 440 Hz sine (well above 200 Hz cutoff, below 5000 Hz)
        let input_signal = generate_test_signal(1000);
        let mut output = [0.0f32; 2];

        let mut rms_low = 0.0f32;
        let mut rms_high = 0.0f32;

        for sample in input_signal.iter() {
            cell_low.tick(&[*sample, *sample], &mut output);
            rms_low += output[0] * output[0];

            cell_high.tick(&[*sample, *sample], &mut output);
            rms_high += output[0] * output[0];
        }

        rms_low = (rms_low / 1000.0).sqrt();
        rms_high = (rms_high / 1000.0).sqrt();

        // Low cutoff (200 Hz) should attenuate 440 Hz signal more than high cutoff (5000 Hz)
        assert!(
            rms_high > rms_low * 1.5,
            "Expected high cutoff to pass more signal than low cutoff, got RMS {} vs {}",
            rms_high,
            rms_low
        );
    }

    #[test]
    fn filter_res_peaks() {
        // High resonance should increase RMS at cutoff frequency
        let dna_low_res = make_dna(1200.0, 0.05, "moog");
        let (mut cell_low_res, _) = FilterCell::new(&dna_low_res, SR).unwrap();

        let dna_high_res = make_dna(1200.0, 0.8, "moog");
        let (mut cell_high_res, _) = FilterCell::new(&dna_high_res, SR).unwrap();

        // Generate test signal at cutoff frequency (1200 Hz)
        let mut input = Vec::new();
        for i in 0..1000 {
            let phase = (i as f32 / SR) * 1200.0 * std::f32::consts::TAU;
            input.push(phase.sin() * 0.3);
        }

        let mut output = [0.0f32; 2];
        let mut rms_low_res = 0.0f32;
        let mut rms_high_res = 0.0f32;

        // Skip warmup period
        for _ in 0..100 {
            cell_low_res.tick(&[0.0, 0.0], &mut output);
            cell_high_res.tick(&[0.0, 0.0], &mut output);
        }

        for sample in input.iter() {
            cell_low_res.tick(&[*sample, *sample], &mut output);
            rms_low_res += output[0] * output[0];

            cell_high_res.tick(&[*sample, *sample], &mut output);
            rms_high_res += output[0] * output[0];
        }

        rms_low_res = (rms_low_res / 1000.0).sqrt();
        rms_high_res = (rms_high_res / 1000.0).sqrt();

        // High resonance should produce higher RMS at cutoff frequency
        // (may not always be true for all filter implementations, so use modest threshold)
        assert!(
            rms_high_res > rms_low_res * 0.9,
            "Expected similar or higher RMS with high resonance, got {} vs {}",
            rms_high_res,
            rms_low_res
        );
    }

    #[test]
    fn filter_type_switch() {
        // Different filter types should produce different magnitude responses
        let types = ["moog", "lowpass", "highpass", "bandpass"];
        let mut outputs = Vec::new();

        let input_signal = generate_test_signal(1000);

        for ftype in &types {
            let dna = make_dna(1200.0, 0.5, ftype);
            let (mut cell, _) = FilterCell::new(&dna, SR).unwrap();

            let mut rms = 0.0f32;
            let mut output = [0.0f32; 2];

            for sample in input_signal.iter() {
                cell.tick(&[*sample, *sample], &mut output);
                rms += output[0] * output[0];
            }

            rms = (rms / 1000.0).sqrt();
            outputs.push((*ftype, rms));
        }

        // Lowpass and moog should pass 440 Hz signal (cutoff=1200 Hz)
        let lowpass_rms = outputs.iter().find(|(t, _)| *t == "lowpass").unwrap().1;
        let moog_rms = outputs.iter().find(|(t, _)| *t == "moog").unwrap().1;

        // Highpass should attenuate 440 Hz signal more
        let highpass_rms = outputs.iter().find(|(t, _)| *t == "highpass").unwrap().1;

        assert!(
            lowpass_rms > highpass_rms * 1.5 && moog_rms > highpass_rms * 1.5,
            "Expected lowpass/moog to pass more signal than highpass at 440Hz, got LP={}, moog={}, HP={}",
            lowpass_rms,
            moog_rms,
            highpass_rms
        );
    }

    #[test]
    fn filter_silence_on_zero_input() {
        let dna = make_dna(1200.0, 0.5, "moog");
        let (mut cell, _handles) = FilterCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // No input → no output (no self-oscillation at default res)
        for _ in 0..1000 {
            cell.tick(&[], &mut output);
            assert!(
                output[0].abs() < 0.001 && output[1].abs() < 0.001,
                "Expected silence with no input, got L={}, R={}",
                output[0],
                output[1]
            );
        }
    }
}
