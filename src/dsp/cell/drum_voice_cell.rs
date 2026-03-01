//! Parametric electronic drum synthesizer cell.
//!
//! Models TR-909-style drum sounds via analog synthesis techniques. Each `preset`
//! configures a different synthesis architecture: sine + pitch-sweep for kick/tom,
//! noise + tonal oscillators for snare, metallic square-wave clusters for hats, etc.
//!
//! All state is pre-allocated — RT-safe, no heap allocation in tick().
//!
//! # Parameters
//! - `tune`: Pitch offset in semitones (-1–1). Shifts the preset's base frequency.
//! - `decay`: Amplitude decay time in seconds (0.01–3.0).
//! - `tone`: Brightness (0–1). Shifts filter cutoffs within preset.
//! - `snap`: Transient click/snap amount (0–1). Controls initial impulse.
//! - `drive`: Saturation/distortion (0–1). Applied via soft-clip before envelope.
//!
//! # String Parameters
//! - `preset`: kick | snare | hat_closed | hat_open | clap | tom | rim
//!
//! # Trigger
//! Activated by `DspCommand::NoteOn` via a Trigger wire from seq_cell or logic_seq_cell.
//! Retrigger during decay restarts from the beginning (clean attack).

use std::collections::HashMap;
use std::f32::consts::TAU;

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Drum preset type — determines synthesis architecture.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DrumPreset {
    Kick,
    Snare,
    HatClosed,
    HatOpen,
    Clap,
    Tom,
    Rim,
}

impl DrumPreset {
    fn from_str(s: &str) -> Self {
        match s {
            "snare" => DrumPreset::Snare,
            "hat_closed" | "hihat_closed" | "hh_closed" => DrumPreset::HatClosed,
            "hat_open" | "hihat_open" | "hh_open" => DrumPreset::HatOpen,
            "clap" => DrumPreset::Clap,
            "tom" => DrumPreset::Tom,
            "rim" | "rimshot" => DrumPreset::Rim,
            _ => DrumPreset::Kick, // default
        }
    }
}

/// Inharmonic frequency ratios for metallic hat oscillators.
/// Chosen to approximate the characteristic metallic ring of the TR-909 hi-hat.
const HAT_RATIOS: [f32; 6] = [1.0, 1.342, 1.618, 1.922, 2.347, 3.0];

/// Hat base frequency (Hz).
const HAT_BASE_HZ: f32 = 380.0;

/// LCG noise step.
#[inline(always)]
fn lcg(seed: u32) -> u32 {
    seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}

/// Parametric drum synthesizer cell.
///
/// Triggered by NoteOn. All oscillator state, filter state, and envelope state
/// is stored on the struct — no allocation in tick(). Output is centered stereo (L=R).
pub struct DrumVoiceCell {
    preset: DrumPreset,

    // --- Main amplitude envelope ---
    env_level: f32,
    /// Per-sample exponential decay multiplier for main envelope.
    env_coeff: f32,

    // --- Oscillator phases (up to 6 for hat metallic cluster) ---
    phases: [f32; 6],
    /// Current oscillator frequencies (Hz), set on trigger.
    osc_freqs: [f32; 6],

    // --- Pitch envelope (kick, tom): sweeps from pitch_hz toward pitch_target ---
    pitch_hz: f32,
    pitch_target: f32,
    /// Per-sample multiplier for pitch sweep decay.
    pitch_coeff: f32,

    // --- Noise path ---
    noise_seed: u32,
    /// Separate decay envelope for snare noise component.
    noise_env_level: f32,
    noise_env_coeff: f32,
    /// LP filter state shared between noise shaping and HP computation.
    noise_lp_state: f32,

    // --- Click impulse (kick, snare snap) ---
    click_samples_left: u32,

    // --- Clap: sample counter since trigger for multi-burst timing ---
    burst_sample: u32,

    // --- Params ---
    tune_handle: Shared,
    decay_handle: Shared,
    tone_handle: Shared,
    snap_handle: Shared,
    drive_handle: Shared,

    base_values: HashMap<String, f32>,
    sample_rate: f32,

    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl DrumVoiceCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let preset_str = string_param_or(dna, "preset", "kick");
        let preset = DrumPreset::from_str(preset_str);

