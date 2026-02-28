use fundsp::hacker32::*;
use fundsp::shared::Shared as FundspShared;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared;
use crate::organism::dna::CellDna;

/// Fast 2^x approximation for |x| <= 1.0 (3rd-order Taylor series).
/// Max error ~0.008% at x=±0.3, ~0.3% at x=±1.0. Avoids per-sample powf().
#[inline]
fn fast_pow2(x: f32) -> f32 {
    const LN2: f32 = 0.693_147_2;
    let a = x * LN2;
    // 1 + a + a²/2 + a³/6
    1.0 + a * (1.0 + a * (0.5 + a * (1.0 / 6.0)))
}

/// Maximum Q value passed to the Moog filter.
///
/// FunDSP's moog internally maps Q through a cutoff-dependent multiplier of ~2.5–4.0×,
/// meaning Q=0.25 produces rez≈0.88 (near self-oscillation). We scale the user-facing
/// `res` (0–1) by this constant so:
///   res=0.0 → Q=0.0  → rez=0.0 (flat)
///   res=0.5 → Q=0.1  → rez≈0.35 (warm)
///   res=1.0 → Q=0.2  → rez≈0.70 (strong, musical)
const MOOG_MAX_Q: f32 = 0.2;

/// Drone bed cell: dual detuned oscillators → filter → LFO on cutoff.
///
/// Oscillator type selected by `wtype` string param:
///   "soft_saw" (default) — mellow, triangle-like harmonic rolloff
///   "saw" — maximally harmonic-rich, aggressive
///   "sine" — pure tone
///   "square" — hollow, odd harmonics
///   "triangle" — soft, odd harmonics with steep rolloff
///
/// Filter type selected by `ftype` string param:
///   "moog" (default) — 4-pole ladder, warm resonance
///   "lowpass" — 2-pole SVF, cleaner
pub struct DroneBed {
    osc1: Box<dyn AudioUnit>,
    osc2: Box<dyn AudioUnit>,
    filter: Box<dyn AudioUnit>,
    filter_type: FilterType,
    // Our Shared handles (UI ↔ audio thread bridge)
    root_hz_handle: shared::Shared,
    det_handle: shared::Shared,
    osc_mix_handle: shared::Shared,
    cutoff_handle: shared::Shared,
    res_handle: shared::Shared,
    lfo_rate_handle: shared::Shared,
    lfo_depth_handle: shared::Shared,
    // FunDSP internal shareds
    freq1_shared: FundspShared,
    freq2_shared: FundspShared,
    // Cached detune ratio: 2^(det/1200). Recomputed only when det changes.
    cached_det: f32,
    cached_det_ratio: f32,
    // LFO phase (manual sine, avoids extra AudioUnit overhead)
    lfo_phase: f32,
    sample_rate: f32,
    // Base values from DNA (for additive modulation)
    base_values: HashMap<String, f32>,
    // Analysis accumulators
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

#[derive(Clone, Copy)]
enum FilterType {
    Moog,
    Lowpass,
}

// build_osc() moved to super (cell/mod.rs) as shared helper

/// Build a filter AudioUnit from an ftype string.
fn build_filter(ftype: &str, sr: f32) -> (Box<dyn AudioUnit>, FilterType) {
    match ftype {
        "lowpass" => {
            // lowpass() is a 2-pole SVF: 2 inputs (audio, cutoff_hz), 1 output
            let mut f: Box<dyn AudioUnit> = Box::new(lowpole());
            f.set_sample_rate(sr as f64);
            (f, FilterType::Lowpass)
        }
        // "moog" and anything else → moog (warm 4-pole ladder)
        _ => {
            let mut f: Box<dyn AudioUnit> = Box::new(moog());
            f.set_sample_rate(sr as f64);
            (f, FilterType::Moog)
        }
    }
}

impl DroneBed {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, shared::Shared)>)> {
        let root_hz = param_or(dna, "root_hz", 110.0);
        let det = param_or(dna, "det", 7.0);
        let osc_mix = param_or(dna, "osc_mix", 0.7);
        let cutoff = param_or(dna, "cutoff", 1200.0);
        let res = param_or(dna, "res", 0.25);
        let lfo_rate = param_or(dna, "lfo_rate", 0.07);
        let lfo_depth = param_or(dna, "lfo_depth", 0.3);

