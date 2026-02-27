use super::DspAtom;

/// Waveform for UnisonOscAtom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnisonWave {
    Sine,
    Triangle,
    Saw,
    Square,
}

/// Unison oscillator: up to 5 detuned voices + sub oscillator. Custom DSP.
/// 0 inputs, 1 output.
pub struct UnisonOscAtom {
    phases: [f64; 5],
    wave: UnisonWave,
    freq: f32,
    detune_cents: f32,
    unison: usize,
    sub_phase: f64,
    sub_mix: f32,
    sample_rate: f64,
}

impl UnisonOscAtom {
    pub fn new(freq: f32, wave: UnisonWave, sr: f32) -> Self {
        Self {
            phases: [0.0; 5],
            wave,
            freq,
            detune_cents: 7.0,
            unison: 3,
            sub_phase: 0.0,
            sub_mix: 0.2,
            sample_rate: sr as f64,
        }
    }

    fn wave_sample(phase: f64, wave: UnisonWave) -> f64 {
        match wave {
            UnisonWave::Sine => (phase * std::f64::consts::TAU).sin(),
            UnisonWave::Triangle => {
                let t = phase.fract();
                if t < 0.25 {
                    t * 4.0
                } else if t < 0.75 {
                    2.0 - t * 4.0
                } else {
                    t * 4.0 - 4.0
                }
            }
            UnisonWave::Saw => {
                let t = phase.fract();
                2.0 * t - 1.0
            }
            UnisonWave::Square => {
                if phase.fract() < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

impl DspAtom for UnisonOscAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let n = Ord::max(Ord::min(self.unison, 5), 1);
        let cents_ratio = 2.0f64.powf(self.detune_cents as f64 / 1200.0);
        let freq_base = self.freq as f64;

        let mut sum = 0.0f64;
        for i in 0..n {
            // Symmetric spread: voice i gets spread = (i as centered) / (n-1)
            let spread = if n == 1 {
                0.0
            } else {
                (i as f64 / (n - 1) as f64) * 2.0 - 1.0 // -1..+1
            };
            let voice_freq = freq_base * cents_ratio.powf(spread);
            let inc = voice_freq / self.sample_rate;
            self.phases[i] += inc;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            sum += Self::wave_sample(self.phases[i], self.wave);
        }
        // Normalize by sqrt(n) to keep energy consistent
        sum /= (n as f64).sqrt();

        // Sub oscillator (sine, one octave down)
        let sub_freq = freq_base * 0.5;
        let sub_inc = sub_freq / self.sample_rate;
        self.sub_phase += sub_inc;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }
        let sub = (self.sub_phase * std::f64::consts::TAU).sin();

        let out = sum * (1.0 - self.sub_mix as f64) + sub * self.sub_mix as f64;
        output[0] = out as f32;
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq = value;
                true
            }
            "detune_cents" => {
                self.detune_cents = value;
                true
            }
            "unison" => {
                self.unison = Ord::max(Ord::min(value as usize, 5), 1);
                true
            }
            "sub_mix" => {
                self.sub_mix = value;
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq),
            "detune_cents" => Some(self.detune_cents),
            "unison" => Some(self.unison as f32),
            "sub_mix" => Some(self.sub_mix),
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
        self.phases = [0.0; 5];
        self.sub_phase = 0.0;
    }

    fn name(&self) -> &str {
        "unison_osc"
    }
}

/// FM oscillator: sine carrier + selectable modulator (sine/square) + sub oscillator.
/// 0 inputs, 1 output.
pub struct FmOscAtom {
    carrier_phase: f64,
    mod_phase: f64,
    freq: f32,
    fm_ratio: f32,
    fm_index: f32,
    mod_square: bool,
    sub_phase: f64,
    sub_mix: f32,
    sample_rate: f64,
}

impl FmOscAtom {
    pub fn new(freq: f32, sr: f32) -> Self {
        Self {
            carrier_phase: 0.0,
            mod_phase: 0.0,
            freq,
            fm_ratio: 2.0,
            fm_index: 1.0,
            mod_square: false,
            sub_phase: 0.0,
            sub_mix: 0.2,
            sample_rate: sr as f64,
        }
    }
}