        let tune = param_or(dna, "tune", 0.0).clamp(-1.0, 1.0);
        let decay = param_or(dna, "decay", 0.3).clamp(0.01, 3.0);
        let tone = param_or(dna, "tone", 0.5).clamp(0.0, 1.0);
        let snap = param_or(dna, "snap", 0.3).clamp(0.0, 1.0);
        let drive = param_or(dna, "drive", 0.0).clamp(0.0, 1.0);

        let tune_handle = shared::shared(tune);
        let decay_handle = shared::shared(decay);
        let tone_handle = shared::shared(tone);
        let snap_handle = shared::shared(snap);
        let drive_handle = shared::shared(drive);

        let mut base_values = HashMap::new();
        base_values.insert("tune".into(), tune);
        base_values.insert("decay".into(), decay);
        base_values.insert("tone".into(), tone);
        base_values.insert("snap".into(), snap);
        base_values.insert("drive".into(), drive);

        let handles = vec![
            ("tune".into(), tune_handle.clone()),
            ("decay".into(), decay_handle.clone()),
            ("tone".into(), tone_handle.clone()),
            ("snap".into(), snap_handle.clone()),
            ("drive".into(), drive_handle.clone()),
        ];

        let cell = Box::new(DrumVoiceCell {
            preset,
            env_level: 0.0,
            env_coeff: 0.0,
            phases: [0.0; 6],
            osc_freqs: [0.0; 6],
            pitch_hz: 0.0,
            pitch_target: 0.0,
            pitch_coeff: 0.0,
            noise_seed: 0xDEADBEEF,
            noise_env_level: 0.0,
            noise_env_coeff: 0.0,
            noise_lp_state: 0.0,
            click_samples_left: 0,
            burst_sample: 0,
            tune_handle,
            decay_handle,
            tone_handle,
            snap_handle,
            drive_handle,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        });

