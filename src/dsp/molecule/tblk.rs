use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

use super::Molecule;

/// Membrane resonance: noise → bandpass resonator.
/// Fused: `noise() >> (pass() | var(&center) | var(&bw)) >> resonator()`
/// Params: center, bandwidth. 0→1.
pub fn membrane_sim(freq_hz: f32, bw_hz: f32, sr: f32) -> Molecule {
    let center = shared(freq_hz);
    let bw = shared(bw_hz);
    let mut unit: Box<dyn AudioUnit> =
        Box::new(noise() >> (pass() | var(&center) | var(&bw)) >> resonator());
    unit.set_sample_rate(sr as f64);
    unit.allocate();
    Molecule::Fused {
        name: "membrane_sim".into(),
        unit,
        params: vec![("center".into(), center), ("bandwidth".into(), bw)],
        audio_inputs: 0,
        audio_outputs: 1,
    }
}

/// Snap transient: noise burst through lowpole → highpass for click character.
/// Fused: `noise() >> lowpole_hz(8000) >> highpass_hz(3000, 1.5)`
/// No runtime params. 0→1. Amplitude shaping done by StrikeVoice's snap_decay.
pub fn snap_transient(sr: f32) -> Molecule {
    let mut unit: Box<dyn AudioUnit> =
        Box::new(noise() >> lowpole_hz(8000.0) >> highpass_hz(3000.0, 1.5));
    unit.set_sample_rate(sr as f64);
    unit.allocate();
    Molecule::Fused {
        name: "snap_transient".into(),
        unit,
        params: vec![],
        audio_inputs: 0,
        audio_outputs: 1,
    }
}

/// Body resonance: feedback delay with lowpass.
/// Fused: `feedback(delay(t) >> lowpass_hz(2000, 0.5))`
/// 1 audio input → 1 output. delay_time fixed at construction.
pub fn body_resonance(delay_time: f32, sr: f32) -> Molecule {
    let mut unit: Box<dyn AudioUnit> =
        Box::new(feedback(delay(delay_time) >> lowpass_hz(2000.0, 0.5)));
    unit.set_sample_rate(sr as f64);
    unit.allocate();
    Molecule::Fused {
        name: "body_resonance".into(),
        unit,
        params: vec![],
        audio_inputs: 1,
        audio_outputs: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::rms;

    const SR: f32 = 44100.0;

    fn render_molecule(mol: &mut Molecule, samples: usize) -> Vec<f32> {
        let ins = mol.audio_inputs();
        let outs = mol.audio_outputs();
        let input = vec![0.0f32; ins];
        let mut buf = Vec::with_capacity(samples * outs);
        let mut output = vec![0.0f32; outs];
        for _ in 0..samples {
            mol.tick(&input, &mut output);
            buf.extend_from_slice(&output);
        }
        buf
    }

    #[test]
    fn membrane_sim_produces_pitched_noise() {
        let mut mol = membrane_sim(200.0, 50.0, SR);
        let buf = render_molecule(&mut mol, 4410);
        let r = rms(&buf);
        assert!(r > 0.001, "Membrane sim should produce audio: rms={r}");
    }

    #[test]
    fn membrane_sim_param_change() {
        let mut mol = membrane_sim(200.0, 50.0, SR);
        assert!(mol.set_param("center", 500.0));
        assert!((mol.get_param("center").unwrap() - 500.0).abs() < 0.01);
    }

    #[test]
    fn snap_transient_produces_audio() {
        let mut mol = snap_transient(SR);
        let buf = render_molecule(&mut mol, 4410);
        // Snap transient is a quick burst — check first few hundred samples
        let end = if buf.len() < 500 { buf.len() } else { 500 };
        let r = rms(&buf[..end]);
        assert!(r > 0.001, "Snap transient should produce audio: rms={r}");
    }

    #[test]
    fn body_resonance_passes_audio() {
        let mut mol = body_resonance(0.005, SR);
        // Feed an impulse
        let mut out = [0.0f32];
        mol.tick(&[1.0], &mut out);
        let mut buf = vec![out[0]];
        for _ in 0..4409 {
            mol.tick(&[0.0], &mut out);
            buf.push(out[0]);
        }
        let r = rms(&buf);
        assert!(
            r > 0.001,
            "Body resonance fed impulse should resonate: rms={r}"
        );
    }
}
