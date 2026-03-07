//! Resonant membrane percussion cell for tabla synthesis.
//!
//! Models a struck membrane through three parallel damped sinusoidal resonators
//! at harmonically-related frequencies. On trigger (NoteOn), all resonators
//! restart from phase zero and a shared exponential decay envelope fades them out.
//!
//! The harmonic ratios between resonators depend on `tension`:
//! - Low tension (0.0): inharmonic bayan ratios (1.0, 1.59, 2.14) — deep, beating
//! - High tension (1.0): near-harmonic dayan ratios (1.0, 2.0, 3.0) — bright, ringing
//! Intermediate tension blends between the two.
//!
//! # Parameters
//! - `membrane_freq`: Fundamental frequency (40–800 Hz)
//! - `tension`: Membrane tension (0–1). 0=bayan inharmonic, 1=dayan harmonic
//! - `decay`: Envelope decay time in seconds (0.01–3.0)
//! - `noise_mix`: Broadband noise component (0–1). Adds chik/slap character
//! - `tone`: Brightness (0–1). 0=dark/warm (200 Hz rolloff), 1=full brightness
//!
//! # Trigger
//! Activated by `DspCommand::NoteOn` via a Trigger wire from logic_seq_cell.
//! Retrigger during decay resets envelope to full amplitude and phases to zero.
//!
//! # Wire Graph Patterns
//! - `logic_seq_cell --[Trigger]--> strike_voice_cell`
//! - `strike_voice_cell --[Audio]--> mixer_cell`

use fundsp::hacker32::*;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};

/// Valid parameter ranges for strike_voice_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("membrane_freq", 20.0, 2000.0),
    ("tension", 0.0, 1.0),
    ("decay", 0.001, 10.0),
    ("noise_mix", 0.0, 1.0),
    ("tone", 0.0, 1.0),
];
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Resonant membrane percussion synthesis via three damped sinusoidal resonators.
///
/// Triggered by NoteOn commands. Resonators reset phase on each strike for a
/// clean attack transient. The R channel is slightly detuned from L for natural
/// stereo spread.
pub struct StrikeVoiceCell {
    /// Shared exponential decay envelope across all resonators.
    env_level: f32,

    /// Phase accumulators for the three resonators (left channel).
    phases_l: [f32; 3],
    /// Phase accumulators for the three resonators (right channel, slightly detuned).
    phases_r: [f32; 3],

    /// Tone (brightness) lowpass filter — shapes the final resonator output.
    tone_filter_l: Box<dyn AudioUnit>,
    tone_filter_r: Box<dyn AudioUnit>,

    /// LCG noise generator — RT-safe, no heap allocation.
    noise_seed: u32,

    // Param handles (shared between audio and control threads)
    membrane_freq_handle: Shared,
    tension_handle: Shared,
    decay_handle: Shared,
    noise_mix_handle: Shared,
    tone_handle: Shared,

    base_values: HashMap<String, f32>,
    sample_rate: f32,

