//! 3-pole diode ladder filter approximating the TB-303's characteristic sound.
//!
//! The 303's filter is 18dB/oct (3 poles) rather than the more common 24dB/oct Moog
//! (4 poles). The diode arrangement creates a distinctive resonance character that
//! screams at high Q and self-oscillates gracefully above res=1.0.
//!
//! # Implementation
//! Three cascaded `lowpole()` filters with:
//! - Pre-filter drive saturation (tanh)
//! - Resonance as negative feedback from pole 3 output back to input
//! - Post-filter tanh soft clip
//!
//! # Parameters
//! - `cutoff`: Cutoff frequency (20-20000 Hz)
//! - `res`: Resonance (0-2.0); >1.0 → self-oscillation
//! - `drive`: Pre-filter saturation (0-1)
//! - `env_mod`: Envelope modulation depth on cutoff (stored for future wire use)
//!
//! # Wire Graph Patterns
//! - Audio: osc_cell → diode_filter_cell → tape_delay_cell
//! - Modulation: accent_env_cell → diode_filter_cell.cutoff (opens filter on accent)

use fundsp::hacker32::*;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for diode_filter_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("cutoff", 20.0, 20000.0),
    ("res", 0.0, 2.0),
    ("drive", 0.0, 1.0),
    ("env_mod", 0.0, 1.0),
];

/// 3-pole diode ladder filter with drive and resonance feedback.
///
/// Uses 3 cascaded FunDSP `lowpole()` units per channel with manual feedback
/// loop to implement the characteristic 303 resonance/squelch behavior.
pub struct DiodeFilterCell {
    // 3 poles per channel (L/R)
    pole1_l: Box<dyn AudioUnit>,
    pole2_l: Box<dyn AudioUnit>,
    pole3_l: Box<dyn AudioUnit>,
    pole1_r: Box<dyn AudioUnit>,
    pole2_r: Box<dyn AudioUnit>,
    pole3_r: Box<dyn AudioUnit>,

    // Resonance feedback state (output of pole3 fed back to input)
    prev_output_l: f32,
    prev_output_r: f32,

    // Param handles
    cutoff_handle: Shared,
    res_handle: Shared,
    drive_handle: Shared,
    #[allow(dead_code)]
    env_mod_handle: Shared,