        // Read string params for oscillator and filter type
        let wtype = string_param_or(dna, "wtype", "soft_saw");
        let ftype = string_param_or(dna, "ftype", "moog");

        // FunDSP shareds for reactive frequency control
        let freq1_shared = FundspShared::new(root_hz);
        let freq2_shared = FundspShared::new(root_hz * 2f32.powf(det / 1200.0));

        // Build oscillators from wtype (using shared helper from cell/mod.rs)
        let osc1 = super::build_osc(wtype, &freq1_shared, sr);
        let osc2 = super::build_osc(wtype, &freq2_shared, sr);

        // Build filter from ftype
        let (filter, filter_type) = build_filter(ftype, sr);

        // Our Shared handles for the control thread
        let root_hz_handle = shared::shared(root_hz);
        let det_handle = shared::shared(det);
        let osc_mix_handle = shared::shared(osc_mix);
        let cutoff_handle = shared::shared(cutoff);
        let res_handle = shared::shared(res);
        let lfo_rate_handle = shared::shared(lfo_rate);
        let lfo_depth_handle = shared::shared(lfo_depth);

        let handles = vec![
            ("root_hz".into(), root_hz_handle.clone()),
            ("det".into(), det_handle.clone()),
            ("osc_mix".into(), osc_mix_handle.clone()),
            ("cutoff".into(), cutoff_handle.clone()),
            ("res".into(), res_handle.clone()),
            ("lfo_rate".into(), lfo_rate_handle.clone()),
            ("lfo_depth".into(), lfo_depth_handle.clone()),
        ];