    // Analysis accumulators
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl StrikeVoiceCell {
    /// Build a strike_voice_cell from DNA.
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let membrane_freq = param_or(dna, "membrane_freq", 200.0);
        let tension       = param_or(dna, "tension", 0.5);
        let decay         = param_or(dna, "decay", 0.4);
        let noise_mix     = param_or(dna, "noise_mix", 0.2);
        let tone          = param_or(dna, "tone", 0.5);

        let membrane_freq_handle = shared::shared(membrane_freq);
        let tension_handle       = shared::shared(tension);
        let decay_handle         = shared::shared(decay);
        let noise_mix_handle     = shared::shared(noise_mix);
        let tone_handle          = shared::shared(tone);

        let handles = vec![
            ("membrane_freq".into(), membrane_freq_handle.clone()),
            ("tension".into(),       tension_handle.clone()),
            ("decay".into(),         decay_handle.clone()),
            ("noise_mix".into(),     noise_mix_handle.clone()),
            ("tone".into(),          tone_handle.clone()),
        ];

        let base_values = [
            ("membrane_freq", membrane_freq),
            ("tension",       tension),
            ("decay",         decay),
            ("noise_mix",     noise_mix),
            ("tone",          tone),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();

        let make_lowpole = |sr: f32| -> Box<dyn AudioUnit> {
            let mut p: Box<dyn AudioUnit> = Box::new(lowpole());
            p.set_sample_rate(sr as f64);
            p
        };

        // R channel starts at 45° (π/4) phase offset for immediate stereo spread on attack.
        // This prevents the L and R channels from constructively interfering at t=0.
        let phase_offset = std::f32::consts::FRAC_PI_4;

        let cell = Self {
            env_level: 0.0,
            phases_l: [0.0; 3],
            phases_r: [phase_offset; 3],
            tone_filter_l: make_lowpole(sr),
            tone_filter_r: make_lowpole(sr),
            noise_seed: 0x9a4e1c2d,
            membrane_freq_handle,
            tension_handle,
            decay_handle,
            noise_mix_handle,
            tone_handle,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }

    /// LCG pseudorandom noise (RT-safe, no allocation).
    fn next_noise(&mut self) -> f32 {
        self.noise_seed = self.noise_seed
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        (self.noise_seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl DspCell for StrikeVoiceCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Advance and apply the decay envelope
        let decay_time = self.decay_handle.value().clamp(0.01, 3.0);
        // exp(-1/(decay_time * sr)): coefficient such that after decay_time seconds,
        // env_level has fallen to 1/e ≈ 0.37.
        let decay_coeff = (-1.0 / (decay_time * self.sample_rate)).exp();
        self.env_level *= decay_coeff;

        // Skip synthesis when effectively silent — saves CPU on quiet passages.
        if self.env_level < 1e-6 {
            output[0] = 0.0;
            if output.len() > 1 {
                output[1] = 0.0;
            }
            return;
        }

        // Read current params
        let freq      = self.membrane_freq_handle.value().clamp(40.0, 800.0);
        let tension   = self.tension_handle.value().clamp(0.0, 1.0);
        let noise_mix = self.noise_mix_handle.value().clamp(0.0, 1.0);

        // Harmonic ratios: lerp between bayan (inharmonic) and dayan (harmonic).
        // Bayan (tabla left drum): inharmonic membrane modes 1.0, 1.59, 2.14
        // Dayan (tabla right drum): near-harmonic  modes   1.0, 2.0,  3.0
        let r2 = 1.59 + tension * (2.00 - 1.59);
        let r3 = 2.14 + tension * (3.00 - 2.14);
        let freqs = [freq, freq * r2, freq * r3];

        // Amplitude taper: fundamental loudest, partials fade naturally
        let gains = [1.0f32, 0.6, 0.3];

        // Nyquist guard: prevent aliasing for high freq × harmonic combos
        let nyquist = self.sample_rate * 0.45;

        // Advance resonator phases and sum for L and R
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;

        for i in 0..3 {
            let fc_l = freqs[i].min(nyquist);
            let delta_l = std::f32::consts::TAU * fc_l / self.sample_rate;
            self.phases_l[i] += delta_l;
            if self.phases_l[i] >= std::f32::consts::TAU {
                self.phases_l[i] -= std::f32::consts::TAU;
            }
            sum_l += self.phases_l[i].sin() * gains[i];

            // R channel: 0.3% frequency offset for natural stereo width without comb filtering
            let fc_r = (freqs[i] * 1.003).min(nyquist);
            let delta_r = std::f32::consts::TAU * fc_r / self.sample_rate;
            self.phases_r[i] += delta_r;
            if self.phases_r[i] >= std::f32::consts::TAU {
                self.phases_r[i] -= std::f32::consts::TAU;
            }
            sum_r += self.phases_r[i].sin() * gains[i];
        }

        // Add broadband noise — adds the "chik" attack character of a tabla strike.
        // Both channels share the same noise sample (centered mono noise component).
        let noise = self.next_noise();
        let raw_l = (sum_l + noise * noise_mix) * self.env_level;
        let raw_r = (sum_r + noise * noise_mix) * self.env_level;

        // Tone filter: lowpole rolls off high frequencies for darker tabla character.
        // Quadratic mapping: tone=0 → 200 Hz (warm/dark), tone=1 → 20 kHz (full brightness)
        let tone = self.tone_handle.value().clamp(0.0, 1.0);
        let tone_cutoff = 200.0 + tone * tone * 19800.0;

        let mut buf_l = [0.0f32];
        let mut buf_r = [0.0f32];
        self.tone_filter_l.tick(&[raw_l, tone_cutoff], &mut buf_l);
        self.tone_filter_r.tick(&[raw_r, tone_cutoff], &mut buf_r);

        let out_l = buf_l[0];
        let out_r = buf_r[0];

        // Analysis
        let avg = (out_l + out_r) * 0.5;
        self.rms_acc += avg * avg;
        self.peak = self.peak.max(out_l.abs()).max(out_r.abs());
        self.sample_count += 1;

        output[0] = out_l;
        if output.len() > 1 {
            output[1] = out_r;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { velocity, .. } => {
                // Strike: reset envelope to full amplitude and restart all resonator phases.
                // Phase reset gives the clean impulsive attack of a physical drum strike.
                // Retrigger during decay is intentional: natural for tabla (player strikes again).
                self.env_level = velocity.clamp(0.0, 1.0);
                self.phases_l = [0.0; 3];
                self.phases_r = [std::f32::consts::FRAC_PI_4; 3];
            }
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
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
        self.env_level = 0.0;
        self.phases_l = [0.0; 3];
        self.phases_r = [std::f32::consts::FRAC_PI_4; 3];
        self.tone_filter_l.reset();
        self.tone_filter_r.reset();
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "strike_voice_cell" }

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

    fn make_dna(freq: f32, tension: f32, decay: f32, noise_mix: f32, tone: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("membrane_freq".into(), freq);
        params.insert("tension".into(), tension);
        params.insert("decay".into(), decay);
        params.insert("noise_mix".into(), noise_mix);
        params.insert("tone".into(), tone);
        CellDna {
            cell_type: "strike_voice_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    fn trigger(cell: &mut Box<dyn DspCell>) {
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
    }

    #[test]
    fn strike_produces_audio_on_trigger() {
        let dna = make_dna(320.0, 0.7, 0.3, 0.15, 0.6);
        let (mut cell, _) = StrikeVoiceCell::new(&dna, SR).unwrap();

        trigger(&mut cell);

        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);

        // Should produce non-zero output immediately after trigger
        assert!(
            out[0].abs() > 0.001 || out[1].abs() > 0.001,
            "Strike should produce audio immediately after trigger, got L={}, R={}",
            out[0], out[1]
        );
    }

    #[test]
    fn strike_silent_before_trigger() {
        let dna = make_dna(320.0, 0.7, 0.3, 0.0, 0.6);
        let (mut cell, _) = StrikeVoiceCell::new(&dna, SR).unwrap();

        let mut out = [0.0f32; 2];
        for _ in 0..100 {
            cell.tick(&[], &mut out);
        }
        assert!(
            out[0].abs() < 1e-5 && out[1].abs() < 1e-5,
            "Should be silent before trigger"
        );
    }

    #[test]
    fn strike_decays_to_silence() {
        let dna = make_dna(320.0, 0.7, 0.1, 0.0, 1.0); // 0.1s decay, full brightness
        let (mut cell, _) = StrikeVoiceCell::new(&dna, SR).unwrap();

        trigger(&mut cell);

        let mut out = [0.0f32; 2];
        // After 3× the decay time (0.3s), should be very quiet
        let samples = (SR * 0.3) as usize;
        for _ in 0..samples {
            cell.tick(&[], &mut out);
        }

        let peak = out[0].abs().max(out[1].abs());
        assert!(
            peak < 0.05,
            "Strike should decay to near-silence after 3× decay time, peak={}",
            peak
        );
    }

    #[test]
    fn strike_membrane_freq_affects_pitch() {
        // Higher membrane_freq should produce higher zero-crossing rate (faster oscillation)
        let dna_low  = make_dna(100.0, 0.5, 0.5, 0.0, 1.0);
        let dna_high = make_dna(400.0, 0.5, 0.5, 0.0, 1.0);

        let (mut cell_low, _)  = StrikeVoiceCell::new(&dna_low,  SR).unwrap();
        let (mut cell_high, _) = StrikeVoiceCell::new(&dna_high, SR).unwrap();

        // Trigger both
        cell_low.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
        cell_high.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        let mut crossings_low = 0u32;
        let mut crossings_high = 0u32;
        let mut prev_low = 0.0f32;
        let mut prev_high = 0.0f32;

        // Count zero crossings over 0.1s
        for _ in 0..(SR * 0.1) as usize {
            cell_low.tick(&[], &mut out);
            if prev_low * out[0] < 0.0 { crossings_low += 1; }
            prev_low = out[0];

            cell_high.tick(&[], &mut out);
            if prev_high * out[0] < 0.0 { crossings_high += 1; }
            prev_high = out[0];
        }

        assert!(
            crossings_high > crossings_low * 2,
            "Higher membrane_freq should have more zero crossings: high={}, low={}",
            crossings_high, crossings_low
        );
    }

    #[test]
    fn strike_tension_affects_harmonics() {
        // Low tension = bayan (inharmonic): ratio2=1.59, ratio3=2.14
        // High tension = dayan (harmonic): ratio2=2.0, ratio3=3.0
        // Verify cell constructs and produces audio for both extremes
        let dna_bayan = make_dna(100.0, 0.0, 0.5, 0.0, 0.5);
        let dna_dayan = make_dna(320.0, 1.0, 0.3, 0.0, 0.7);

        let (mut bayan, _) = StrikeVoiceCell::new(&dna_bayan, SR).unwrap();
        let (mut dayan, _) = StrikeVoiceCell::new(&dna_dayan, SR).unwrap();

        bayan.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
        dayan.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        let mut bayan_peak = 0.0f32;
        let mut dayan_peak = 0.0f32;

        for _ in 0..1000 {
            bayan.tick(&[], &mut out);
            bayan_peak = bayan_peak.max(out[0].abs());

            dayan.tick(&[], &mut out);
            dayan_peak = dayan_peak.max(out[0].abs());
        }

        assert!(bayan_peak > 0.01, "Bayan (low tension) should produce audio: peak={}", bayan_peak);
        assert!(dayan_peak > 0.01, "Dayan (high tension) should produce audio: peak={}", dayan_peak);
    }

    #[test]
    fn strike_noise_mix_adds_broadband() {
        // With noise_mix=0 and noise_mix=1: both should produce audio (noise adds, not replaces)
        // Higher noise_mix → different spectral character (more energy at higher freq)
        let dna_clean = make_dna(320.0, 0.7, 0.5, 0.0, 1.0);
        let dna_noisy = make_dna(320.0, 0.7, 0.5, 1.0, 1.0);

        let (mut clean, _) = StrikeVoiceCell::new(&dna_clean, SR).unwrap();
        let (mut noisy, _) = StrikeVoiceCell::new(&dna_noisy, SR).unwrap();

        clean.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
        noisy.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        let mut peak_clean = 0.0f32;
        let mut peak_noisy = 0.0f32;

        for _ in 0..500 {
            clean.tick(&[], &mut out);
            peak_clean = peak_clean.max(out[0].abs());

            noisy.tick(&[], &mut out);
            peak_noisy = peak_noisy.max(out[0].abs());
        }

        assert!(peak_clean > 0.01, "Clean strike should produce audio");
        assert!(peak_noisy > 0.01, "Noisy strike should produce audio");
    }

    #[test]
    fn strike_tone_shapes_brightness() {
        // Low tone (dark) should produce less high-frequency content than high tone (bright).
        // Measure this as energy in the output signal — lower tone reduces overall output
        // because the lowpass filter attenuates more.
        let dna_dark  = make_dna(320.0, 0.7, 0.5, 0.0, 0.0); // 200Hz rolloff
        let dna_bright = make_dna(320.0, 0.7, 0.5, 0.0, 1.0); // Full range

        let (mut dark, _)   = StrikeVoiceCell::new(&dna_dark,   SR).unwrap();
        let (mut bright, _) = StrikeVoiceCell::new(&dna_bright, SR).unwrap();

        dark.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
        bright.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        let mut rms_dark = 0.0f32;
        let mut rms_bright = 0.0f32;
        let n = 2000usize;

        for _ in 0..n {
            dark.tick(&[], &mut out);
            rms_dark += out[0] * out[0];

            bright.tick(&[], &mut out);
            rms_bright += out[0] * out[0];
        }

        // Bright should have more energy than dark (lowpass blocks less at full brightness)
        assert!(
            rms_bright > rms_dark,
            "Bright tone should have more energy than dark tone: bright={:.6}, dark={:.6}",
            rms_bright, rms_dark
        );
    }

    #[test]
    fn strike_retrigger_resets_envelope() {
        let dna = make_dna(320.0, 0.7, 1.0, 0.0, 1.0); // 1s decay
        let (mut cell, _) = StrikeVoiceCell::new(&dna, SR).unwrap();

        // Initial trigger
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        // Let it decay for 0.5s (env_level drops to ~e^(-0.5) ≈ 0.6)
        for _ in 0..(SR * 0.5) as usize {
            cell.tick(&[], &mut out);
        }
        let peak_before_retrigger = out[0].abs().max(out[1].abs());

        // Retrigger
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
        cell.tick(&[], &mut out);
        let peak_after_retrigger = out[0].abs().max(out[1].abs());

        assert!(
            peak_after_retrigger > peak_before_retrigger,
            "Retrigger should restore amplitude: after={:.4}, before={:.4}",
            peak_after_retrigger, peak_before_retrigger
        );
    }

    #[test]
    fn strike_stereo_output() {
        let dna = make_dna(320.0, 0.7, 0.3, 0.0, 1.0);
        let (mut cell, _) = StrikeVoiceCell::new(&dna, SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });

        let mut out = [0.0f32; 2];
        let mut l_acc = 0.0f32;
        let mut r_acc = 0.0f32;

        for _ in 0..4410 {
            cell.tick(&[], &mut out);
            l_acc += out[0] * out[0];
            r_acc += out[1] * out[1];
        }

        // Both channels should have energy
        assert!(l_acc > 0.0, "Left channel should have energy");
        assert!(r_acc > 0.0, "Right channel should have energy");
        // Channels should be different (stereo spread from phase offset + detuning)
        assert!(
            (l_acc - r_acc).abs() / l_acc.max(r_acc) > 0.001,
            "L and R channels should be slightly different (stereo spread)"
        );
    }
}
