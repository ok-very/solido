use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

use super::{build_scratch, Molecule};
use crate::dsp::atom::DspAtom;

/// Oscillator pair: square + sub sine, fused.
/// Fused: `(var(&freq) >> square()) & (var(&freq_sub) >> sine())`
/// Params: freq, freq_sub. 0→1.
pub fn osc_pair(freq_hz: f32, sr: f32) -> Molecule {
    let freq = shared(freq_hz);
    let freq_sub = shared(freq_hz * 0.5);
    let mut unit: Box<dyn AudioUnit> =
        Box::new((var(&freq) >> square()) & (var(&freq_sub) >> sine()));
    unit.set_sample_rate(sr as f64);
    unit.allocate();

    Molecule::Fused {
        name: "osc_pair".into(),
        unit,
        params: vec![("freq".into(), freq), ("freq_sub".into(), freq_sub)],
        audio_inputs: 0,
        audio_outputs: 1,
    }
}

/// Filter envelope: ADSR drives lowpass cutoff.
/// Wired: AdsrAtom → LowpassAtom (ADSR output scales cutoff).
/// External audio input goes to lowpass. 1→1.
/// Params: adsr.gate, adsr.a, adsr.d, adsr.s, adsr.r, cutoff (base), depth.
pub fn filter_envelope(base_cutoff: f32, depth: f32, sr: f32) -> Molecule {
    use crate::dsp::atom::envelopes::AdsrAtom;
    use crate::dsp::atom::filters::LowpassAtom;

    let atoms: Vec<(String, Box<dyn DspAtom>)> = vec![
        ("adsr".into(), Box::new(AdsrAtom::new(sr))),
        (
            "filter".into(),
            Box::new(LowpassAtom::new(base_cutoff, 0.707, sr)),
        ),
    ];
    let scratch = build_scratch(&atoms);

    // Store base_cutoff and depth as metadata in the molecule name for now.
    // The tick_filter_envelope function handles the ADSR→cutoff modulation.
    Molecule::Wired {
        name: "filter_envelope".into(),
        atoms,
        wiring: vec![], // ADSR modulates cutoff param, not audio wiring
        process_order: vec![0, 1],
        scratch,
        external_inputs: vec![(1, 0)], // external audio → filter input
        external_outputs: vec![(1, 0)], // filter output
    }
}

/// Custom tick for filter_envelope: ADSR envelope scales filter cutoff.
pub fn tick_filter_envelope(
    mol: &mut Molecule,
    input: &[f32],
    output: &mut [f32],
    base_cutoff: f32,
    depth: f32,
) {
    if let Molecule::Wired { atoms, .. } = mol {
        // Tick ADSR
        let mut env_out = [0.0f32];
        atoms[0].1.tick(&[], &mut env_out);

        // Modulate filter cutoff: base + env * depth
        let cutoff = base_cutoff + env_out[0] * depth;
        atoms[1].1.set_param("cutoff", cutoff);

        // Tick filter with external audio
        atoms[1].1.tick(input, output);
    }
}

/// Amplitude envelope: ADSR gates audio level.
/// Wired: AdsrAtom provides amplitude multiplier for audio pass-through.
/// 1 audio input → 1 output.
/// Params: adsr.gate, adsr.a, adsr.d, adsr.s, adsr.r.
pub fn amp_envelope(sr: f32) -> Molecule {
    use crate::dsp::atom::envelopes::AdsrAtom;

    let atoms: Vec<(String, Box<dyn DspAtom>)> =
        vec![("adsr".into(), Box::new(AdsrAtom::new(sr)))];
    let scratch = build_scratch(&atoms);

    Molecule::Wired {
        name: "amp_envelope".into(),
        atoms,
        wiring: vec![],
        process_order: vec![0],
        scratch,
        external_inputs: vec![], // audio is multiplied, not routed through an atom
        external_outputs: vec![],
    }
}

/// Custom tick for amp_envelope: ADSR scales audio amplitude.
pub fn tick_amp_envelope(mol: &mut Molecule, input: &[f32], output: &mut [f32]) {
    if let Molecule::Wired { atoms, .. } = mol {
        let mut env_out = [0.0f32];
        atoms[0].1.tick(&[], &mut env_out);
        output[0] = input[0] * env_out[0];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::rms;

    const SR: f32 = 44100.0;

    #[test]
    fn osc_pair_produces_audio() {
        let mut mol = osc_pair(440.0, SR);
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            mol.tick(&[], &mut out);
            buf.push(out[0]);
        }
        let r = rms(&buf);
        assert!(r > 0.1, "Osc pair should produce audio: rms={r}");
    }

    #[test]
    fn osc_pair_freq_param() {
        let mut mol = osc_pair(440.0, SR);
        assert!(mol.set_param("freq", 880.0));
        assert!((mol.get_param("freq").unwrap() - 880.0).abs() < 0.01);
        assert!(mol.set_param("freq_sub", 220.0));
    }

    #[test]
    fn filter_envelope_modulates() {
        let mut mol = filter_envelope(200.0, 5000.0, SR);
        let base = 200.0f32;
        let depth = 5000.0f32;

        // Gate on
        mol.set_param("adsr.gate", 1.0);

        // Feed a rich signal (saw-like)
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for i in 0..4410 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin();
            tick_filter_envelope(&mut mol, &[input], &mut out, base, depth);
            buf.push(out[0]);
        }
        let r = rms(&buf);
        assert!(
            r > 0.01,
            "Filter envelope with gate on should pass audio: rms={r}"
        );
    }

    #[test]
    fn filter_envelope_gate_param() {
        let mut mol = filter_envelope(200.0, 5000.0, SR);
        assert!(mol.set_param("adsr.gate", 1.0));
        assert!((mol.get_param("adsr.gate").unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn amp_envelope_gates_audio() {
        let mut mol = amp_envelope(SR);

        // Without gate, should be silent
        let mut out = [0.0f32];
        for _ in 0..100 {
            tick_amp_envelope(&mut mol, &[0.5], &mut out);
        }
        assert!(
            out[0].abs() < 0.01,
            "Without gate, amp envelope should be silent"
        );

        // Gate on
        mol.set_param("adsr.gate", 1.0);
        for _ in 0..500 {
            tick_amp_envelope(&mut mol, &[0.5], &mut out);
        }
        assert!(
            out[0].abs() > 0.1,
            "With gate on, amp envelope should pass audio: {}",
            out[0]
        );
    }

    #[test]
    fn amp_envelope_release() {
        let mut mol = amp_envelope(SR);
        mol.set_param("adsr.gate", 1.0);
        let mut out = [0.0f32];
        // Run through attack+decay to sustain
        for _ in 0..6000 {
            tick_amp_envelope(&mut mol, &[0.5], &mut out);
        }
        assert!(out[0] > 0.1, "At sustain, should pass audio");

        // Gate off
        mol.set_param("adsr.gate", 0.0);
        // Run through release
        for _ in 0..15000 {
            tick_amp_envelope(&mut mol, &[0.5], &mut out);
        }
        assert!(
            out[0].abs() < 0.01,
            "After release, should be silent: {}",
            out[0]
        );
    }
}
