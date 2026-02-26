use fundsp::prelude32::{shared, Shared};

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::molecule::{melo, Molecule};
use crate::dsp::molecule::melo::{OscMode, FilterMode};
use crate::organism::dna::CellDna;

/// MELO synth voice cell.
///
/// Owns oscillator + filter_envelope + amp_envelope molecules.
/// NoteOn sets freq and triggers envelopes, NoteOff releases.
/// Output: mono (1ch).
///
/// Supports osc.mode (string_param) → OscMode dispatch:
///   sine_cluster, tri_cluster, saw_unison, sine_fm, square_fm
/// Supports filter.type (string_param) → FilterMode dispatch:
///   lowpass, ladder, svf_drive
pub struct TimbreVoice {
    osc: Molecule,
    filter_env: Molecule,
    amp_env: Molecule,
    freq: Shared,
    pulse_width: Shared,
    filter_base: Shared,
    filter_depth: Shared,
    filter_q: Shared,
    attack_ms: Shared,
    decay_ms: Shared,
    sustain: Shared,
    release_ms: Shared,
    detune_cents: Shared,
    osc_unison: Shared,
    osc_sub_mix: Shared,
    osc_fm_ratio: Shared,
    osc_fm_index: Shared,
    filter_drive: Shared,
    osc_mode: OscMode,
    /// Velocity of current note.
    velocity: f32,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl TimbreVoice {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let freq_val = param_or(dna, "freq", 440.0);
        let pw = param_or(dna, "pulse_width", 0.5);
        let fbase = param_or(dna, "filter_base", 200.0);
        let fdepth = param_or(dna, "filter_depth", 5000.0);
        let fq = param_or(dna, "filter_q", 0.707);
        let att = param_or(dna, "attack_ms", 5.0);
        let dec = param_or(dna, "decay_ms", 100.0);
        let sus = param_or(dna, "sustain", 0.7);
        let rel = param_or(dna, "release_ms", 200.0);
        let fdrive = param_or(dna, "filter.drive", 1.0);
        let detune = param_or(dna, "osc.detune_cents", 7.0);
        let unison_val = param_or(dna, "osc.unison", 3.0);
        let sub_mix_val = param_or(dna, "osc.sub_mix", 0.2);
        let fm_ratio_val = param_or(dna, "osc.fm_ratio", 2.0);
        let fm_index_val = param_or(dna, "osc.fm_index", 1.0);

        let osc_mode = OscMode::from_str(string_param_or(dna, "osc.mode", "sine_cluster"));
        let filter_mode = FilterMode::from_str(string_param_or(dna, "filter.type", "lowpass"));

        let freq = shared(freq_val);
        let pulse_width = shared(pw);
        let filter_base = shared(fbase);
        let filter_depth = shared(fdepth);
        let filter_q = shared(fq);
        let attack_ms = shared(att);
        let decay_ms = shared(dec);
        let sustain = shared(sus);
        let release_ms = shared(rel);
        let filter_drive = shared(fdrive);
        let detune_cents = shared(detune);
        let osc_unison = shared(unison_val);
        let osc_sub_mix = shared(sub_mix_val);
        let osc_fm_ratio = shared(fm_ratio_val);
        let osc_fm_index = shared(fm_index_val);

        // Build oscillator: use osc_engine for new modes, fall back to osc_pair for default
        let osc = if dna.string_params.contains_key("osc.mode") {
            melo::osc_engine(osc_mode, dna, sr)
        } else {
            melo::osc_pair(freq_val, pw, sr)
        };

        // Build filter envelope: use typed for new modes, fall back to original
        let (filter_env, _fb, _fd) = if dna.string_params.contains_key("filter.type") {
            melo::filter_envelope_typed(fbase, fdepth, fdrive, filter_mode, sr)
        } else {
            melo::filter_envelope(fbase, fdepth, sr)
        };

        let amp_env = melo::amp_envelope(sr);

        // Set ADSR params on filter and amp envelopes
        let mut filter_env = filter_env;
        filter_env.set_param("adsr.a", att);
        filter_env.set_param("adsr.d", dec);
        filter_env.set_param("adsr.s", sus);
        filter_env.set_param("adsr.r", rel);

        let mut amp_env = amp_env;
        amp_env.set_param("adsr.a", att);
        amp_env.set_param("adsr.d", dec);
        amp_env.set_param("adsr.s", sus);
        amp_env.set_param("adsr.r", rel);

        let mut handles = vec![
            ("freq".into(), freq.clone()),
            ("pulse_width".into(), pulse_width.clone()),
            ("filter_base".into(), filter_base.clone()),
            ("filter_depth".into(), filter_depth.clone()),
            ("filter_q".into(), filter_q.clone()),
            ("attack_ms".into(), attack_ms.clone()),
            ("decay_ms".into(), decay_ms.clone()),
            ("sustain".into(), sustain.clone()),
            ("release_ms".into(), release_ms.clone()),
            ("osc.detune_cents".into(), detune_cents.clone()),
            ("osc.unison".into(), osc_unison.clone()),
            ("osc.sub_mix".into(), osc_sub_mix.clone()),
            ("osc.fm_ratio".into(), osc_fm_ratio.clone()),
            ("osc.fm_index".into(), osc_fm_index.clone()),
            ("filter.drive".into(), filter_drive.clone()),
        ];

        let cell = TimbreVoice {
            osc,
            filter_env,
            amp_env,
            freq,
            pulse_width,
            filter_base,
            filter_depth,
            filter_q,
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            detune_cents,
            osc_unison,
            osc_sub_mix,
            osc_fm_ratio,
            osc_fm_index,
            filter_drive,
            osc_mode,
            velocity: 0.0,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for TimbreVoice {
    fn tick(&mut self, output: &mut [f32]) {
        let freq_val = self.freq.value();

        // Update oscillator params from shared handles
        self.osc.set_param("freq", freq_val);
        self.osc.set_param("osc.freq", freq_val);
        self.osc.set_param("freq_sub", freq_val * 0.5);
        self.osc.set_param("pulse_width", self.pulse_width.value());

        // Forward new osc params
        self.osc.set_param("osc.detune_cents", self.detune_cents.value());
        self.osc.set_param("osc.unison", self.osc_unison.value());
        self.osc.set_param("osc.sub_mix", self.osc_sub_mix.value());
        self.osc.set_param("osc.fm_ratio", self.osc_fm_ratio.value());
        self.osc.set_param("osc.fm_index", self.osc_fm_index.value());

        // Update ADSR params if changed
        let att = self.attack_ms.value();
        let dec = self.decay_ms.value();
        let sus = self.sustain.value();
        let rel = self.release_ms.value();

        self.filter_env.set_param("adsr.a", att);
        self.filter_env.set_param("adsr.d", dec);
        self.filter_env.set_param("adsr.s", sus);
        self.filter_env.set_param("adsr.r", rel);
        self.amp_env.set_param("adsr.a", att);
        self.amp_env.set_param("adsr.d", dec);
        self.amp_env.set_param("adsr.s", sus);
        self.amp_env.set_param("adsr.r", rel);

        // Update filter envelope base/depth from shared handles
        self.filter_env
            .set_param("cutoff_base", self.filter_base.value());
        self.filter_env
            .set_param("cutoff_depth", self.filter_depth.value());

        // Update filter drive
        self.filter_env.set_param("filter.drive", self.filter_drive.value());

        // Process chain
        let mut osc_out = [0.0f32];
        let mut filt_out = [0.0f32];
        let mut amp_out = [0.0f32];

        self.osc.tick(&[], &mut osc_out);
        self.filter_env.tick(&[osc_out[0]], &mut filt_out);
        self.amp_env.tick(&[filt_out[0]], &mut amp_out);

        let sample = (amp_out[0] * self.velocity).tanh();
        output[0] = sample;

        // Update analysis
        self.rms_acc += sample * sample;
        self.peak = self.peak.max(sample.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { freq, velocity } => {
                self.freq.set(*freq);
                self.velocity = *velocity;
                self.osc.set_param("freq", *freq);
                self.osc.set_param("osc.freq", *freq);
                self.osc.set_param("freq_sub", *freq * 0.5);
                // Gate on
                self.filter_env.set_param("adsr.gate", 1.0);
                self.amp_env.set_param("adsr.gate", 1.0);
            }
            DspCommand::NoteOff => {
                // Gate off
                self.filter_env.set_param("adsr.gate", 0.0);
                self.amp_env.set_param("adsr.gate", 0.0);
            }
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
        }
    }

    fn analysis(&self) -> DspAnalysis {
        let rms = if self.sample_count > 0 {
            (self.rms_acc / self.sample_count as f32).sqrt()
        } else {
            0.0
        };
        DspAnalysis {
            rms,
            peak: self.peak,
        }
    }

    fn output_channels(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.osc.reset();
        self.filter_env.reset();
        self.amp_env.reset();
        self.filter_env.set_param("adsr.gate", 0.0);
        self.amp_env.set_param("adsr.gate", 0.0);
        self.velocity = 0.0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "timbre_voice"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), 440.0);
        params.insert("pulse_width".into(), 0.5);
        params.insert("filter_base".into(), 200.0);
        params.insert("filter_depth".into(), 5000.0);
        params.insert("filter_q".into(), 0.707);
        params.insert("attack_ms".into(), 5.0);
        params.insert("decay_ms".into(), 100.0);
        params.insert("sustain".into(), 0.7);
        params.insert("release_ms".into(), 200.0);
        CellDna {
            cell_type: "timbre_voice".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn timbre_voice_produces_audio_on_note_on() {
        let (mut cell, _) = TimbreVoice::from_dna(&make_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 440.0,
            velocity: 1.0,
        });

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.01, "TimbreVoice should produce audio: rms={rms}");
    }

    #[test]
    fn timbre_voice_silent_after_release() {
        let (mut cell, _) = TimbreVoice::from_dna(&make_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 440.0,
            velocity: 1.0,
        });

        // Play through attack+decay
        let mut out = [0.0f32];
        for _ in 0..8820 {
            cell.tick(&mut out);
        }

        // Release
        cell.handle_command(&DspCommand::NoteOff);

        // Wait for release to finish (200ms = 8820 samples, add margin for exponential)
        for _ in 0..20000 {
            cell.tick(&mut out);
        }

        assert!(
            out[0].abs() < 0.05,
            "TimbreVoice should be silent after release: {}",
            out[0]
        );
    }

    #[test]
    fn timbre_voice_is_mono() {
        let (cell, _) = TimbreVoice::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
