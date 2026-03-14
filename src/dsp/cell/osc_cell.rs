//! Dual detuned oscillators with waveform selection and stereo spread.
//!
//! Produces stereo output with slight pan spread for width. Oscillator 1 panned
//! slightly left (60% left channel), oscillator 2 panned slightly right (60% right
//! channel). Detune specified in cents (100 cents = 1 semitone).
//!
//! # Parameters
//! - `freq`: Base frequency (20-2000 Hz)
//! - `det`: Detune amount (0-50 cents)
//! - `gain`: Output level (0-1)
//!
//! # String Parameters
//! - `wtype`: Waveform type (soft_saw, saw, sine, square, triangle)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "osc_cell",
//!   "params": { "freq": 110.0, "det": 7.0, "gain": 0.5 },
//!   "string_params": { "wtype": "soft_saw" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Source cell (no inputs): osc_cell → filter_cell (audio wire)
//! - Modulation target: lfo_cell → osc_cell.freq (modulation wire)

use std::any::Any;

use fundsp::hacker32::*;
use fundsp::shared::Shared as FundspShared;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for osc_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("freq", 20.0, 20000.0),
    ("det", 0.0, 100.0),
    ("gain", 0.0, 1.0),
    ("pw", 0.01, 0.99),
];

/// Fast 2^x approximation for |x| <= 1.0 (3rd-order Taylor series).
///
/// Max error ~0.008% at x=±0.3, ~0.3% at x=±1.0. Avoids per-sample powf().
#[inline]
fn fast_pow2(x: f32) -> f32 {
    const LN2: f32 = 0.693_147_2;
    let a = x * LN2;
    // 1 + a + a²/2 + a³/6
    1.0 + a * (1.0 + a * (0.5 + a * (1.0 / 6.0)))
}

