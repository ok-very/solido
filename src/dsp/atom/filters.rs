use super::DspAtom;

/// 4-pole Moog-style ladder filter with tanh saturation between stages.
/// 1 audio input, 1 audio output.
pub struct LadderAtom {
    stage: [f32; 4],
    freq: f32,
    resonance: f32,
    drive: f32,
    sample_rate: f32,
}

impl LadderAtom {
    pub fn new(freq: f32, resonance: f32, sr: f32) -> Self {
        Self {
            stage: [0.0; 4],
            freq,
            resonance,
            drive: 1.0,
            sample_rate: sr,
        }
    }
}

impl DspAtom for LadderAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let f = (std::f32::consts::PI * self.freq / self.sample_rate).clamp(0.001, 0.499);
        let feedback = self.stage[3] * self.resonance * 4.0;
        let x = (input[0] * self.drive - feedback).tanh();

        // 4 cascaded one-pole stages with tanh between
        self.stage[0] += f * (x - self.stage[0]);
        self.stage[0] = self.stage[0].tanh();
        self.stage[1] += f * (self.stage[0] - self.stage[1]);
        self.stage[1] = self.stage[1].tanh();
        self.stage[2] += f * (self.stage[1] - self.stage[2]);
        self.stage[2] = self.stage[2].tanh();
        self.stage[3] += f * (self.stage[2] - self.stage[3]);

        output[0] = self.stage[3];
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "cutoff" => {
                self.freq = value;
                true
            }
            "resonance" => {
                self.resonance = value;
                true
            }
            "drive" => {
                self.drive = value;
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "cutoff" => Some(self.freq),
            "resonance" => Some(self.resonance),
            "drive" => Some(self.drive),
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
        self.stage = [0.0; 4];
    }

    fn name(&self) -> &str {
        "ladder"
    }
}

/// Chamberlin SVF with tanh input drive. Lowpass output.
/// 1 audio input, 1 audio output.
pub struct SvfDriveAtom {
    ic1eq: f32,
    ic2eq: f32,
    cutoff: f32,
    resonance: f32,
    drive: f32,
    sample_rate: f32,
}

impl SvfDriveAtom {
    pub fn new(cutoff: f32, resonance: f32, sr: f32) -> Self {
        Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            cutoff,
            resonance,
            drive: 1.0,
            sample_rate: sr,
        }
    }
}

impl DspAtom for SvfDriveAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let v0 = (input[0] * self.drive).tanh();
        let g = (std::f32::consts::PI * self.cutoff / self.sample_rate)
            .clamp(0.001, 0.499)
            .tan();
        let k = 2.0 - 2.0 * self.resonance.clamp(0.0, 0.99);

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = v0 - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        // Lowpass output
        output[0] = v2;
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "cutoff" => {
                self.cutoff = value;
                true
            }
            "resonance" => {
                self.resonance = value;
                true
            }
            "drive" => {
                self.drive = value;
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "cutoff" => Some(self.cutoff),
            "resonance" => Some(self.resonance),
            "drive" => Some(self.drive),
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
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    fn name(&self) -> &str {
        "svf_drive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::{render_atom_with_input, rms};
    use std::f32::consts::PI;

    const SR: f32 = 44100.0;
    const SAMPLES: usize = 4410;

    fn sine_signal(freq_hz: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * PI * freq_hz * i as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn ladder_attenuates_above_cutoff() {
        let mut atom = LadderAtom::new(1000.0, 0.0, SR);
        let signal = sine_signal(8000.0, SAMPLES * 4);
        let out = render_atom_with_input(&mut atom, &signal);
        let r = rms(&out);
        // 4-pole = 24dB/oct, 8kHz is ~3 octaves above 1kHz → ~72dB attenuation
        assert!(
            r < 0.05,
            "8kHz through 1kHz ladder should be heavily attenuated: rms={r}"
        );
    }

    #[test]
    fn ladder_drive_adds_harmonics() {
        // Compare zero crossings with drive=1 (clean) vs drive=5 (saturated)
        let mut atom_clean = LadderAtom::new(10000.0, 0.0, SR);
        atom_clean.set_param("drive", 1.0);
        let signal = sine_signal(440.0, SAMPLES * 4);
        let out_clean = render_atom_with_input(&mut atom_clean, &signal);
        let zc_clean: usize = out_clean
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();

        let mut atom_driven = LadderAtom::new(10000.0, 0.0, SR);
        atom_driven.set_param("drive", 5.0);
        let out_driven = render_atom_with_input(&mut atom_driven, &signal);
        let zc_driven: usize = out_driven
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();

        assert!(
            zc_driven >= zc_clean,
            "Higher drive should produce >= zero crossings: {zc_driven} vs {zc_clean}"
        );
    }

    #[test]
    fn ladder_no_inf_nan() {
        let mut atom = LadderAtom::new(2000.0, 0.9, SR);
        atom.set_param("drive", 2.0);
        let signal = sine_signal(440.0, 44100 * 10); // 10 seconds
        let out = render_atom_with_input(&mut atom, &signal);
        for (i, &s) in out.iter().enumerate() {
            assert!(s.is_finite(), "Sample {i} is not finite: {s}");
            assert!(s.abs() <= 4.0, "Sample {i} exceeds bounds: {s}");
        }
    }

    #[test]
    fn svf_drive_passes_low_attenuates_high() {
        let mut atom = SvfDriveAtom::new(2000.0, 0.0, SR);
        let low = sine_signal(200.0, SAMPLES);
        let high = sine_signal(10000.0, SAMPLES);
        let out_low = render_atom_with_input(&mut atom, &low);
        atom.reset();
        let out_high = render_atom_with_input(&mut atom, &high);
        let rms_low = rms(&out_low);
        let rms_high = rms(&out_high);
        assert!(
            rms_low > rms_high * 3.0,
            "SVF should pass low and attenuate high: {rms_low} vs {rms_high}"
        );
    }

    #[test]
    fn svf_drive_bounded_output() {
        let mut atom = SvfDriveAtom::new(2000.0, 0.5, SR);
        atom.set_param("drive", 3.0);
        let signal = sine_signal(440.0, 44100 * 5);
        let out = render_atom_with_input(&mut atom, &signal);
        for (i, &s) in out.iter().enumerate() {
            assert!(s.is_finite(), "SVF sample {i} is not finite: {s}");
            assert!(
                s.abs() <= 4.0,
                "SVF sample {i} exceeds bounds: {s}"
            );
        }
    }
}