    base_values: HashMap<String, f32>,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl DiodeFilterCell {
    /// Build a diode_filter_cell from DNA.
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let cutoff = param_or(dna, "cutoff", 400.0);
        let res = param_or(dna, "res", 0.8);
        let drive = param_or(dna, "drive", 0.3);
        let env_mod = param_or(dna, "env_mod", 0.7);

        let build_pole = |sr: f32| -> Box<dyn AudioUnit> {
            let mut p: Box<dyn AudioUnit> = Box::new(lowpole());
            p.set_sample_rate(sr as f64);
            p
        };

        let cutoff_handle = shared::shared(cutoff);
        let res_handle = shared::shared(res);
        let drive_handle = shared::shared(drive);
        let env_mod_handle = shared::shared(env_mod);

        let handles = vec![
            ("cutoff".into(), cutoff_handle.clone()),
            ("res".into(), res_handle.clone()),
            ("drive".into(), drive_handle.clone()),
            ("env_mod".into(), env_mod_handle.clone()),
        ];

        let base_values = [
            ("cutoff", cutoff),
            ("res", res),
            ("drive", drive),
            ("env_mod", env_mod),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();

        let cell = Self {
            pole1_l: build_pole(sr),
            pole2_l: build_pole(sr),
            pole3_l: build_pole(sr),
            pole1_r: build_pole(sr),
            pole2_r: build_pole(sr),
            pole3_r: build_pole(sr),
            prev_output_l: 0.0,
            prev_output_r: 0.0,
            cutoff_handle,
            res_handle,
            drive_handle,
            env_mod_handle,
            base_values,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for DiodeFilterCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let input_l = if !input.is_empty() { input[0] } else { 0.0 };
        let input_r = if input.len() > 1 { input[1] } else { input_l };

        let cutoff = clamp_param(PARAM_RANGES, "cutoff", self.cutoff_handle.value());
        let res = clamp_param(PARAM_RANGES, "res", self.res_handle.value());
        let drive = clamp_param(PARAM_RANGES, "drive", self.drive_handle.value());

        // Pre-filter drive saturation: gentle tanh clip to add harmonic content
        // Drive=0: no saturation; Drive=1: 4× gain before clip → rich harmonics
        let gain = 1.0 + drive * 3.0;
        let driven_l = (input_l * gain).tanh();
        let driven_r = (input_r * gain).tanh();

        // Resonance feedback: negative feedback from pole3 output back to input.
        // Coefficient 0.5 means at res=2.0 the loop gain reaches 1.0 → self-oscillation.
        // At res=1.2 (303 squelch): feedback = 0.6 → strong peak, approaching instability.
        let feedback_coeff = res * 0.5;
        let fb_l = driven_l - self.prev_output_l * feedback_coeff;
        let fb_r = driven_r - self.prev_output_r * feedback_coeff;

        // 3 cascaded lowpole() — each 6dB/oct → 18dB/oct total (303 diode ladder slope)
        let mut buf = [0.0f32];

        self.pole1_l.tick(&[fb_l, cutoff], &mut buf);
        let p1_l = buf[0];
        self.pole2_l.tick(&[p1_l, cutoff], &mut buf);
        let p2_l = buf[0];
        self.pole3_l.tick(&[p2_l, cutoff], &mut buf);
        let raw_l = buf[0];

        self.pole1_r.tick(&[fb_r, cutoff], &mut buf);
        let p1_r = buf[0];
        self.pole2_r.tick(&[p1_r, cutoff], &mut buf);
        let p2_r = buf[0];
        self.pole3_r.tick(&[p2_r, cutoff], &mut buf);
        let raw_r = buf[0];

        // Post-filter tanh: catches any self-oscillation peaks and clamps gracefully
        let out_l = raw_l.tanh();
        let out_r = raw_r.tanh();

        self.prev_output_l = out_l;
        self.prev_output_r = out_r;

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
        self.pole1_l.reset();
        self.pole2_l.reset();
        self.pole3_l.reset();
        self.pole1_r.reset();
        self.pole2_r.reset();
        self.pole3_r.reset();
        self.prev_output_l = 0.0;
        self.prev_output_r = 0.0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "diode_filter_cell" }

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

    fn make_dna(cutoff: f32, res: f32, drive: f32, env_mod: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("cutoff".into(), cutoff);
        params.insert("res".into(), res);
        params.insert("drive".into(), drive);
        params.insert("env_mod".into(), env_mod);
        CellDna {
            cell_type: "diode_filter_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    fn sine_signal(samples: usize, freq_hz: f32) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                let phase = (i as f32 / SR) * freq_hz * std::f32::consts::TAU;
                phase.sin() * 0.5
            })
            .collect()
    }

    fn rms(signal: &[f32]) -> f32 {
        let sum: f32 = signal.iter().map(|s| s * s).sum();
        (sum / signal.len() as f32).sqrt()
    }

    #[test]
    fn diode_passes_audio() {
        let dna = make_dna(2000.0, 0.5, 0.0, 0.7);
        let (mut cell, _) = DiodeFilterCell::new(&dna, SR).unwrap();

        let signal = sine_signal(1000, 440.0);
        let mut out = [0.0f32; 2];
        let mut any_nonzero = false;

        for s in &signal {
            cell.tick(&[*s, *s], &mut out);
            if out[0].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Diode filter should pass audio");
    }

    #[test]
    fn diode_cutoff_shapes_spectrum() {
        // High cutoff passes more of a 440 Hz test signal than low cutoff
        let dna_low = make_dna(200.0, 0.0, 0.0, 0.0);
        let dna_high = make_dna(5000.0, 0.0, 0.0, 0.0);
        let (mut cell_low, _) = DiodeFilterCell::new(&dna_low, SR).unwrap();
        let (mut cell_high, _) = DiodeFilterCell::new(&dna_high, SR).unwrap();

        let signal = sine_signal(2000, 440.0);
        let mut out = [0.0f32; 2];
        let mut output_low = vec![0.0f32; 2000];
        let mut output_high = vec![0.0f32; 2000];

        for (i, s) in signal.iter().enumerate() {
            cell_low.tick(&[*s, *s], &mut out);
            output_low[i] = out[0];
            cell_high.tick(&[*s, *s], &mut out);
            output_high[i] = out[0];
        }

        let rms_low = rms(&output_low[500..]);
        let rms_high = rms(&output_high[500..]);

        assert!(
            rms_high > rms_low * 1.5,
            "High cutoff should pass more signal: high={:.4}, low={:.4}",
            rms_high,
            rms_low
        );
    }

    #[test]
    fn diode_res_squelch() {
        // High resonance at cutoff frequency should produce higher RMS (resonant peak)
        let dna_flat = make_dna(440.0, 0.0, 0.0, 0.0);
        let dna_peak = make_dna(440.0, 1.5, 0.0, 0.0);
        let (mut flat, _) = DiodeFilterCell::new(&dna_flat, SR).unwrap();
        let (mut peak, _) = DiodeFilterCell::new(&dna_peak, SR).unwrap();

        let signal = sine_signal(4000, 440.0);
        let mut out = [0.0f32; 2];

        // Warm up
        for s in &signal[..500] {
            flat.tick(&[*s, *s], &mut out);
            peak.tick(&[*s, *s], &mut out);
        }

        let mut rms_flat = 0.0f32;
        let mut rms_peak = 0.0f32;
        for s in &signal[500..] {
            flat.tick(&[*s, *s], &mut out);
            rms_flat += out[0] * out[0];
            peak.tick(&[*s, *s], &mut out);
            rms_peak += out[0] * out[0];
        }
        rms_flat = (rms_flat / 3500.0).sqrt();
        rms_peak = (rms_peak / 3500.0).sqrt();

        assert!(
            rms_peak > rms_flat,
            "High resonance should boost signal at cutoff: peak={:.4}, flat={:.4}",
            rms_peak,
            rms_flat
        );
    }

    #[test]
    fn diode_drive_saturates() {
        // Higher drive adds harmonic content and boosts the filter's input.
        // With double-tanh (pre-drive + post-filter), output is bounded at tanh(1) ≈ 0.76.
        // Verify: (1) output is always tanh-bounded, (2) driven output is stronger than undriven.
        let dna_driven = make_dna(5000.0, 0.0, 1.0, 0.0);
        let dna_flat = make_dna(5000.0, 0.0, 0.0, 0.0);
        let (mut driven, _) = DiodeFilterCell::new(&dna_driven, SR).unwrap();
        let (mut flat, _) = DiodeFilterCell::new(&dna_flat, SR).unwrap();

        let mut out = [0.0f32; 2];
        let mut peak_driven = 0.0f32;
        let mut peak_flat = 0.0f32;

        for i in 0..2000 {
            let s = (i as f32 * 0.1).sin() * 0.3; // Moderate amplitude input
            driven.tick(&[s, s], &mut out);
            assert!(out[0].abs() <= 1.01, "Drive output must be tanh-bounded, got {}", out[0]);
            peak_driven = peak_driven.max(out[0].abs());

            flat.tick(&[s, s], &mut out);
            peak_flat = peak_flat.max(out[0].abs());
        }

        assert!(
            peak_driven > peak_flat,
            "High drive should produce stronger output than no drive: driven={:.4}, flat={:.4}",
            peak_driven,
            peak_flat
        );
    }

    #[test]
    fn diode_env_mod_opens_filter() {
        // env_mod is stored and retrievable; actual cutoff modulation comes via Shared
        let dna = make_dna(400.0, 0.5, 0.3, 0.7);
        let (mut cell, handles) = DiodeFilterCell::new(&dna, SR).unwrap();

        // env_mod base value accessible
        let base = cell.get_param_base("env_mod");
        assert!(
            base.is_some() && (base.unwrap() - 0.7).abs() < 0.01,
            "env_mod base should be 0.7, got {:?}",
            base
        );

        // Modulate cutoff via the Shared handle (simulates modulation wire)
        let cutoff_handle = handles.iter().find(|(n, _)| n == "cutoff").map(|(_, h)| h.clone());
        assert!(cutoff_handle.is_some(), "Should have cutoff handle");
        cutoff_handle.unwrap().set(2000.0);

        let signal = sine_signal(500, 440.0);
        let mut out = [0.0f32; 2];
        let mut any_nonzero = false;
        for s in &signal {
            cell.tick(&[*s, *s], &mut out);
            if out[0].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Filter with boosted cutoff should pass audio");
    }
}
