use fundsp::prelude32::{shared, Shared};

use crate::dsp::atom::effects::DelayAtom;
use crate::dsp::atom::filters::AllpassAtom;
use crate::dsp::atom::oscillators::SineAtom;
use crate::dsp::atom::DspAtom;
use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// DRON upper harmonics shimmer cell.
///
/// Inline shimmer wash: SineAtom (octave up) -> AllpassAtom chain -> feedback delay.
/// Always running (no NoteOn/NoteOff needed).
/// Output: stereo (2ch).
pub struct ShimmerLayer {
    /// Sine oscillator at octave-up frequency.
    osc: SineAtom,
    /// Chain of allpass filters for diffusion.
    allpass: [AllpassAtom; 3],
    /// Feedback delay line.
    delay: DelayAtom,
    /// Feedback accumulator (left).
    feedback_l: f32,
    /// Feedback accumulator (right).
    feedback_r: f32,
    shimmer_amount: Shared,
    diffusion: Shared,
    feedback: Shared,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
    sample_rate: f32,
}

impl ShimmerLayer {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let shimmer = param_or(dna, "shimmer_amount", 0.5);
        let diff = param_or(dna, "diffusion", 0.5);
        let fb = param_or(dna, "feedback", 0.4);

        let shimmer_amount = shared(shimmer);
        let diffusion = shared(diff);
        let feedback = shared(fb);

        // Octave-up sine at 220Hz (will be modulated by root_hz from HarmonicBed via wiring)
        let osc = SineAtom::new(220.0, sr);

        // Allpass chain at different frequencies for diffusion
        let allpass = [
            AllpassAtom::new(1100.0, 0.5, sr),
            AllpassAtom::new(1700.0, 0.5, sr),
            AllpassAtom::new(2300.0, 0.5, sr),
        ];

        let delay = DelayAtom::new(0.05, sr); // 50ms feedback delay

        let handles = vec![
            ("shimmer_amount".into(), shimmer_amount.clone()),
            ("diffusion".into(), diffusion.clone()),
            ("feedback".into(), feedback.clone()),
        ];

        let cell = ShimmerLayer {
            osc,
            allpass,
            delay,
            feedback_l: 0.0,
            feedback_r: 0.0,
            shimmer_amount,
            diffusion,
            feedback,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
            sample_rate: sr,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for ShimmerLayer {
    fn tick(&mut self, output: &mut [f32]) {
        let shimmer = self.shimmer_amount.value();
        let diff = self.diffusion.value();
        let fb = self.feedback.value().clamp(0.0, 0.8);

        // Update allpass Q based on diffusion
        for ap in &mut self.allpass {
            ap.set_param("q", 0.3 + diff * 0.7);
        }

        // Generate shimmer source
        let mut osc_out = [0.0f32];
        self.osc.tick(&[], &mut osc_out);

        // Feed through allpass chain with feedback
        let mut signal = osc_out[0] * shimmer + self.feedback_l * fb;

        for ap in &mut self.allpass {
            let mut ap_out = [0.0f32];
            ap.tick(&[signal], &mut ap_out);
            signal = ap_out[0];
        }

        // Delay for feedback
        let mut delay_out = [0.0f32];
        self.delay.tick(&[signal], &mut delay_out);
        self.feedback_l = delay_out[0];

        // Create pseudo-stereo by inverting phase for right channel
        let left = signal * 0.5;
        let right = delay_out[0] * 0.5;

        output[0] = left;
        if output.len() > 1 {
            output[1] = right;
        }

        // Update analysis
        let mono = (left + right) * 0.5;
        self.rms_acc += mono * mono;
        self.peak = self.peak.max(left.abs()).max(right.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
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
        DspAnalysis {
            rms,
            peak: self.peak,
        }
    }

    fn output_channels(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.osc.reset();
        for ap in &mut self.allpass {
            ap.reset();
        }
        self.delay.reset();
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "shimmer_layer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("shimmer_amount".into(), 0.5);
        params.insert("diffusion".into(), 0.5);
        params.insert("feedback".into(), 0.4);
        CellDna {
            cell_type: "shimmer_layer".into(),
            params,
        }
    }

    #[test]
    fn shimmer_layer_produces_stereo_audio() {
        let (mut cell, _) = ShimmerLayer::from_dna(&make_dna(), SR).unwrap();

        let mut has_audio = false;
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            cell.tick(&mut out);
            if out[0].abs() > 0.001 || out[1].abs() > 0.001 {
                has_audio = true;
            }
        }

        assert!(has_audio, "ShimmerLayer should produce audio");
    }

    #[test]
    fn shimmer_layer_is_stereo() {
        let (cell, _) = ShimmerLayer::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }
}