        // Store base values from DNA for additive modulation
        let base_values = [
            ("root_hz", root_hz),
            ("det", det),
            ("osc_mix", osc_mix),
            ("cutoff", cutoff),
            ("res", res),
            ("lfo_rate", lfo_rate),
            ("lfo_depth", lfo_depth),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        let cell = Self {
            osc1,
            osc2,
            filter,
            filter_type,
            root_hz_handle,
            det_handle,
            osc_mix_handle,
            cutoff_handle,
            res_handle,
            lfo_rate_handle,
            lfo_depth_handle,
            freq1_shared,
            freq2_shared,
            cached_det: det,
            cached_det_ratio: 2f32.powf(det / 1200.0),
            lfo_phase: 0.0,
            sample_rate: sr,
            base_values,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for DroneBed {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Bridge: read our Shared params, write to FunDSP shareds
        let root = self.root_hz_handle.value();
        let det = self.det_handle.value();
        let mix = self.osc_mix_handle.value().clamp(0.0, 1.0);
        let base_cutoff = self.cutoff_handle.value();
        let res = self.res_handle.value().clamp(0.0, 1.0);
        let lfo_rate = self.lfo_rate_handle.value();
        let lfo_depth = self.lfo_depth_handle.value().clamp(0.0, 1.0);

        // Cache detune ratio — only recompute powf when det actually changes
        if det != self.cached_det {
            self.cached_det = det;
            self.cached_det_ratio = 2f32.powf(det / 1200.0);
        }

        self.freq1_shared.set_value(root);
        self.freq2_shared.set_value(root * self.cached_det_ratio);

        // Process one sample from each oscillator
        let s1 = self.osc1.get_mono();
        let s2 = self.osc2.get_mono();

        // Crossfade with pre-filter gain reduction (*0.5)
        // Prevents filter overdrive and downstream tanh distortion
        let osc_out = (s1 * mix + s2 * (1.0 - mix)) * 0.5;

        // LFO: sine modulation on cutoff
        // cutoff_hz = base_cutoff * 2^(depth * sin(phase))
        // Fast polynomial approximation of 2^x for |x| <= 1.0
        // (3rd-order Taylor of 2^x around 0; max error ~0.008% at x=±0.3)
        let lfo_val = (self.lfo_phase * std::f32::consts::TAU).sin();
        let lfo_x = lfo_depth * lfo_val;
        let cutoff_hz = (base_cutoff * fast_pow2(lfo_x))
            .clamp(20.0, 20000.0);

        // Advance LFO phase
        self.lfo_phase += lfo_rate / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        // Filter: dispatch based on filter type
        let mut filtered = [0.0f32];
        match self.filter_type {
            FilterType::Moog => {
                // Scale res → Q so the 0–1 slider maps to musical resonance (rez ≈ 0–0.7)
                // instead of pushing near self-oscillation (rez ≈ 0.88 at Q=0.25).
                let q = res * MOOG_MAX_Q;
                // moog(): tick(input=[audio, cutoff_hz, Q], output=[filtered])
                self.filter.tick(&[osc_out, cutoff_hz, q], &mut filtered);
            }
            FilterType::Lowpass => {
                // lowpole(): tick(input=[audio, cutoff_hz], output=[filtered])
                self.filter.tick(&[osc_out, cutoff_hz], &mut filtered);
            }
        }

        // Post-filter output scaling — keeps organism-level soft_clip below compression knee
        let sample = filtered[0] * 0.8;

        // Analysis accumulation
        self.rms_acc += sample * sample;
        self.peak = self.peak.max(sample.abs());
        self.sample_count += 1;

        // Output mono → stereo
        output[0] = sample;
        if output.len() > 1 {
            output[1] = sample;
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
        self.osc1.reset();
        self.osc2.reset();
        self.filter.reset();
        self.lfo_phase = 0.0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "drone_bed" }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_dna(root_hz: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("root_hz".into(), root_hz);
        params.insert("det".into(), 7.0);
        params.insert("osc_mix".into(), 0.7);
        params.insert("cutoff".into(), 1200.0);
        params.insert("res".into(), 0.25);
        params.insert("lfo_rate".into(), 0.07);
        params.insert("lfo_depth".into(), 0.3);
        let mut string_params = BTreeMap::new();
        string_params.insert("wtype".into(), "soft_saw".into());
        string_params.insert("ftype".into(), "moog".into());
        CellDna {
            cell_type: "drone_bed".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn builds_successfully() {
        let dna = make_dna(110.0);
        let result = DroneBed::new(&dna, 44100.0);
        assert!(result.is_some());
    }

    #[test]
    fn produces_nonzero_audio() {
        let dna = make_dna(440.0);
        let (mut cell, _handles) = DroneBed::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        for _ in 0..100 {
            cell.tick(&[], &mut out);
        }
        let mut any_nonzero = false;
        for _ in 0..441 {
            cell.tick(&[], &mut out);
            if out[0].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Filtered saw should produce audible output");
    }

    #[test]
    fn output_in_range() {
        let dna = make_dna(110.0);
        let (mut cell, _handles) = DroneBed::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            cell.tick(&[], &mut out);
            assert!(out[0].abs() <= 1.5, "Output should be bounded, got {}", out[0]);
            assert!(out[1].abs() <= 1.5, "Output should be bounded, got {}", out[1]);
        }
    }

    #[test]
    fn shared_handle_changes_frequency() {
        let dna = make_dna(110.0);
        let (mut cell, handles) = DroneBed::new(&dna, 44100.0).unwrap();
        let root_hz = handles.iter().find(|(n, _)| n == "root_hz").map(|(_, h)| h).unwrap();

        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }
        root_hz.set(440.0);
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }
        let mut any_nonzero = false;
        for _ in 0..441 {
            cell.tick(&[], &mut out);
            if out[0].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Should still produce audio after frequency change");
    }

    #[test]
    fn analysis_tracks_signal() {
        let dna = make_dna(440.0);
        let (mut cell, _handles) = DroneBed::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }
        let analysis = cell.analysis();
        assert!(analysis.rms > 0.0, "RMS should be nonzero: {}", analysis.rms);
        assert!(analysis.peak > 0.0, "Peak should be nonzero: {}", analysis.peak);
    }

    #[test]
    fn detune_produces_beating() {
        let mut dna = make_dna(220.0);
        dna.params.insert("det".into(), 0.0);
        dna.params.insert("osc_mix".into(), 0.5);
        let (mut cell_no_det, _) = DroneBed::new(&dna, 44100.0).unwrap();

        dna.params.insert("det".into(), 7.0);
        let (mut cell_det, _) = DroneBed::new(&dna, 44100.0).unwrap();

        let window = 2205;
        let windows = 10;
        let mut rms_no_det = Vec::new();
        let mut rms_det = Vec::new();
        let mut out = [0.0f32; 2];

        for _ in 0..windows {
            let mut acc = 0.0f32;
            for _ in 0..window {
                cell_no_det.tick(&[], &mut out);
                acc += out[0] * out[0];
            }
            rms_no_det.push((acc / window as f32).sqrt());
        }
        for _ in 0..windows {
            let mut acc = 0.0f32;
            for _ in 0..window {
                cell_det.tick(&[], &mut out);
                acc += out[0] * out[0];
            }
            rms_det.push((acc / window as f32).sqrt());
        }

        let var_no_det = rms_variance(&rms_no_det);
        let var_det = rms_variance(&rms_det);
        assert!(
            var_det > var_no_det * 0.5,
            "Detuned should have amplitude modulation: var_det={var_det}, var_no_det={var_no_det}"
        );
    }

    #[test]
    fn osc_mix_crossfade() {
        let mut dna = make_dna(440.0);
        dna.params.insert("det".into(), 100.0);
        dna.params.insert("osc_mix".into(), 1.0);
        let (mut cell, _) = DroneBed::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }
        let rms_osc1_only = cell.analysis().rms;

        dna.params.insert("osc_mix".into(), 0.0);
        let (mut cell, _) = DroneBed::new(&dna, 44100.0).unwrap();
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }
        let rms_osc2_only = cell.analysis().rms;

        assert!(rms_osc1_only > 0.01, "osc_mix=1.0 should produce audio: {rms_osc1_only}");
        assert!(rms_osc2_only > 0.01, "osc_mix=0.0 should produce audio: {rms_osc2_only}");
    }

    #[test]
    fn filter_reduces_brightness() {
        // Low cutoff should reduce RMS compared to high cutoff.
        // Use "saw" wtype for maximum harmonic content (makes filter effect obvious).
        // Use zero resonance to avoid resonant peaks adding energy at the cutoff frequency.
        let mut dna = make_dna(110.0);
        dna.string_params.insert("wtype".into(), "saw".into());
        dna.params.insert("lfo_depth".into(), 0.0);
        dna.params.insert("res".into(), 0.0);

        dna.params.insert("cutoff".into(), 20000.0); // wide open
        let (mut cell_open, _) = DroneBed::new(&dna, 44100.0).unwrap();

        dna.params.insert("cutoff".into(), 80.0); // below fundamental — very dark
        let (mut cell_dark, _) = DroneBed::new(&dna, 44100.0).unwrap();

        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            cell_open.tick(&[], &mut out);
            cell_dark.tick(&[], &mut out);
        }

        let rms_open = cell_open.analysis().rms;
        let rms_dark = cell_dark.analysis().rms;

        // Low cutoff should have less energy (filter removes harmonics + attenuates fundamental)
        assert!(
            rms_dark < rms_open,
            "Low cutoff ({rms_dark}) should have less energy than high cutoff ({rms_open})"
        );
    }

