use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;
use fundsp::setting::Setting;

use super::DspAtom;

/// Fixed-time delay line. 1 input, 1 output. Time set at construction only.
pub struct DelayAtom {
    unit: Box<dyn AudioUnit>,
    delay_time: f32,
}

impl DelayAtom {
    pub fn new(time_secs: f32, sr: f32) -> Self {
        let mut unit: Box<dyn AudioUnit> = Box::new(delay(time_secs));
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self {
            unit,
            delay_time: time_secs,
        }
    }
}

impl DspAtom for DelayAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        self.unit.tick(input, output);
    }
    fn set_param(&mut self, _name: &str, _value: f32) -> bool {
        false // fixed at construction
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "time" => Some(self.delay_time),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        1
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "delay"
    }
}

/// Stereo panner. 1 mono input → 2 stereo outputs.
/// Position controlled via `AudioUnit::set(Setting::pan(p))`.
pub struct PanAtom {
    unit: Box<dyn AudioUnit>,
    position: f32,
}

impl PanAtom {
    pub fn new(position: f32, sr: f32) -> Self {
        let mut unit: Box<dyn AudioUnit> = Box::new(pan(position));
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self { unit, position }
    }
}

impl DspAtom for PanAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        self.unit.tick(input, output);
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "position" => {
                self.position = value;
                self.unit.set(Setting::pan(value));
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "position" => Some(self.position),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        1
    }
    fn audio_outputs(&self) -> usize {
        2
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "pan"
    }
}

/// Gate/threshold comparator. Custom implementation (not FunDSP).
/// Output is 1.0 when |input| >= threshold, 0.0 otherwise. 1→1.
pub struct GateAtom {
    threshold: f32,
}

impl GateAtom {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl DspAtom for GateAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        output[0] = if input[0].abs() >= self.threshold {
            1.0
        } else {
            0.0
        };
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "threshold" => {
                self.threshold = value;
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "threshold" => Some(self.threshold),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        1
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        // Stateless — nothing to reset.
    }
    fn name(&self) -> &str {
        "gate"
    }
}

/// Envelope follower. Fixed time constant set at construction. 1→1.
pub struct EnvFollowAtom {
    unit: Box<dyn AudioUnit>,
    time: f32,
}

impl EnvFollowAtom {
    pub fn new(time_secs: f32, sr: f32) -> Self {
        let mut unit: Box<dyn AudioUnit> = Box::new(follow(time_secs));
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self {
            unit,
            time: time_secs,
        }
    }
}

impl DspAtom for EnvFollowAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        self.unit.tick(input, output);
    }
    fn set_param(&mut self, _name: &str, _value: f32) -> bool {
        false // fixed at construction
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "time" => Some(self.time),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        1
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "env_follow"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::{render_atom_with_input, rms};
    use std::f32::consts::PI;

    const SR: f32 = 44100.0;

    fn sine_signal(freq_hz: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * PI * freq_hz * i as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn delay_offsets_signal() {
        let delay_secs = 0.01; // 10ms = 441 samples
        let mut atom = DelayAtom::new(delay_secs, SR);
        // Feed an impulse then silence
        let mut signal = vec![0.0f32; 1000];
        signal[0] = 1.0;
        let out = render_atom_with_input(&mut atom, &signal);
        // The impulse should appear around sample 441
        let expected_sample = (delay_secs * SR) as usize;
        let peak_idx = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .unwrap()
            .0;
        let diff = (peak_idx as i32 - expected_sample as i32).unsigned_abs();
        assert!(
            diff <= 2,
            "Peak should be near sample {expected_sample}, got {peak_idx}"
        );
    }

    #[test]
    fn delay_no_params() {
        let atom = DelayAtom::new(0.01, SR);
        assert!(atom.get_param("time").is_some());
        assert_eq!(atom.audio_inputs(), 1);
        assert_eq!(atom.audio_outputs(), 1);
    }

    #[test]
    fn pan_center_equal_lr() {
        let mut atom = PanAtom::new(0.0, SR);
        let signal = sine_signal(440.0, 4410);
        let mut out_l_energy = 0.0f32;
        let mut out_r_energy = 0.0f32;
        let mut output = [0.0f32; 2];
        for &s in &signal {
            atom.tick(&[s], &mut output);
            out_l_energy += output[0] * output[0];
            out_r_energy += output[1] * output[1];
        }
        let ratio = out_l_energy / out_r_energy.max(1e-10);
        assert!(
            (ratio - 1.0).abs() < 0.3,
            "Center pan should be roughly equal L/R: ratio={ratio}"
        );
    }

    #[test]
    fn pan_left_louder_left() {
        let mut atom = PanAtom::new(0.0, SR);
        atom.set_param("position", -1.0);
        let signal = sine_signal(440.0, 4410);
        let mut out_l_energy = 0.0f32;
        let mut out_r_energy = 0.0f32;
        let mut output = [0.0f32; 2];
        for &s in &signal {
            atom.tick(&[s], &mut output);
            out_l_energy += output[0] * output[0];
            out_r_energy += output[1] * output[1];
        }
        assert!(
            out_l_energy > out_r_energy,
            "Pan left should have more left energy: L={out_l_energy} R={out_r_energy}"
        );
    }

    #[test]
    fn pan_io() {
        let atom = PanAtom::new(0.0, SR);
        assert_eq!(atom.audio_inputs(), 1);
        assert_eq!(atom.audio_outputs(), 2);
    }

    #[test]
    fn gate_opens_above_threshold() {
        let mut atom = GateAtom::new(0.5);
        let mut out = [0.0f32];
        atom.tick(&[0.7], &mut out);
        assert_eq!(out[0], 1.0);
        atom.tick(&[0.3], &mut out);
        assert_eq!(out[0], 0.0);
        atom.tick(&[-0.6], &mut out);
        assert_eq!(out[0], 1.0); // abs(-0.6) >= 0.5
    }

    #[test]
    fn gate_threshold_change() {
        let mut atom = GateAtom::new(0.5);
        let mut out = [0.0f32];
        atom.tick(&[0.4], &mut out);
        assert_eq!(out[0], 0.0);
        atom.set_param("threshold", 0.3);
        atom.tick(&[0.4], &mut out);
        assert_eq!(out[0], 1.0);
    }

    #[test]
    fn env_follow_tracks_amplitude() {
        let mut atom = EnvFollowAtom::new(0.01, SR);
        // Feed loud signal
        let loud = sine_signal(440.0, 4410);
        let out_loud = render_atom_with_input(&mut atom, &loud);
        let rms_loud = rms(&out_loud[2000..]); // skip transient

        atom.reset();
        // Feed quiet signal
        let quiet: Vec<f32> = sine_signal(440.0, 4410)
            .iter()
            .map(|s| s * 0.1)
            .collect();
        let out_quiet = render_atom_with_input(&mut atom, &quiet);
        let rms_quiet = rms(&out_quiet[2000..]);

        assert!(
            rms_loud > rms_quiet * 2.0,
            "Follower should track louder signal higher: {rms_loud} vs {rms_quiet}"
        );
    }

    #[test]
    fn env_follow_io() {
        let atom = EnvFollowAtom::new(0.01, SR);
        assert_eq!(atom.audio_inputs(), 1);
        assert_eq!(atom.audio_outputs(), 1);
    }
}