/// Dual detuned oscillators with waveform selection.
///
/// Produces stereo output with slight pan spread. Oscillator 1 panned left,
/// oscillator 2 panned right for width. Detune specified in cents.
///
/// # Parameters
/// - `freq`: Base frequency (20-2000 Hz)
/// - `det`: Detune amount (0-50 cents)
/// - `gain`: Output level (0-1)
///
/// # String Parameters
/// - `wtype`: Waveform type (soft_saw, saw, sine, square, triangle, pulse)
pub struct OscCell {
    osc1: Box<dyn AudioUnit>,
    osc2: Box<dyn AudioUnit>,
    freq_shared: FundspShared,
    freq_det_shared: FundspShared,
    pw_shared: Option<FundspShared>, // For pulse mode only
    freq_handle: Shared,
    det_handle: Shared,
    gain_handle: Shared,
    pw_handle: Option<Shared>, // For pulse mode only
    wtype: String,              // Store to check if pulse mode in tick()
    /// Waveform-dependent constant for frequency normalization.
    /// 0.0 = sine (no normalization needed).
    /// For band-limited waveforms: peak ≈ norm_c / sqrt(N_harmonics).
    norm_c: f32,
    cached_det: f32,
    cached_det_ratio: f32,
    /// One-pole smoothed frequency to prevent phase discontinuity clicks.
    /// Converges to target freq with ~2ms time constant at 44.1kHz.
    smoothed_freq: f32,
    base_values: HashMap<String, f32>,
    #[allow(dead_code)]
    sample_rate: f32,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl OscCell {
    /// Build an osc_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let freq = param_or(dna, "freq", 110.0);
        let det = param_or(dna, "det", 7.0);
        let gain = param_or(dna, "gain", 0.5);
        let pw = param_or(dna, "pw", 0.5);

        // Read waveform type string param
        let wtype = string_param_or(dna, "wtype", "soft_saw");

        // FunDSP shareds for reactive frequency control
        let freq_shared = FundspShared::new(freq);
        let freq_det_shared = FundspShared::new(freq * fast_pow2(det / 1200.0));

        // Pulse width shared (only used if wtype == "pulse")
        let pw_shared = if wtype == "pulse" {
            Some(FundspShared::new(pw))
        } else {
            None
        };

        // Build oscillators from wtype
        let pw_shared_ref = pw_shared.as_ref();
        let osc1 = super::build_osc(wtype, &freq_shared, pw_shared_ref, sr);
        let osc2 = super::build_osc(wtype, &freq_det_shared, pw_shared_ref, sr);

        // Our Shared handles for the control thread
        let freq_handle = shared::shared(freq);
        let det_handle = shared::shared(det);
        let gain_handle = shared::shared(gain);
        let pw_handle = if wtype == "pulse" {
            Some(shared::shared(pw))
        } else {
            None
        };

        let mut handles = vec![
            ("freq".into(), freq_handle.clone()),
            ("det".into(), det_handle.clone()),
            ("gain".into(), gain_handle.clone()),
        ];
        if let Some(ref h) = pw_handle {
            handles.push(("pw".into(), h.clone()));
        }

        // Store base values from DNA for additive modulation
        let mut base_values: HashMap<String, f32> = [
            ("freq", freq),
            ("det", det),
            ("gain", gain),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        if wtype == "pulse" {
            base_values.insert("pw".into(), pw);
        }

        // Waveform normalization constant.
        // Band-limited oscillators have frequency-dependent peak: peak ≈ C / sqrt(N)
        // where N = harmonics below Nyquist. We store C per waveform type so tick()
        // can compute norm = sqrt(N) / C to bring output to ~unity at any frequency.
        let norm_c = match wtype {
            "sine" => 0.0,    // sine outputs at unity — no normalization
            "square" => 2.23, // measured from FunDSP square() output
            _ => 2.63,        // soft_saw, saw, triangle, pulse
        };

        let cell = Self {
            osc1,
            osc2,
            freq_shared,
            freq_det_shared,
            pw_shared,
            freq_handle,
            det_handle,
            gain_handle,
            pw_handle,
            wtype: wtype.to_string(),
            norm_c,
            cached_det: det,
            cached_det_ratio: fast_pow2(det / 1200.0),
            smoothed_freq: freq, // sample-first-input: start at DNA freq
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for OscCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Bridge: read our Shared params, write to FunDSP shareds
        let freq = self.freq_handle.value();
        let det = self.det_handle.value();
        let gain = clamp_param(PARAM_RANGES, "gain", self.gain_handle.value());

        // Cache detune ratio — only recompute when det actually changes
        if det != self.cached_det {
            self.cached_det = det;
            self.cached_det_ratio = fast_pow2(det / 1200.0);
        }

        // One-pole smoother: ~2ms convergence at 44.1kHz. Prevents phase discontinuity clicks
        // when seq_cell Turing Machine jumps to a new pitch.
        const FREQ_SMOOTH_ALPHA: f32 = 0.002;
        self.smoothed_freq += (freq - self.smoothed_freq) * FREQ_SMOOTH_ALPHA;
        self.freq_shared.set_value(self.smoothed_freq);
        self.freq_det_shared.set_value(self.smoothed_freq * self.cached_det_ratio);

        // Bridge pw param if in pulse mode
        if self.wtype == "pulse" {
            if let (Some(ref pw_shared), Some(ref pw_handle)) = (&self.pw_shared, &self.pw_handle) {
                let pw = clamp_param(PARAM_RANGES, "pw", pw_handle.value());
                pw_shared.set_value(pw);
            }
        }

        // Process one sample from each oscillator
        let s1 = self.osc1.get_mono();
        let s2 = self.osc2.get_mono();

        // Frequency-dependent amplitude normalization for band-limited waveforms.
        // FunDSP oscillators output peak ≈ norm_c / sqrt(N_harmonics) where
        // N = nyquist / freq. Normalizing by sqrt(N) / norm_c gives ~unity at
        // any frequency — like a mechanical taper pot compensating for the circuit.
        let norm = if self.norm_c > 0.0 {
            let n_harmonics = (self.sample_rate * 0.5 / freq.max(20.0)).max(1.0);
            (n_harmonics.sqrt() / self.norm_c).clamp(0.5, 8.0)
        } else {
            1.0
        };
        let scaled_gain = gain * norm;

        // Stereo spread: osc1 60% left, osc2 60% right
        // This creates subtle width while maintaining mono compatibility
        let left = (s1 * 0.6 + s2 * 0.4) * scaled_gain;
        let right = (s1 * 0.4 + s2 * 0.6) * scaled_gain;

        // Analysis accumulation (use average of both channels)
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
            _ => {} // NoteOn/NoteOff not relevant for oscillators
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // Return accumulated RMS and peak, then consumer should reset via analysis()
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
        self.osc1.reset();
        self.osc2.reset();
        self.smoothed_freq = self.freq_handle.value();
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "osc_cell" }

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

    fn make_dna(freq: f32, det: f32, gain: f32, wtype: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), freq);
        params.insert("det".into(), det);
        params.insert("gain".into(), gain);

        let mut string_params = BTreeMap::new();
        string_params.insert("wtype".into(), wtype.into());

        CellDna {
            cell_type: "osc_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn osc_produces_signal() {
        let dna = make_dna(440.0, 0.0, 0.5, "soft_saw");
        let (mut cell, _handles) = OscCell::new(&dna, SR).unwrap();

        // Warmup period (oscillators need a few samples to settle)
        let mut output = [0.0f32; 2];
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }

        // Check for non-zero output over one period (440 Hz = 100 samples at 44.1kHz)
        let mut any_nonzero = false;
        for _ in 0..100 {
            cell.tick(&[], &mut output);
            if output[0].abs() > 0.001 || output[1].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Expected non-zero audio output");
    }

    #[test]
    fn osc_freq_tracks_shared() {
        let dna = make_dna(110.0, 0.0, 0.5, "sine");
        let (mut cell, handles) = OscCell::new(&dna, SR).unwrap();
        let freq_handle = handles.iter().find(|(n, _)| n == "freq").map(|(_, h)| h).unwrap();

        let mut output = [0.0f32; 2];

        // Count zero crossings at 110 Hz over 0.1 seconds (4410 samples)
        let mut crossings_110 = 0;
        let mut last_sample = 0.0f32;
        for _ in 0..4410 {
            cell.tick(&[], &mut output);
            if last_sample < 0.0 && output[0] >= 0.0 {
                crossings_110 += 1;
            }
            last_sample = output[0];
        }

        // Change frequency to 220 Hz
        freq_handle.set(220.0);
        for _ in 0..100 { cell.tick(&[], &mut output); } // Settle

        // Count zero crossings at 220 Hz over 0.1 seconds
        let mut crossings_220 = 0;
        last_sample = 0.0f32;
        for _ in 0..4410 {
            cell.tick(&[], &mut output);
            if last_sample < 0.0 && output[0] >= 0.0 {
                crossings_220 += 1;
            }
            last_sample = output[0];
        }

        // 220 Hz should have roughly twice as many crossings as 110 Hz
        assert!(
            crossings_220 as f32 > crossings_110 as f32 * 1.5,
            "Expected ~2× crossings at 220Hz vs 110Hz, got {} vs {}",
            crossings_220,
            crossings_110
        );
    }

    #[test]
    fn osc_detune_spreads() {
        let dna_unison = make_dna(110.0, 0.0, 0.5, "soft_saw");
        let (mut cell_unison, _) = OscCell::new(&dna_unison, SR).unwrap();

        let dna_detune = make_dna(110.0, 10.0, 0.5, "soft_saw");
        let (mut cell_detune, _) = OscCell::new(&dna_detune, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Measure stereo spread: detune should create difference between L and R
        let mut unison_lr_diff = 0.0f32;
        let mut detune_lr_diff = 0.0f32;

        for _ in 0..SR as usize {
            cell_unison.tick(&[], &mut output);
            unison_lr_diff += (output[0] - output[1]).abs();

            cell_detune.tick(&[], &mut output);
            detune_lr_diff += (output[0] - output[1]).abs();
        }

        // Detune should create more L/R difference due to oscillators at different frequencies
        assert!(
            detune_lr_diff > unison_lr_diff * 1.1,
            "Expected detune to create more stereo spread, got {} vs {}",
            detune_lr_diff,
            unison_lr_diff
        );
    }

    #[test]
    fn osc_wtype_changes_timbre() {
        // Just verify that different waveforms produce different outputs
        let dna_sine = make_dna(440.0, 0.0, 0.5, "sine");
        let (mut cell_sine, _) = OscCell::new(&dna_sine, SR).unwrap();

        let dna_saw = make_dna(440.0, 0.0, 0.5, "saw");
        let (mut cell_saw, _) = OscCell::new(&dna_saw, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Warmup
        for _ in 0..100 {
            cell_sine.tick(&[], &mut output);
            cell_saw.tick(&[], &mut output);
        }

        // Collect samples over one period
        let mut samples_sine = Vec::new();
        let mut samples_saw = Vec::new();

        for _ in 0..100 {
            cell_sine.tick(&[], &mut output);
            samples_sine.push(output[0]);

            cell_saw.tick(&[], &mut output);
            samples_saw.push(output[0]);
        }

        // Samples should be different (not identical waveforms)
        let mut diff_count = 0;
        for (s1, s2) in samples_sine.iter().zip(samples_saw.iter()) {
            if (s1 - s2).abs() > 0.01 {
                diff_count += 1;
            }
        }

        assert!(
            diff_count > 50,
            "Expected sine and saw waveforms to differ significantly, only {} / 100 samples differ",
            diff_count
        );
    }

    #[test]
    fn osc_gain_scales() {
        let dna_zero = make_dna(440.0, 0.0, 0.0, "soft_saw");
        let (mut cell_zero, _) = OscCell::new(&dna_zero, SR).unwrap();

        let dna_half = make_dna(440.0, 0.0, 0.5, "soft_saw");
        let (mut cell_half, _) = OscCell::new(&dna_half, SR).unwrap();

        let dna_full = make_dna(440.0, 0.0, 1.0, "soft_saw");
        let (mut cell_full, _) = OscCell::new(&dna_full, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Measure peak over 1000 samples
        let mut peak_zero = 0.0f32;
        let mut peak_half = 0.0f32;
        let mut peak_full = 0.0f32;

        for _ in 0..1000 {
            cell_zero.tick(&[], &mut output);
            peak_zero = peak_zero.max(output[0].abs());

            cell_half.tick(&[], &mut output);
            peak_half = peak_half.max(output[0].abs());

            cell_full.tick(&[], &mut output);
            peak_full = peak_full.max(output[0].abs());
        }

        // gain=0 should be effectively silent
        assert!(peak_zero < 0.001, "Expected gain=0 to be silent, got peak {}", peak_zero);

        // gain=1.0 should be roughly 2× gain=0.5 (accounting for 0.5× pre-scaling)
        assert!(
            peak_full > peak_half * 1.5,
            "Expected gain=1.0 to produce higher output than gain=0.5, got {} vs {}",
            peak_full,
            peak_half
        );
    }

    #[test]
    fn osc_waveform_levels_boosted() {
        // All waveforms at gain=1.0 should produce audible output (×2 norm for non-sine)
        for wtype in &["soft_saw", "saw", "sine", "square", "triangle"] {
            let dna = make_dna(110.0, 0.0, 1.0, wtype);
            let (mut cell, _) = OscCell::new(&dna, SR).unwrap();

            let mut output = [0.0f32; 2];
            let mut peak = 0.0f32;

            for _ in 0..SR as usize {
                cell.tick(&[], &mut output);
                peak = peak.max(output[0].abs()).max(output[1].abs());
            }

            assert!(
                peak > 0.2,
                "{} at gain=1.0 should produce peak > 0.2, got {:.4}",
                wtype, peak
            );
        }
    }

    // Helper: Calculate RMS variance of a sample buffer
    fn rms_variance(values: &[f32]) -> f32 {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
    }

    #[test]
    fn freq_smoother_no_instant_jump() {
        // After a sudden freq change, smoothed_freq should be intermediate (not at target)
        let dna = make_dna(110.0, 0.0, 0.5, "sine");
        let (mut cell, handles) = OscCell::new(&dna, SR).unwrap();
        let freq_handle = handles.iter().find(|(n, _)| n == "freq").map(|(_, h)| h).unwrap();

        let mut output = [0.0f32; 2];
        // Settle at 110 Hz
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }

        // Jump to 440 Hz
        freq_handle.set(440.0);
        cell.tick(&[], &mut output);

        // Access smoothed_freq via downcast
        let osc = cell.as_any().downcast_ref::<OscCell>().unwrap();
        assert!(
            osc.smoothed_freq < 440.0 && osc.smoothed_freq > 110.0,
            "smoothed_freq should be intermediate after 1 tick: {}",
            osc.smoothed_freq
        );
    }

    #[test]
    fn freq_smoother_converges() {
        // alpha=0.002: residual after N ticks = (1-0.002)^N.
        // At 3000 ticks: (0.998)^3000 ≈ 0.0025 → 0.25% error.
        let dna = make_dna(110.0, 0.0, 0.5, "sine");
        let (mut cell, handles) = OscCell::new(&dna, SR).unwrap();
        let freq_handle = handles.iter().find(|(n, _)| n == "freq").map(|(_, h)| h).unwrap();

        let mut output = [0.0f32; 2];
        // Settle at 110 Hz
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }

        // Jump to 440 Hz and tick 3000 times
        freq_handle.set(440.0);
        for _ in 0..3000 {
            cell.tick(&[], &mut output);
        }

        let osc = cell.as_any().downcast_ref::<OscCell>().unwrap();
        let error = (osc.smoothed_freq - 440.0).abs() / 440.0;
        assert!(
            error < 0.01,
            "smoothed_freq should converge within 1%: {}, error={}",
            osc.smoothed_freq, error
        );
    }
}
