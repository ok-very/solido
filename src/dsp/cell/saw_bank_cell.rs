//! N-voice detuned saw oscillator bank with stereo panning.
//!
//! Produces dense, detuned saw layers with voices spread across the stereo field.
//! Gain is normalized by 1/√N to prevent level increase with more voices.
//!
//! # Parameters
//! - `freq`: Base frequency (20-2000 Hz)
//! - `voices`: Number of oscillators (1-8, construction-time only)
//! - `spread`: Detune spread in cents (0-100)
//! - `gain`: Output level (0-1)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "saw_bank_cell",
//!   "params": { "freq": 55.0, "voices": 6.0, "spread": 15.0, "gain": 0.4 }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Source cell: saw_bank → filter_cell (audio wire)
//! - Modulation target: func_gen → saw_bank.freq (glacial pitch drift)

use fundsp::hacker32::*;
use fundsp::shared::Shared as FundspShared;
use std::collections::HashMap;

use crate::dsp::cell::{DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Fast 2^x approximation for |x| <= 1.0 (3rd-order Taylor series).
///
/// Reused from osc_cell. Max error ~0.008% at x=±0.3, ~0.3% at x=±1.0.
#[inline]
fn fast_pow2(x: f32) -> f32 {
    const LN2: f32 = 0.693_147_2;
    let a = x * LN2;
    // 1 + a + a²/2 + a³/6
    1.0 + a * (1.0 + a * (0.5 + a * (1.0 / 6.0)))
}

/// N-voice detuned saw oscillator bank.
///
/// Voices are symmetrically detuned around base frequency and panned across
/// stereo field with equal-power panning law. Gain normalized by 1/√N.
///
/// # Parameters
/// - `freq`: Base frequency (20-2000 Hz)
/// - `voices`: Number of oscillators (1-8, **construction-time only**)
/// - `spread`: Detune spread in cents (0-100)
/// - `gain`: Output level (0-1)
///
/// **Note**: `voices` parameter is fixed at construction. Modulating it at runtime
/// reads the value but does not rebuild the oscillator array.
pub struct SawBankCell {
    oscillators: Vec<Box<dyn AudioUnit>>,  // N FunDSP saw() units
    freq_shareds: Vec<FundspShared>,       // One per oscillator
    freq_handle: Shared,
    voices_handle: Shared,   // Read-only at runtime (can't rebuild array)
    spread_handle: Shared,
    gain_handle: Shared,
    voice_count: usize,      // Fixed at construction
    cached_freq: f32,        // Last freq value - only update FunDSP when changed
    cached_spread: f32,      // Last spread value - only recompute ratios when changed
    first_tick: bool,        // Force update on first tick
    detune_ratios: Vec<f32>, // Precomputed freq multipliers
    pan_coeffs: Vec<(f32, f32)>, // Precomputed (left, right) pan coefficients
    base_values: HashMap<String, f32>,
    sample_rate: f32,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl SawBankCell {
    /// Build a saw_bank_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let freq = dna.params.get("freq").copied().unwrap_or(55.0);
        let voices = dna.params.get("voices").copied().unwrap_or(6.0);
        let spread = dna.params.get("spread").copied().unwrap_or(15.0);
        let gain = dna.params.get("gain").copied().unwrap_or(0.4);

        let voice_count = voices.clamp(1.0, 8.0) as usize;

        // Build N saw oscillators with symmetric detune spread
        let mut oscillators = Vec::with_capacity(voice_count);
        let mut freq_shareds = Vec::with_capacity(voice_count);
        let mut detune_ratios = Vec::with_capacity(voice_count);
        let mut pan_coeffs = Vec::with_capacity(voice_count);

        for i in 0..voice_count {
            // Symmetric detune: center voices around base frequency
            // For N voices: voice 0 at -spread/2, voice N-1 at +spread/2
            let voice_idx_centered = (i as f32) - ((voice_count - 1) as f32 / 2.0);
            let cents_offset = if voice_count > 1 {
                (spread / ((voice_count - 1) as f32 / 2.0)) * voice_idx_centered
            } else {
                0.0 // Single voice: no detune
            };
            let freq_ratio = fast_pow2(cents_offset / 1200.0);
            detune_ratios.push(freq_ratio);

            // Create FunDSP shared with detuned frequency
            let freq_shared = FundspShared::new(freq * freq_ratio);
            freq_shareds.push(freq_shared.clone());

            // Build soft_saw oscillator (gentler harmonics than saw)
            // soft_saw has triangle-like harmonic rolloff, better for dense voice stacking
            let mut osc: Box<dyn AudioUnit> = Box::new(var(&freq_shared) >> soft_saw());
            osc.set_sample_rate(sr as f64);
            // NOTE: Don't call reset() here - let FunDSP handle initialization
            // Calling reset() immediately after construction can cause startup artifacts
            oscillators.push(osc);

            // Precompute stereo pan coefficients (equal-power law)
            let pan_pos = if voice_count > 1 {
                -1.0 + 2.0 * (i as f32 / (voice_count - 1) as f32)
            } else {
                0.0 // Center for single voice
            };
            let pan_left = ((1.0 - pan_pos) / 2.0).sqrt();
            let pan_right = ((1.0 + pan_pos) / 2.0).sqrt();
            pan_coeffs.push((pan_left, pan_right));
        }

        // Our Shared handles for the control thread
        let freq_handle = shared::shared(freq);
        let voices_handle = shared::shared(voices);
        let spread_handle = shared::shared(spread);
        let gain_handle = shared::shared(gain);

        let handles = vec![
            ("freq".into(), freq_handle.clone()),
            ("voices".into(), voices_handle.clone()),
            ("spread".into(), spread_handle.clone()),
            ("gain".into(), gain_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let mut base_values = HashMap::new();
        base_values.insert("freq".into(), freq);
        base_values.insert("voices".into(), voices);
        base_values.insert("spread".into(), spread);
        base_values.insert("gain".into(), gain);

        let cell = Self {
            oscillators,
            freq_shareds,
            freq_handle,
            voices_handle,
            spread_handle,
            gain_handle,
            voice_count,
            cached_freq: freq,
            cached_spread: spread,
            first_tick: true,  // Force update on first tick
            detune_ratios,
            pan_coeffs,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for SawBankCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Read params
        let freq = self.freq_handle.value().clamp(20.0, 2000.0);
        let spread = self.spread_handle.value().clamp(0.0, 100.0);
        let gain = self.gain_handle.value().clamp(0.0, 1.0);

        // Detect changes (use larger threshold to avoid update jitter)
        let freq_changed = self.first_tick || (freq - self.cached_freq).abs() > 0.1;
        let spread_changed = self.first_tick || (spread - self.cached_spread).abs() > 0.01;

        if self.first_tick {
            self.first_tick = false;
        }

        // Recompute detune ratios if spread changed
        if spread_changed {
            self.cached_spread = spread;
            for i in 0..self.voice_count {
                let voice_idx_centered = (i as f32) - ((self.voice_count - 1) as f32 / 2.0);
                let cents_offset = if self.voice_count > 1 {
                    (spread / ((self.voice_count - 1) as f32 / 2.0)) * voice_idx_centered
                } else {
                    0.0
                };
                self.detune_ratios[i] = fast_pow2(cents_offset / 1200.0);
            }
        }

        // Update FunDSP shareds ONLY when freq or spread actually changes
        // Prevents zipper noise and phase discontinuities from 44kHz updates
        if freq_changed || spread_changed {
            self.cached_freq = freq;
            for i in 0..self.voice_count {
                let voice_freq = freq * self.detune_ratios[i];
                self.freq_shareds[i].set_value(voice_freq);
            }
        }

        // Gain normalization: 1/√N + 0.5× safety headroom to prevent clipping
        // Phase-aligned oscillators can cause constructive interference peaks.
        // Pattern: osc_cell uses 0.5× safety factor (osc_cell.rs:194)
        let normalization = 1.0 / (self.voice_count as f32).sqrt();
        let scaled_gain = gain * normalization * 0.5;

        // Mix oscillators to stereo with panning
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        for i in 0..self.voice_count {
            let sample = self.oscillators[i].get_mono();
            let (pan_left, pan_right) = self.pan_coeffs[i];
            left += sample * pan_left * scaled_gain;
            right += sample * pan_right * scaled_gain;
        }

        // Stereo output
        output[0] = left;
        output[1] = right;

        // Accumulate RMS and peak for analysis
        let sample_energy = left * left + right * right;
        self.rms_acc += sample_energy;
        self.peak = self.peak.max(left.abs()).max(right.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {} // NoteOn/NoteOff not relevant for drones
        }
    }

    fn analysis(&self) -> DspAnalysis {
        if self.sample_count == 0 {
            return DspAnalysis { rms: 0.0, peak: 0.0 };
        }

        let rms = (self.rms_acc / self.sample_count as f32 / 2.0).sqrt(); // /2 for stereo
        DspAnalysis {
            rms,
            peak: self.peak,
        }
    }

    fn output_channels(&self) -> usize { 2 } // Stereo audio

    fn reset(&mut self) {
        for osc in &mut self.oscillators {
            osc.reset();
        }
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "saw_bank_cell" }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna(freq: f32, voices: f32, spread: f32, gain: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), freq);
        params.insert("voices".into(), voices);
        params.insert("spread".into(), spread);
        params.insert("gain".into(), gain);

        CellDna {
            cell_type: "saw_bank_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn saw_bank_produces_audio() {
        // Non-zero stereo output over 1000 samples
        let dna = make_dna(110.0, 4.0, 10.0, 0.5);
        let (mut cell, _handles) = SawBankCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut has_left_signal = false;
        let mut has_right_signal = false;

        for _ in 0..1000 {
            cell.tick(&[], &mut output);
            if output[0].abs() > 0.001 {
                has_left_signal = true;
            }
            if output[1].abs() > 0.001 {
                has_right_signal = true;
            }
        }

        assert!(has_left_signal, "Left channel should have signal");
        assert!(has_right_signal, "Right channel should have signal");
    }

    #[test]
    fn saw_bank_voices_scale() {
        // 2 voices (hard L/R) have HIGHER variance than 6 voices (evenly spread)
        let dna_few = make_dna(110.0, 2.0, 10.0, 0.5);
        let dna_many = make_dna(110.0, 6.0, 10.0, 0.5);

        let (mut cell_few, _) = SawBankCell::new(&dna_few, SR).unwrap();
        let (mut cell_many, _) = SawBankCell::new(&dna_many, SR).unwrap();

        let mut output_few = [0.0f32; 2];
        let mut output_many = [0.0f32; 2];

        // Collect L-R differences
        let mut variance_few = 0.0f32;
        let mut variance_many = 0.0f32;

        for _ in 0..1000 {
            cell_few.tick(&[], &mut output_few);
            cell_many.tick(&[], &mut output_many);

            let diff_few = (output_few[0] - output_few[1]).abs();
            let diff_many = (output_many[0] - output_many[1]).abs();

            variance_few += diff_few;
            variance_many += diff_many;
        }

        // 2 voices (hard L/R pan) → higher L-R variance than 6 voices (spread evenly)
        assert!(
            variance_few > variance_many * 1.1,
            "2 voices should have higher stereo variance than 6 voices: {} vs {}",
            variance_few,
            variance_many
        );
    }

    #[test]
    fn saw_bank_spread_widens() {
        // spread=50 should have more L-R variance than spread=0
        let dna_narrow = make_dna(110.0, 4.0, 0.0, 0.5);
        let dna_wide = make_dna(110.0, 4.0, 50.0, 0.5);

        let (mut cell_narrow, _) = SawBankCell::new(&dna_narrow, SR).unwrap();
        let (mut cell_wide, _) = SawBankCell::new(&dna_wide, SR).unwrap();

        let mut output_narrow = [0.0f32; 2];
        let mut output_wide = [0.0f32; 2];

        // Collect L-R differences
        let mut variance_narrow = 0.0f32;
        let mut variance_wide = 0.0f32;

        for _ in 0..1000 {
            cell_narrow.tick(&[], &mut output_narrow);
            cell_wide.tick(&[], &mut output_wide);

            let diff_narrow = (output_narrow[0] - output_narrow[1]).abs();
            let diff_wide = (output_wide[0] - output_wide[1]).abs();

            variance_narrow += diff_narrow;
            variance_wide += diff_wide;
        }

        // Wider spread → more detuning → higher stereo variance
        assert!(
            variance_wide > variance_narrow * 1.2,
            "spread=50 should have higher variance than spread=0: {} vs {}",
            variance_wide,
            variance_narrow
        );
    }

    #[test]
    fn saw_bank_gain_normalized() {
        // 8 voices vs 2 voices: with 1/√N normalization, ratio should be √(8/2) = 2
        // Because: 8 voices × (1/√8) = √8, 2 voices × (1/√2) = √2, ratio = √8/√2 = 2
        let dna_few = make_dna(110.0, 2.0, 10.0, 0.5);
        let dna_many = make_dna(110.0, 8.0, 10.0, 0.5);

        let (mut cell_few, _) = SawBankCell::new(&dna_few, SR).unwrap();
        let (mut cell_many, _) = SawBankCell::new(&dna_many, SR).unwrap();

        let mut output_few = [0.0f32; 2];
        let mut output_many = [0.0f32; 2];

        // Measure RMS over 1000 samples
        let mut rms_few = 0.0f32;
        let mut rms_many = 0.0f32;

        for _ in 0..1000 {
            cell_few.tick(&[], &mut output_few);
            cell_many.tick(&[], &mut output_many);

            rms_few += output_few[0] * output_few[0] + output_few[1] * output_few[1];
            rms_many += output_many[0] * output_many[0] + output_many[1] * output_many[1];
        }

        rms_few = (rms_few / 2000.0).sqrt(); // /2000 = /1000 samples /2 channels
        rms_many = (rms_many / 2000.0).sqrt();

        // Expected ratio: √(8/2) = 2.0 (with some variance due to phase/detune)
        let ratio = rms_many / rms_few;
        let expected_ratio = (8.0f32 / 2.0).sqrt();
        assert!(
            (ratio - expected_ratio).abs() < 0.6,
            "8 voices vs 2 voices should have ratio ~{}, got {}",
            expected_ratio,
            ratio
        );
    }

    #[test]
    fn saw_bank_diagnostic_trace() {
        // DIAGNOSTIC: Trace actual sample values at audible frequency (440 Hz)
        let dna = make_dna(440.0, 2.0, 0.0, 1.0);  // 440 Hz, 2 voices, no spread, full gain
        let (mut cell, handles) = SawBankCell::new(&dna, SR).unwrap();

        // Check what frequency our handle thinks it has
        let freq_handle = &handles.iter().find(|(n, _)| n == "freq").unwrap().1;
        println!("\n=== DIAGNOSTIC: saw_bank_cell at 440 Hz ===");
        println!("Config: freq=440Hz, voices=2, spread=0, gain=1.0");
        println!("Handle freq value: {}", freq_handle.value());

        // (Internal state inspection via downcast removed — DspCell doesn't extend Any)
        println!("Expected: Oscillating samples\n");

        let mut output = [0.0f32; 2];
        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;

        for i in 0..50 {
            cell.tick(&[], &mut output);
            min_val = min_val.min(output[0]);
            max_val = max_val.max(output[0]);
            if i < 20 {
                println!("Sample {}: L={:.6}, R={:.6}", i, output[0], output[1]);
            }
        }

        println!("\nRange over 50 samples: {:.6} to {:.6} (span={:.6})",
                 min_val, max_val, max_val - min_val);

        // At 440 Hz with SR=44100, we complete 440/44100 cycles per sample
        // In 50 samples we should see significant oscillation
        let span = max_val - min_val;
        assert!(
            span > 0.01,
            "FAILED: No oscillation! span={}, expected >0.01",
            span
        );
        println!("PASSED: Oscillation detected");
    }

    #[test]
    fn saw_bank_freq_tracks() {
        // Changing freq should shift all voices proportionally
        let dna = make_dna(110.0, 4.0, 10.0, 0.5);
        let (mut cell, handles) = SawBankCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Sample at 110 Hz
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }
        let sample_110 = output[0];

        // Change frequency to 220 Hz
        let freq_handle = &handles.iter().find(|(n, _)| n == "freq").unwrap().1;
        freq_handle.set(220.0);

        // Sample at 220 Hz
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }
        let sample_220 = output[0];

        // Higher frequency → different waveform samples (not equal)
        assert!(
            (sample_110 - sample_220).abs() > 0.01,
            "Freq change should affect output, got {} vs {}",
            sample_110,
            sample_220
        );
    }
}