    #[test]
    fn lfo_modulates_brightness() {
        // With LFO active, the filtered signal should have amplitude variation
        let mut dna = make_dna(110.0);
        dna.params.insert("cutoff".into(), 800.0);
        dna.params.insert("lfo_rate".into(), 2.0); // fast LFO for visible modulation
        dna.params.insert("lfo_depth".into(), 0.8);
        let (mut cell, _) = DroneBed::new(&dna, 44100.0).unwrap();

        let window = 2205; // 50ms
        let windows = 10;
        let mut rms_windows = Vec::new();
        let mut out = [0.0f32; 2];

        for _ in 0..windows {
            let mut acc = 0.0f32;
            for _ in 0..window {
                cell.tick(&[], &mut out);
                acc += out[0] * out[0];
            }
            rms_windows.push((acc / window as f32).sqrt());
        }

        let variance = rms_variance(&rms_windows);
        assert!(
            variance > 1e-6,
            "LFO should cause RMS variation across windows, variance={variance}"
        );
    }

    #[test]
    fn all_params_exposed_as_handles() {
        let dna = make_dna(110.0);
        let (_cell, handles) = DroneBed::new(&dna, 44100.0).unwrap();
        let names: Vec<&str> = handles.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"root_hz"));
        assert!(names.contains(&"det"));
        assert!(names.contains(&"osc_mix"));
        assert!(names.contains(&"cutoff"));
        assert!(names.contains(&"res"));
        assert!(names.contains(&"lfo_rate"));
        assert!(names.contains(&"lfo_depth"));
        assert_eq!(handles.len(), 7);
    }

    fn rms_variance(values: &[f32]) -> f32 {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
    }
}
