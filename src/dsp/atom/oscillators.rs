use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

use super::DspAtom;

/// White noise source. No parameters, 0 inputs, 1 output.
pub struct NoiseAtom {
    unit: Box<dyn AudioUnit>,
}

impl NoiseAtom {
    pub fn new(sr: f32) -> Self {
        let mut unit: Box<dyn AudioUnit> = Box::new(noise());
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self { unit }
    }
}

impl DspAtom for NoiseAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.unit.tick(&[], output);
    }
    fn set_param(&mut self, _name: &str, _value: f32) -> bool {
        false
    }
    fn get_param(&self, _name: &str) -> Option<f32> {
        None
    }
    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "noise"
    }
}

/// Sine oscillator. Shared `freq` param, 0 inputs, 1 output.
pub struct SineAtom {
    unit: Box<dyn AudioUnit>,
    freq: Shared,
}

impl SineAtom {
    pub fn new(freq_hz: f32, sr: f32) -> Self {
        let freq = shared(freq_hz);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> sine());
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self { unit, freq }
    }
}

impl DspAtom for SineAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.unit.tick(&[], output);
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq.set(value);
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq.value()),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "sine"
    }
}

/// Sawtooth oscillator. Shared `freq` param, 0 inputs, 1 output.
pub struct SawAtom {
    unit: Box<dyn AudioUnit>,
    freq: Shared,
}

impl SawAtom {
    pub fn new(freq_hz: f32, sr: f32) -> Self {
        let freq = shared(freq_hz);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> saw());
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self { unit, freq }
    }
}

impl DspAtom for SawAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.unit.tick(&[], output);
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq.set(value);
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq.value()),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "saw"
    }
}

/// Square wave oscillator. Shared `freq` param, 0 inputs, 1 output.
pub struct SquareAtom {
    unit: Box<dyn AudioUnit>,
    freq: Shared,
}

impl SquareAtom {
    pub fn new(freq_hz: f32, sr: f32) -> Self {
        let freq = shared(freq_hz);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> square());
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self { unit, freq }
    }
}

impl DspAtom for SquareAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.unit.tick(&[], output);
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq.set(value);
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq.value()),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "square"
    }
}

/// Pulse wave oscillator. Shared `freq` and `width` params, 0 inputs, 1 output.
pub struct PulseAtom {
    unit: Box<dyn AudioUnit>,
    freq: Shared,
    width: Shared,
}

impl PulseAtom {
    pub fn new(freq_hz: f32, width: f32, sr: f32) -> Self {
        let freq = shared(freq_hz);
        let w = shared(width);
        let mut unit: Box<dyn AudioUnit> = Box::new((var(&freq) | var(&w)) >> pulse());
        unit.set_sample_rate(sr as f64);
        unit.allocate();
        Self {
            unit,
            freq,
            width: w,
        }
    }
}

impl DspAtom for PulseAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.unit.tick(&[], output);
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq.set(value);
                true
            }
            "width" => {
                self.width.set(value);
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq.value()),
            "width" => Some(self.width.value()),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        self.unit.reset();
    }
    fn name(&self) -> &str {
        "pulse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::{render_atom, rms, zero_crossings};

    const SR: f32 = 44100.0;
    const SAMPLES: usize = 4410; // 100ms

    #[test]
    fn noise_produces_audio() {
        let mut atom = NoiseAtom::new(SR);
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.01, "Noise should produce audio");
    }

    #[test]
    fn noise_has_no_params() {
        let mut atom = NoiseAtom::new(SR);
        assert!(!atom.set_param("anything", 1.0));
        assert!(atom.get_param("anything").is_none());
    }

    #[test]
    fn sine_produces_audio() {
        let mut atom = SineAtom::new(440.0, SR);
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.1, "Sine should produce audio");
    }

    #[test]
    fn sine_freq_change_affects_zero_crossings() {
        let mut atom = SineAtom::new(220.0, SR);
        let buf_low = render_atom(&mut atom, SAMPLES);
        let zc_low = zero_crossings(&buf_low);

        atom.reset();
        atom.set_param("freq", 880.0);
        let buf_high = render_atom(&mut atom, SAMPLES);
        let zc_high = zero_crossings(&buf_high);

        assert!(
            zc_high > zc_low * 2,
            "880Hz should have >2x zero crossings of 220Hz: {zc_high} vs {zc_low}"
        );
    }

    #[test]
    fn sine_reset_works() {
        let mut atom = SineAtom::new(440.0, SR);
        render_atom(&mut atom, SAMPLES);
        atom.reset();
        // After reset, atom should still produce audio without panic
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.1, "Sine should produce audio after reset");
    }

    #[test]
    fn saw_produces_audio() {
        let mut atom = SawAtom::new(440.0, SR);
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.1, "Saw should produce audio");
    }

    #[test]
    fn saw_freq_change() {
        let mut atom = SawAtom::new(220.0, SR);
        let buf_low = render_atom(&mut atom, SAMPLES);
        let zc_low = zero_crossings(&buf_low);

        atom.reset();
        atom.set_param("freq", 880.0);
        let buf_high = render_atom(&mut atom, SAMPLES);
        let zc_high = zero_crossings(&buf_high);

        assert!(
            zc_high > zc_low * 2,
            "880Hz saw should have >2x zero crossings of 220Hz: {zc_high} vs {zc_low}"
        );
    }

    #[test]
    fn square_produces_audio() {
        let mut atom = SquareAtom::new(440.0, SR);
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.1, "Square should produce audio");
    }

    #[test]
    fn pulse_produces_audio() {
        let mut atom = PulseAtom::new(440.0, 0.5, SR);
        let buf = render_atom(&mut atom, SAMPLES);
        assert!(rms(&buf) > 0.1, "Pulse should produce audio");
    }

    #[test]
    fn pulse_width_param() {
        let mut atom = PulseAtom::new(440.0, 0.5, SR);
        assert!(atom.set_param("width", 0.25));
        assert!((atom.get_param("width").unwrap() - 0.25).abs() < 0.01);
    }

    #[test]
    fn all_oscillators_0_inputs_1_output() {
        assert_eq!(NoiseAtom::new(SR).audio_inputs(), 0);
        assert_eq!(NoiseAtom::new(SR).audio_outputs(), 1);
        assert_eq!(SineAtom::new(440.0, SR).audio_inputs(), 0);
        assert_eq!(SineAtom::new(440.0, SR).audio_outputs(), 1);
        assert_eq!(SawAtom::new(440.0, SR).audio_inputs(), 0);
        assert_eq!(SawAtom::new(440.0, SR).audio_outputs(), 1);
        assert_eq!(SquareAtom::new(440.0, SR).audio_inputs(), 0);
        assert_eq!(SquareAtom::new(440.0, SR).audio_outputs(), 1);
        assert_eq!(PulseAtom::new(440.0, 0.5, SR).audio_inputs(), 0);
        assert_eq!(PulseAtom::new(440.0, 0.5, SR).audio_outputs(), 1);
    }
}
