use fundsp::prelude32::{shared, Shared};

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::molecule::{tblk, Molecule};
use crate::organism::dna::CellDna;

/// TBLK percussion voice cell.
///
/// Owns membrane_sim + snap_transient + body_resonance molecules.
/// NoteOn triggers the hit (resets molecules for a fresh transient).
/// Output: mono (1ch).
pub struct StrikeVoice {
    membrane: Molecule,
    snap: Molecule,
    body: Molecule,
    membrane_freq: Shared,
    bandwidth: Shared,
    click_mix: Shared,
    body_feedback: Shared,
    velocity: f32,
    /// Exponential decay for snap transient (1.0 on NoteOn, decays each sample).
    snap_decay: f32,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl StrikeVoice {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let freq = param_or(dna, "membrane_freq", 180.0);
        let bw = param_or(dna, "bandwidth", 60.0);
        let click = param_or(dna, "click_mix", 0.3);
        let fb = param_or(dna, "body_feedback", 0.4);

        let membrane_freq = shared(freq);
        let bandwidth = shared(bw);
        let click_mix = shared(click);
        let body_feedback = shared(fb);

        // Compute delay time from body_feedback: higher feedback = shorter delay = higher pitch
        let delay_time = 1.0 / freq.max(20.0);

        let membrane = tblk::membrane_sim(freq, bw, sr);
        let snap = tblk::snap_transient(sr);
        let body = tblk::body_resonance(delay_time.min(0.05), sr);

        let handles = vec![
            ("membrane_freq".into(), membrane_freq.clone()),
            ("bandwidth".into(), bandwidth.clone()),
            ("click_mix".into(), click_mix.clone()),
            ("body_feedback".into(), body_feedback.clone()),
        ];

        let cell = StrikeVoice {
            membrane,
            snap,
            body,
            membrane_freq,
            bandwidth,
            click_mix,
            body_feedback,
            velocity: 0.0,
            snap_decay: 0.0,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for StrikeVoice {
    fn tick(&mut self, output: &mut [f32]) {
        let mut mem_out = [0.0f32];
        let mut snap_out = [0.0f32];
        let mut body_out = [0.0f32];

        // Update membrane params from shared handles
        self.membrane
            .set_param("center", self.membrane_freq.value());
        self.membrane
            .set_param("bandwidth", self.bandwidth.value());

        self.membrane.tick(&[], &mut mem_out);
        self.snap.tick(&[], &mut snap_out);
        snap_out[0] *= self.snap_decay;
        self.snap_decay *= 0.995; // ~3ms exponential decay at 44100Hz

        let click = self.click_mix.value();
        let mix = mem_out[0] * (1.0 - click) + snap_out[0] * click;
        let mix = mix * self.velocity;

        let fb = self.body_feedback.value().clamp(0.0, 0.95);
        self.body.tick(&[mix], &mut body_out);
        let sample = (mix * (1.0 - fb) + body_out[0] * fb).tanh();
        output[0] = sample;

        // Update analysis
        self.rms_acc += sample * sample;
        self.peak = self.peak.max(sample.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { velocity, .. } => {
                self.velocity = *velocity;
                self.snap_decay = 1.0;
                // Reset molecules for fresh transient
                self.membrane.reset();
                self.snap.reset();
                self.body.reset();
            }
            DspCommand::NoteOff => {
                // Percussion self-decays, but we can reduce velocity
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
        self.membrane.reset();
        self.snap.reset();
        self.body.reset();
        self.velocity = 0.0;
        self.snap_decay = 0.0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "strike_voice"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("membrane_freq".into(), 180.0);
        params.insert("bandwidth".into(), 60.0);
        params.insert("click_mix".into(), 0.3);
        params.insert("body_feedback".into(), 0.4);
        CellDna {
            cell_type: "strike_voice".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn strike_voice_produces_audio_on_note_on() {
        let (mut cell, _handles) = StrikeVoice::from_dna(&make_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 180.0,
            velocity: 1.0,
        });

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.001, "StrikeVoice should produce audio: rms={rms}");
    }

    #[test]
    fn strike_voice_silent_without_trigger() {
        let (mut cell, _handles) = StrikeVoice::from_dna(&make_dna(), SR).unwrap();
        // No NoteOn
        let mut out = [0.0f32];
        for _ in 0..1000 {
            cell.tick(&mut out);
        }
        // velocity is 0, so output should be near zero
        assert!(
            out[0].abs() < 0.01,
            "StrikeVoice without trigger should be quiet"
        );
    }

    #[test]
    fn strike_voice_is_mono() {
        let (cell, _) = StrikeVoice::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }

    #[test]
    fn strike_voice_shared_handles_work() {
        let (mut cell, handles) = StrikeVoice::from_dna(&make_dna(), SR).unwrap();
        // Find the membrane_freq handle and change it
        for (name, handle) in &handles {
            if name == "membrane_freq" {
                handle.set(300.0);
            }
        }
        cell.handle_command(&DspCommand::NoteOn {
            freq: 300.0,
            velocity: 1.0,
        });
        let mut out = [0.0f32];
        for _ in 0..100 {
            cell.tick(&mut out);
        }
        // Just verify it runs without panic
    }
}