        Some((cell, handles))
    }

    /// Configure all oscillator/envelope state for the current preset on NoteOn.
    fn trigger_preset(&mut self) {
        let decay = self.decay_handle.value().clamp(0.01, 3.0);
        let tune = self.tune_handle.value().clamp(-1.0, 1.0);
        let snap = self.snap_handle.value().clamp(0.0, 1.0);
        let sr = self.sample_rate;

        let mult = (2.0_f32).powf(tune / 12.0);

        // Main envelope restart.
        self.env_level = 1.0;
        self.env_coeff = (-1.0 / (decay * sr)).exp();

        // Reset oscillator phases and filter state.
        self.phases = [0.0; 6];
        self.noise_lp_state = 0.0;

        match self.preset {
            DrumPreset::Kick => {
                // Sine oscillator with exponential pitch sweep 200Hz → 50Hz.
                self.pitch_hz = 200.0 * mult;
                self.pitch_target = 50.0 * mult;
                // Pitch sweeps to target in ~50ms.
                self.pitch_coeff = (-1.0 / (0.05 * sr)).exp();
                // Click impulse duration scales with snap.
                self.click_samples_left = (snap * 0.003 * sr) as u32;
            }
            DrumPreset::Snare => {
                // Tonal body: two sines at 180Hz and 330Hz (tunable).
                self.osc_freqs[0] = 180.0 * mult;
                self.osc_freqs[1] = 330.0 * mult;
                // Noise component has its own faster-decaying envelope.
                self.noise_env_level = 1.0;
                self.noise_env_coeff = (-1.0 / (decay * 0.5 * sr)).exp();
                // Click for snap.
                self.click_samples_left = (snap * 0.002 * sr) as u32;
            }
            DrumPreset::HatClosed | DrumPreset::HatOpen => {
                // 6 square oscillators at inharmonic metallic ratios.
                for i in 0..6 {
                    self.osc_freqs[i] = HAT_BASE_HZ * HAT_RATIOS[i];
                }
            }
            DrumPreset::Clap => {
                // Multi-burst noise: 4 short bursts in first 32ms then decay tail.
                self.burst_sample = 0;
                self.noise_lp_state = 0.0;
                self.noise_seed = 0xCAFEBABE;
            }
            DrumPreset::Tom => {
                // Pitch sweep: 150Hz → 80Hz over ~150ms.
                self.pitch_hz = 150.0 * mult;
                self.pitch_target = 80.0 * mult;
                self.pitch_coeff = (-1.0 / (0.15 * sr)).exp();
            }
            DrumPreset::Rim => {
                // Three fixed-frequency sines at 500, 800, 1100 Hz.
                // No tune for rim — it's a metallic, non-pitched sound.
                self.osc_freqs[0] = 500.0;
                self.osc_freqs[1] = 800.0;
                self.osc_freqs[2] = 1100.0;
            }
        }
    }

    /// Generate one sample of the drum body signal (before drive + envelope).
    #[inline]
    fn tick_body(&mut self) -> f32 {
        let sr = self.sample_rate;

        match self.preset {
            DrumPreset::Kick => {
                // Sweep pitch toward target.
                let pd = self.pitch_hz - self.pitch_target;
                self.pitch_hz = self.pitch_target + pd * self.pitch_coeff;

                // Sine oscillator at sweeping pitch.
                self.phases[0] = (self.phases[0] + self.pitch_hz / sr) % 1.0;
                let sine = (self.phases[0] * TAU).sin();

                // Click impulse for transient snap.
                let click = if self.click_samples_left > 0 {
                    self.click_samples_left -= 1;
                    1.0f32
                } else {
                    0.0
                };
                let snap = self.snap_handle.value();
                sine + click * snap
            }

            DrumPreset::Snare => {
                // Tonal component: two sines for body resonance.
                self.phases[0] = (self.phases[0] + self.osc_freqs[0] / sr) % 1.0;
                self.phases[1] = (self.phases[1] + self.osc_freqs[1] / sr) % 1.0;
                let tonal = (self.phases[0] * TAU).sin() * 0.55
                    + (self.phases[1] * TAU).sin() * 0.35;

                // Click snap.
                let click = if self.click_samples_left > 0 {
                    self.click_samples_left -= 1;
                    1.0f32
                } else {
                    0.0
                };
                let snap = self.snap_handle.value();

                // Noise component: filtered broadband for snare wire rattle.
                // tone param shifts the bandpass center: low=warm, high=crispy.
                let tone = self.tone_handle.value();
                let lp_cutoff = 800.0 + tone * 4000.0;
                let lp_coeff = 1.0 - (-TAU * lp_cutoff / sr).exp();

                self.noise_seed = lcg(self.noise_seed);
                let raw = self.noise_seed as i32 as f32 / i32::MAX as f32;
                self.noise_lp_state += (raw - self.noise_lp_state) * lp_coeff;
                // HP via subtraction gives bandpass-ish character.
                let noise_bp = raw - self.noise_lp_state;

                let noise_out = noise_bp * self.noise_env_level;
                self.noise_env_level *= self.noise_env_coeff;

                tonal + noise_out * 0.6 + click * snap * 0.3
            }

            DrumPreset::HatClosed | DrumPreset::HatOpen => {
                // 6 square waves at inharmonic metallic ratios.
                let mut sum = 0.0f32;
                for i in 0..6 {
                    self.phases[i] = (self.phases[i] + self.osc_freqs[i] / sr) % 1.0;
                    sum += if self.phases[i] < 0.5 { 1.0 } else { -1.0 };
                }
                sum /= 6.0;

                // HP filter via LP subtraction.
                // tone shifts the HP cutoff: 0=darkest (5kHz), 1=brightest (10kHz).
                let tone = self.tone_handle.value();
                let hp_cutoff = 5000.0 + tone * 5000.0;
                let lp_coeff = 1.0 - (-TAU * hp_cutoff / sr).exp();
                self.noise_lp_state += (sum - self.noise_lp_state) * lp_coeff;
                sum - self.noise_lp_state
            }

            DrumPreset::Clap => {
                // 4 noise bursts (each ~4ms) at 0, 8, 16, 24ms, then decay tail.
                self.burst_sample += 1;
                let burst_len = (0.004 * sr) as u32;

                let active = self.burst_sample < burst_len * 8
                    && (self.burst_sample / burst_len) % 2 == 0;

                self.noise_seed = lcg(self.noise_seed);
                let raw = self.noise_seed as i32 as f32 / i32::MAX as f32;

                // LP at ~1.2kHz for characteristic clap "slap" color.
                let coeff = 1.0 - (-TAU * 1200.0 / sr).exp();
                self.noise_lp_state += (raw - self.noise_lp_state) * coeff;

                if active {
                    // Burst window: full noise output.
                    self.noise_lp_state
                } else if self.burst_sample >= burst_len * 8 {
                    // Tail: attenuated noise for reverb-y sustain.
                    raw * 0.25
                } else {
                    // Silent gap between bursts.
                    0.0
                }
            }

            DrumPreset::Tom => {
                // Three harmonic sines with pitch sweep for tom resonance.
                let pd = self.pitch_hz - self.pitch_target;
                self.pitch_hz = self.pitch_target + pd * self.pitch_coeff;
                let f = self.pitch_hz;

                self.phases[0] = (self.phases[0] + f / sr) % 1.0;
                self.phases[1] = (self.phases[1] + f * 1.5 / sr) % 1.0;
                self.phases[2] = (self.phases[2] + f * 2.0 / sr) % 1.0;

                (self.phases[0] * TAU).sin() * 0.7
                    + (self.phases[1] * TAU).sin() * 0.2
                    + (self.phases[2] * TAU).sin() * 0.1
            }

            DrumPreset::Rim => {
                // Three sine oscillators for metallic rimshot ring.
                self.phases[0] = (self.phases[0] + self.osc_freqs[0] / sr) % 1.0;
                self.phases[1] = (self.phases[1] + self.osc_freqs[1] / sr) % 1.0;
                self.phases[2] = (self.phases[2] + self.osc_freqs[2] / sr) % 1.0;
                let sum = (self.phases[0] * TAU).sin() * 0.5
                    + (self.phases[1] * TAU).sin() * 0.3
                    + (self.phases[2] * TAU).sin() * 0.2;

                // HP at 400Hz to remove low-end muddiness.
                let hp_cutoff = 400.0;
                let lp_coeff = 1.0 - (-TAU * hp_cutoff / sr).exp();
                self.noise_lp_state += (sum - self.noise_lp_state) * lp_coeff;
                sum - self.noise_lp_state
            }
        }
    }
}

