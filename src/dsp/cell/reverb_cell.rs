use fundsp::hacker32::*;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared;
use crate::organism::dna::CellDna;

/// Stereo reverb cell using FunDSP's reverb_stereo (32-channel FDN).
///
/// Parameters:
///   size  — room size [0, 1]
///   dcy   — decay time [0, 1] (mapped to seconds)
///   damp  — high-frequency damping [0, 1]
///   mix   — dry/wet mix [0, 1]
pub struct ReverbCell {
    reverb: Box<dyn AudioUnit>,
    // Our Shared handles
    mix_handle: shared::Shared,
    // Analysis
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl ReverbCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, shared::Shared)>)> {
        let size = param_or(dna, "size", 0.7);
        let dcy = param_or(dna, "dcy", 0.7);
        let damp = param_or(dna, "damp", 0.4);
        let mix = param_or(dna, "mix", 0.4);

        // Map DNA params to FunDSP reverb_stereo args:
        //   room_size: size * 40 + 10 → [10, 50] meters
        //   time: dcy * 8 + 1 → [1, 9] seconds
        //   damping: damp
        let room_size = size * 40.0 + 10.0;
        let time = dcy * 8.0 + 1.0;

        let mut reverb: Box<dyn AudioUnit> = Box::new(reverb_stereo(room_size, time, damp));
        reverb.set_sample_rate(sr as f64);

        let size_handle = shared::shared(size);
        let dcy_handle = shared::shared(dcy);
        let damp_handle = shared::shared(damp);
        let mix_handle = shared::shared(mix);

        let handles = vec![
            ("size".into(), size_handle),
            ("dcy".into(), dcy_handle),
            ("damp".into(), damp_handle),
            ("mix".into(), mix_handle.clone()),
        ];

        let cell = Self {
            reverb,
            mix_handle,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for ReverbCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let dry_l = if !input.is_empty() { input[0] } else { 0.0 };
        let dry_r = if input.len() > 1 { input[1] } else { dry_l };

        // Process stereo through reverb
        let mut wet = [0.0f32; 2];
        self.reverb.tick(&[dry_l, dry_r], &mut wet);

        // Dry/wet mix
        let mix = self.mix_handle.value().clamp(0.0, 1.0);
        let left = dry_l * (1.0 - mix) + wet[0] * mix;
        let right = dry_r * (1.0 - mix) + wet[1] * mix;

        // Analysis
        let mono = (left + right) * 0.5;
        self.rms_acc += mono * mono;
        self.peak = self.peak.max(left.abs()).max(right.abs());
        self.sample_count += 1;

        output[0] = left;
        if output.len() > 1 {
            output[1] = right;
        }
    }

    fn handle_command(&mut self, _cmd: &DspCommand) {}

    fn analysis(&self) -> DspAnalysis {
        if self.sample_count > 0 {
            DspAnalysis {
                rms: (self.rms_acc / self.sample_count as f32).sqrt(),
                peak: self.peak,
            }
        } else {
            DspAnalysis { rms: 0.0, peak: 0.0 }
        }
    }

    fn output_channels(&self) -> usize { 2 }

    fn reset(&mut self) {
        self.reverb.reset();
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "reverb" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("size".into(), 0.5);
        params.insert("dcy".into(), 0.5);
        params.insert("damp".into(), 0.3);
        params.insert("mix".into(), 0.5);
        CellDna {
            cell_type: "reverb".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn builds_successfully() {
        let dna = make_dna();
        assert!(ReverbCell::new(&dna, 44100.0).is_some());
    }

    #[test]
    fn silence_in_produces_silence_out() {
        let dna = make_dna();
        let (mut cell, _) = ReverbCell::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        // Feed silence
        for _ in 0..44100 {
            cell.tick(&[0.0, 0.0], &mut out);
        }
        assert!(out[0].abs() < 0.001, "Silence in should give ~silence out: {}", out[0]);
    }

    #[test]
    fn reverb_tail_persists() {
        let dna = make_dna();
        let (mut cell, _) = ReverbCell::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];

        // Feed sustained signal for ~100ms to prime the reverb
        for _ in 0..4410 {
            cell.tick(&[0.5, 0.5], &mut out);
        }

        // Now feed silence and check that reverb tail continues
        let mut any_nonzero = false;
        for _ in 0..44100 {
            cell.tick(&[0.0, 0.0], &mut out);
            if out[0].abs() > 0.0001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Reverb should have a tail after sustained input");
    }

    #[test]
    fn mix_zero_is_dry_passthrough() {
        let mut dna = make_dna();
        dna.params.insert("mix".into(), 0.0);
        let (mut cell, _) = ReverbCell::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];

        // Feed a known signal
        cell.tick(&[0.5, 0.5], &mut out);
        assert!(
            (out[0] - 0.5).abs() < 0.01,
            "mix=0 should pass dry signal through: {}", out[0]
        );
    }

    #[test]
    fn all_params_exposed() {
        let dna = make_dna();
        let (_, handles) = ReverbCell::new(&dna, 44100.0).unwrap();
        let names: Vec<&str> = handles.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"size"));
        assert!(names.contains(&"dcy"));
        assert!(names.contains(&"damp"));
        assert!(names.contains(&"mix"));
    }
}
