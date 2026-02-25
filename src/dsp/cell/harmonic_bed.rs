use fundsp::prelude32::{shared, Shared};

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::molecule::{dron, Molecule};
use crate::organism::dna::CellDna;

/// DRON continuous drone voice cell.
///
/// Owns detuned_stack + slow_filter + stereo_spread molecules.
/// Always running (no NoteOn/NoteOff needed).
/// Output: stereo (2ch).
pub struct HarmonicBed {
    stack: Molecule,
    filter: Molecule,
    spread: Molecule,
    root_hz: Shared,
    detune_cents: Shared,
    cutoff: Shared,
    resonance: Shared,
    pan_rate: Shared,
    rms_acc_l: f32,
    rms_acc_r: f32,
    peak: f32,
    sample_count: u32,
}

impl HarmonicBed {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let root = param_or(dna, "root_hz", 110.0);
        let detune = param_or(dna, "detune_cents", 5.0);
        let cut = param_or(dna, "cutoff", 800.0);
        let res = param_or(dna, "resonance", 0.707);
        let pan = param_or(dna, "pan_rate", 0.05);

        let root_hz = shared(root);
        let detune_cents = shared(detune);
        let cutoff = shared(cut);
        let resonance = shared(res);
        let pan_rate = shared(pan);

        let stack = dron::detuned_stack(root, sr);
        let filter = dron::slow_filter(cut, res, sr);
        let spread = dron::stereo_spread(sr);

        // Set initial LFO rate for stereo spread
        let mut spread_mol = spread;
        spread_mol.set_param("lfo.rate", pan);

        let handles = vec![
            ("root_hz".into(), root_hz.clone()),
            ("detune_cents".into(), detune_cents.clone()),
            ("cutoff".into(), cutoff.clone()),
            ("resonance".into(), resonance.clone()),
            ("pan_rate".into(), pan_rate.clone()),
        ];

        let cell = HarmonicBed {
            stack,
            filter,
            spread: spread_mol,
            root_hz,
            detune_cents,
            cutoff,
            resonance,
            pan_rate,
            rms_acc_l: 0.0,
            rms_acc_r: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for HarmonicBed {
    fn tick(&mut self, output: &mut [f32]) {
        // Update params from shared handles
        let root = self.root_hz.value();
        let detune = self.detune_cents.value();
        let cents_ratio_up = 2.0f32.powf(detune / 1200.0);
        let cents_ratio_down = 2.0f32.powf(-detune / 1200.0);

        self.stack.set_param("f1", root);
        self.stack.set_param("f2", root * cents_ratio_up);
        self.stack.set_param("f3", root * cents_ratio_down);
        self.stack.set_param("f_sub", root * 0.5);

        self.filter.set_param("cutoff", self.cutoff.value());
        self.filter.set_param("q", self.resonance.value());

        self.spread.set_param("lfo.rate", self.pan_rate.value());

        // Process chain
        let mut stack_out = [0.0f32];
        let mut filt_out = [0.0f32];
        let mut stereo_out = [0.0f32; 2];

        self.stack.tick(&[], &mut stack_out);
        self.filter.tick(&[stack_out[0]], &mut filt_out);
        dron::tick_stereo_spread(&mut self.spread, &[filt_out[0]], &mut stereo_out);

        output[0] = stereo_out[0];
        if output.len() > 1 {
            output[1] = stereo_out[1];
        }

        // Update analysis
        self.rms_acc_l += stereo_out[0] * stereo_out[0];
        self.rms_acc_r += stereo_out[1] * stereo_out[1];
        self.peak = self.peak.max(stereo_out[0].abs()).max(stereo_out[1].abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {} // Always running, NoteOn/Off ignored
        }
    }

    fn analysis(&self) -> DspAnalysis {
        let rms = if self.sample_count > 0 {
            let avg = (self.rms_acc_l + self.rms_acc_r) / (2.0 * self.sample_count as f32);
            avg.sqrt()
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
        self.stack.reset();
        self.filter.reset();
        self.spread.reset();
        self.rms_acc_l = 0.0;
        self.rms_acc_r = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "harmonic_bed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("root_hz".into(), 110.0);
        params.insert("detune_cents".into(), 5.0);
        params.insert("cutoff".into(), 800.0);
        params.insert("resonance".into(), 0.707);
        params.insert("pan_rate".into(), 0.05);
        CellDna {
            cell_type: "harmonic_bed".into(),
            params,
        }
    }

    #[test]
    fn harmonic_bed_produces_continuous_stereo() {
        let (mut cell, _) = HarmonicBed::from_dna(&make_dna(), SR).unwrap();

        let mut buf_l = Vec::new();
        let mut buf_r = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            cell.tick(&mut out);
            buf_l.push(out[0]);
            buf_r.push(out[1]);
        }

        let rms_l: f32 =
            (buf_l.iter().map(|s| s * s).sum::<f32>() / buf_l.len() as f32).sqrt();
        let rms_r: f32 =
            (buf_r.iter().map(|s| s * s).sum::<f32>() / buf_r.len() as f32).sqrt();
        assert!(rms_l > 0.01, "HarmonicBed left channel should have audio: rms={rms_l}");
        assert!(rms_r > 0.01, "HarmonicBed right channel should have audio: rms={rms_r}");
    }

    #[test]
    fn harmonic_bed_is_stereo() {
        let (cell, _) = HarmonicBed::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }
}