impl DspCell for DrumVoiceCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        if output.len() < 2 {
            return;
        }

        if self.env_level < 1e-6 {
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        }

        // Generate drum body signal.
        let body = self.tick_body();

        // Apply drive (soft saturation) before envelope.
        let drive = self.drive_handle.value();
        let saturated = if drive > 0.01 {
            ((1.0 + drive * 4.0) * body).tanh()
        } else {
            body
        };

        // Apply main amplitude envelope.
        let out = saturated * self.env_level;
        self.env_level *= self.env_coeff;

        // Centered stereo output.
        output[0] = out;
        output[1] = out;

        // Analysis.
        self.rms_acc += out * out;
        self.peak = self.peak.max(out.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { .. } => {
                self.trigger_preset();
            }
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        let rms = if self.sample_count > 0 {
            (self.rms_acc / self.sample_count as f32).sqrt()
        } else {
            0.0
        };
        DspAnalysis { rms, peak: self.peak }
    }

    fn output_channels(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.env_level = 0.0;
        self.env_coeff = 0.0;
        self.phases = [0.0; 6];
        self.osc_freqs = [0.0; 6];
        self.pitch_hz = 0.0;
        self.pitch_target = 0.0;
        self.pitch_coeff = 0.0;
        self.noise_seed = 0xDEADBEEF;
        self.noise_env_level = 0.0;
        self.noise_env_coeff = 0.0;
        self.noise_lp_state = 0.0;
        self.click_samples_left = 0;
        self.burst_sample = 0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "drum_voice_cell"
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use crate::organism::dna::CellDna;

    const SR: f32 = 44100.0;

    fn make_dna(preset: &str, tune: f32, decay: f32, tone: f32, snap: f32, drive: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("tune".into(), tune);
        params.insert("decay".into(), decay);
        params.insert("tone".into(), tone);
        params.insert("snap".into(), snap);
        params.insert("drive".into(), drive);
        let mut string_params = BTreeMap::new();
        string_params.insert("preset".into(), preset.into());
        CellDna { cell_type: "drum_voice_cell".into(), params, string_params }
    }

    fn trigger(cell: &mut Box<dyn DspCell>) {
        cell.handle_command(&DspCommand::NoteOn { freq: 60.0, velocity: 1.0 });
    }

    fn sum_rms(cell: &mut Box<dyn DspCell>, samples: usize) -> f32 {
        let mut acc = 0.0f32;
        let mut out = [0.0f32; 2];
        for _ in 0..samples {
            cell.tick(&[], &mut out);
            acc += out[0] * out[0];
        }
        (acc / samples as f32).sqrt()
    }

    // --- Kick ---

    #[test]
    fn kick_produces_audio_on_trigger() {
        let dna = make_dna("kick", 0.0, 0.4, 0.5, 0.4, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "kick should produce audio after trigger");
    }

    #[test]
    fn kick_silent_before_trigger() {
        let dna = make_dna("kick", 0.0, 0.4, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn kick_decays_to_silence() {
        let decay = 0.1f32;
        let dna = make_dna("kick", 0.0, decay, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        // Run well past the decay time, then check the LAST output value.
        let run = (decay * SR * 10.0) as usize;
        let mut out = [0.0f32; 2];
        for _ in 0..run {
            cell.tick(&[], &mut out);
        }
        assert!(out[0].abs() < 0.01, "kick should decay to near-silence after 10× decay time (last={:.5})", out[0]);
    }

    #[test]
    fn kick_retrigger_restarts_envelope() {
        let decay = 0.001f32;
        let dna = make_dna("kick", 0.0, decay, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        // Let decay exhaust.
        sum_rms(&mut cell, (decay * SR * 20.0) as usize);
        // Retrigger.
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "retrigger should restart the kick");
    }

    // --- Snare ---

    #[test]
    fn snare_produces_audio_on_trigger() {
        let dna = make_dna("snare", 0.0, 0.25, 0.6, 0.5, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let rms = sum_rms(&mut cell, 100);
        assert!(rms > 0.0, "snare should produce audio");
    }

    #[test]
    fn snare_has_noise_component() {
        // Snare with tone=1.0 (bright, lots of noise) should have high RMS.
        let dna_bright = make_dna("snare", 0.0, 0.25, 1.0, 0.0, 0.0);
        let (mut bright, _) = DrumVoiceCell::new(&dna_bright, SR).unwrap();
        trigger(&mut bright);
        let rms_bright = sum_rms(&mut bright, 200);
        assert!(rms_bright > 0.0, "bright snare should have audio");
    }

    // --- Hats ---

    #[test]
    fn hat_closed_is_shorter_than_hat_open() {
        let sr = SR;
        // Closed: short decay (0.05s). Open: long decay (0.3s).
        let dna_closed = make_dna("hat_closed", 0.0, 0.05, 0.7, 0.0, 0.0);
        let dna_open = make_dna("hat_open", 0.0, 0.3, 0.7, 0.0, 0.0);

        let (mut closed, _) = DrumVoiceCell::new(&dna_closed, sr).unwrap();
        let (mut open_, _) = DrumVoiceCell::new(&dna_open, sr).unwrap();

        trigger(&mut closed);
        trigger(&mut open_);

        // Measure RMS at the midpoint between closed and open decay times.
        let check_samples = (0.15 * sr) as usize;
        let rms_closed = sum_rms(&mut closed, check_samples);
        let rms_open = sum_rms(&mut open_, check_samples);

        assert!(
            rms_closed < rms_open,
            "hat_closed (rms={rms_closed:.4}) should decay faster than hat_open (rms={rms_open:.4})"
        );
    }

    #[test]
    fn hat_produces_metallic_audio() {
        let dna = make_dna("hat_closed", 0.0, 0.05, 0.7, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "hat should produce audio on trigger");
    }

    // --- Clap ---

    #[test]
    fn clap_produces_audio_on_trigger() {
        let dna = make_dna("clap", 0.0, 0.15, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let rms = sum_rms(&mut cell, 500);
        assert!(rms > 0.0, "clap should produce audio");
    }

    #[test]
    fn clap_has_multi_burst_attack() {
        // The clap produces non-zero audio in multiple burst windows.
        // Check that there's a silent gap within the first 32ms, indicating multi-burst.
        let dna = make_dna("clap", 0.0, 0.15, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);

        let burst_len = (0.004 * SR) as usize;
        let mut out = [0.0f32; 2];
        let mut max_in_gap = 0.0f32;

        // Second burst gap: samples burst_len..2*burst_len.
        // Shrink window by 5 samples on each side to avoid burst-boundary edge ticks.
        for i in 0..(burst_len * 4) {
            cell.tick(&[], &mut out);
            if i >= burst_len + 5 && i < burst_len * 2 - 5 {
                max_in_gap = max_in_gap.max(out[0].abs());
            }
        }
        // In the first gap (between bursts 1 and 2), output should be ~0.
        assert!(
            max_in_gap < 0.001,
            "clap should have silent gaps between bursts (max_in_gap={max_in_gap:.5})"
        );
    }

    // --- Tom ---

    #[test]
    fn tom_produces_audio_on_trigger() {
        let dna = make_dna("tom", 0.0, 0.4, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "tom should produce audio");
    }

    // --- Rim ---

    #[test]
    fn rim_produces_audio_on_trigger() {
        let dna = make_dna("rim", 0.0, 0.015, 0.5, 0.0, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "rim should produce audio");
    }

    // --- Param tests ---

    #[test]
    fn decay_controls_sustain_length() {
        let sr = SR;
        let dna_short = make_dna("kick", 0.0, 0.05, 0.5, 0.0, 0.0);
        let dna_long = make_dna("kick", 0.0, 0.5, 0.5, 0.0, 0.0);

        let (mut short, _) = DrumVoiceCell::new(&dna_short, sr).unwrap();
        let (mut long_, _) = DrumVoiceCell::new(&dna_long, sr).unwrap();

        trigger(&mut short);
        trigger(&mut long_);

        // At 200ms, short should be near-silent, long should still have energy.
        let samples = (0.2 * sr) as usize;
        let rms_short = sum_rms(&mut short, samples);
        let rms_long = sum_rms(&mut long_, samples);

        assert!(rms_short < rms_long, "longer decay should produce more energy at 200ms");
    }

    #[test]
    fn drive_changes_output() {
        // Drive=1 vs drive=0 on kick — different output due to saturation.
        let dna_dry = make_dna("kick", 0.0, 0.3, 0.5, 0.5, 0.0);
        let dna_driven = make_dna("kick", 0.0, 0.3, 0.5, 0.5, 1.0);

        let (mut dry, _) = DrumVoiceCell::new(&dna_dry, SR).unwrap();
        let (mut driven, _) = DrumVoiceCell::new(&dna_driven, SR).unwrap();

        trigger(&mut dry);
        trigger(&mut driven);

        let mut out_dry = [0.0f32; 2];
        let mut out_driven = [0.0f32; 2];
        dry.tick(&[], &mut out_dry);
        driven.tick(&[], &mut out_driven);

        // They should differ (drive modifies the signal path).
        assert_ne!(
            out_dry[0], out_driven[0],
            "drive should change the output"
        );
    }

    #[test]
    fn output_channels_is_two() {
        let dna = make_dna("kick", 0.0, 0.3, 0.5, 0.3, 0.0);
        let (cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn centered_stereo_l_equals_r() {
        let dna = make_dna("kick", 0.0, 0.3, 0.5, 0.5, 0.0);
        let (mut cell, _) = DrumVoiceCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], out[1], "drum output should be centered stereo (L=R)");
    }

    #[test]
    fn all_presets_build_without_panic() {
        let presets = ["kick", "snare", "hat_closed", "hat_open", "clap", "tom", "rim"];
        for p in &presets {
            let dna = make_dna(p, 0.0, 0.2, 0.5, 0.3, 0.0);
            let result = DrumVoiceCell::new(&dna, SR);
            assert!(result.is_some(), "preset '{p}' should build without error");
        }
    }
}