impl DspAtom for FmOscAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let freq = self.freq as f64;
        let mod_freq = freq * self.fm_ratio as f64;

        // Advance modulator phase
        self.mod_phase += mod_freq / self.sample_rate;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= self.mod_phase.floor();
        }

        // Modulator output
        let mod_out = if self.mod_square {
            if self.mod_phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        } else {
            (self.mod_phase * std::f64::consts::TAU).sin()
        };

        // FM: carrier frequency is modulated
        let fm_deviation = mod_out * self.fm_index as f64 * mod_freq;
        let carrier_freq = freq + fm_deviation;
        self.carrier_phase += carrier_freq / self.sample_rate;
        if self.carrier_phase >= 1.0 {
            self.carrier_phase -= self.carrier_phase.floor();
        }

        let carrier = (self.carrier_phase * std::f64::consts::TAU).sin();

        // Sub oscillator (sine, one octave down)
        let sub_freq = freq * 0.5;
        self.sub_phase += sub_freq / self.sample_rate;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= self.sub_phase.floor();
        }
        let sub = (self.sub_phase * std::f64::consts::TAU).sin();

        let out = carrier * (1.0 - self.sub_mix as f64) + sub * self.sub_mix as f64;
        output[0] = out as f32;
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq" => {
                self.freq = value;
                true
            }
            "fm_ratio" => {
                self.fm_ratio = value;
                true
            }
            "fm_index" => {
                self.fm_index = value;
                true
            }
            "mod_square" => {
                self.mod_square = value > 0.5;
                true
            }
            "sub_mix" => {
                self.sub_mix = value;
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq" => Some(self.freq),
            "fm_ratio" => Some(self.fm_ratio),
            "fm_index" => Some(self.fm_index),
            "mod_square" => Some(if self.mod_square { 1.0 } else { 0.0 }),
            "sub_mix" => Some(self.sub_mix),
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
        self.carrier_phase = 0.0;
        self.mod_phase = 0.0;
        self.sub_phase = 0.0;
    }

    fn name(&self) -> &str {
        "fm_osc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::{render_atom, rms, zero_crossings};

    const SR: f32 = 44100.0;
    const SAMPLES: usize = 4410; // 100ms

    #[test]
    fn all_oscillators_0_inputs_1_output() {
        assert_eq!(UnisonOscAtom::new(440.0, UnisonWave::Sine, SR).audio_inputs(), 0);
        assert_eq!(UnisonOscAtom::new(440.0, UnisonWave::Sine, SR).audio_outputs(), 1);
        assert_eq!(FmOscAtom::new(440.0, SR).audio_inputs(), 0);
        assert_eq!(FmOscAtom::new(440.0, SR).audio_outputs(), 1);
    }

    #[test]
    fn unison_1_matches_single_sine() {
        // Unison=1, detune=0, sine wave should approximate a manual sine at 440Hz
        let mut unison = UnisonOscAtom::new(440.0, UnisonWave::Sine, SR);
        unison.set_param("unison", 1.0);
        unison.set_param("detune_cents", 0.0);
        unison.set_param("sub_mix", 0.0);
        let buf_unison = render_atom(&mut unison, SAMPLES);
        let rms_unison = rms(&buf_unison);

        // Manual sine reference
        let rms_sine = {
            let buf: Vec<f32> = (0..SAMPLES)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin())
                .collect();
            rms(&buf)
        };

        let ratio = rms_unison / rms_sine;
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "Unison 1 sine should match single sine RMS within 15%: ratio={ratio}"
        );
    }

    #[test]
    fn unison_3_no_clipping() {
        let mut atom = UnisonOscAtom::new(440.0, UnisonWave::Saw, SR);
        atom.set_param("unison", 3.0);
        atom.set_param("detune_cents", 15.0);
        atom.set_param("sub_mix", 0.2);
        let buf = render_atom(&mut atom, SAMPLES * 10);
        let peak: f32 = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            peak <= 1.5,
            "Unison 3 peak should be <= 1.5, got {peak}"
        );
    }

    #[test]
    fn detune_creates_beating() {
        // 2 voices with detune should create amplitude variation (beating)
        let mut atom = UnisonOscAtom::new(440.0, UnisonWave::Sine, SR);
        atom.set_param("unison", 2.0);
        atom.set_param("detune_cents", 7.0);
        atom.set_param("sub_mix", 0.0);

        let buf = render_atom(&mut atom, 44100); // 1 second
        // Check for amplitude variation by comparing RMS of first and second halves
        let chunk_size = 2205; // 50ms chunks
        let chunks: Vec<f32> = buf
            .chunks(chunk_size)
            .map(|c| rms(c))
            .collect();
        let min_rms = chunks.iter().copied().fold(f32::MAX, f32::min);
        let max_rms = chunks.iter().copied().fold(0.0f32, f32::max);
        assert!(
            max_rms - min_rms > 0.05,
            "Detuned unison should create beating/amplitude variation: min={min_rms}, max={max_rms}"
        );
    }

    #[test]
    fn fm_index_0_is_pure_sine() {
        let mut fm = FmOscAtom::new(440.0, SR);
        fm.set_param("fm_index", 0.0);
        fm.set_param("sub_mix", 0.0);
        let buf_fm = render_atom(&mut fm, SAMPLES);
        let rms_fm = rms(&buf_fm);

        // Manual sine reference
        let rms_sine = {
            let buf: Vec<f32> = (0..SAMPLES)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin())
                .collect();
            rms(&buf)
        };

        let ratio = rms_fm / rms_sine;
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "FM with index=0 should match sine RMS within 15%: ratio={ratio}"
        );
    }

    #[test]
    fn fm_index_increases_brightness() {
        // Higher FM index → more harmonics → more zero crossings
        let mut fm_low = FmOscAtom::new(440.0, SR);
        fm_low.set_param("fm_index", 0.0);
        fm_low.set_param("sub_mix", 0.0);
        let buf_low = render_atom(&mut fm_low, SAMPLES);
        let zc_low = zero_crossings(&buf_low);

        let mut fm_high = FmOscAtom::new(440.0, SR);
        fm_high.set_param("fm_index", 5.0);
        fm_high.set_param("sub_mix", 0.0);
        let buf_high = render_atom(&mut fm_high, SAMPLES);
        let zc_high = zero_crossings(&buf_high);

        assert!(
            zc_high > zc_low,
            "Higher FM index should produce more zero crossings: {zc_high} vs {zc_low}"
        );
    }
}
