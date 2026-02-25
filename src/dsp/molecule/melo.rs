use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

use super::{build_scratch, ModRoute, Molecule};
use crate::dsp::atom::DspAtom;

/// Oscillator pair: pulse + sub sine, fused.
/// Fused: `((var(&freq) | var(&pw)) >> pulse()) & (var(&freq_sub) >> sine())`
/// Params: freq, freq_sub, pulse_width. 0→1.
pub fn osc_pair(freq_hz: f32, pulse_width: f32, sr: f32) -> Molecule {
    let freq = shared(freq_hz);
    let freq_sub = shared(freq_hz * 0.5);
    let pw = shared(pulse_width);
    let mut unit: Box<dyn AudioUnit> =
        Box::new(((var(&freq) | var(&pw)) >> pulse()) & (var(&freq_sub) >> sine()));
    unit.set_sample_rate(sr as f64);
    unit.allocate();

    Molecule::Fused {
        name: "osc_pair".into(),
        unit,
        params: vec![
            ("freq".into(), freq),
            ("freq_sub".into(), freq_sub),
            ("pulse_width".into(), pw),
        ],
        audio_inputs: 0,
        audio_outputs: 1,
    }
}

/// Filter envelope: ADSR drives lowpass cutoff.
/// Wired: AdsrAtom → LowpassAtom (ADSR output scales cutoff via ParamMod).
/// External audio input goes to lowpass. 1→1.
/// Params: adsr.gate, adsr.a, adsr.d, adsr.s, adsr.r, cutoff_base, cutoff_depth.
pub fn filter_envelope(base_cutoff: f32, depth_hz: f32, sr: f32) -> (Molecule, Shared, Shared) {
    use crate::dsp::atom::envelopes::AdsrAtom;
    use crate::dsp::atom::filters::LowpassAtom;

    let base = shared(base_cutoff);
    let depth = shared(depth_hz);

    let atoms: Vec<(String, Box<dyn DspAtom>)> = vec![
        ("adsr".into(), Box::new(AdsrAtom::new(sr))),
        (
            "filter".into(),
            Box::new(LowpassAtom::new(base_cutoff, 0.707, sr)),
        ),
    ];
    let mod_outputs = vec![0.0f32; atoms.len()];
    let scratch = build_scratch(&atoms);

    let mol = Molecule::Wired {
        name: "filter_envelope".into(),
        atoms,
        wiring: vec![],
        process_order: vec![0, 1],
        scratch,
        external_inputs: vec![(1, 0)], // external audio → filter input
        external_outputs: vec![(1, 0)], // filter output
        mod_routes: vec![ModRoute::ParamMod {
            src_atom: 0,
            dst_atom: 1,
            dst_param: "cutoff".into(),
            base: base.clone(),
            depth: depth.clone(),
        }],
        mod_outputs,
    };

    (mol, base, depth)
}

/// Amplitude envelope: ADSR gates audio level via AmpMod route.
/// 1 audio input → 1 output (input * ADSR level).
/// Params: adsr.gate, adsr.a, adsr.d, adsr.s, adsr.r.
pub fn amp_envelope(sr: f32) -> Molecule {
    use crate::dsp::atom::envelopes::AdsrAtom;

    let atoms: Vec<(String, Box<dyn DspAtom>)> =
        vec![("adsr".into(), Box::new(AdsrAtom::new(sr)))];
    let mod_outputs = vec![0.0f32; atoms.len()];
    let scratch = build_scratch(&atoms);

    Molecule::Wired {
        name: "amp_envelope".into(),
        atoms,
        wiring: vec![],
        process_order: vec![0],
        scratch,
        external_inputs: vec![],
        external_outputs: vec![],
        mod_routes: vec![ModRoute::AmpMod { src_atom: 0 }],
        mod_outputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::rms;

    const SR: f32 = 44100.0;

    #[test]
    fn osc_pair_produces_audio() {
        let mut mol = osc_pair(440.0, 0.5, SR);
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
        let mut mol = osc_pair(440.0, 0.5, SR);
        assert!(mol.set_param("freq", 880.0));
        assert!((mol.get_param("freq").unwrap() - 880.0).abs() < 0.01);
        assert!(mol.set_param("freq_sub", 220.0));
    }

    #[test]
    fn filter_envelope_modulates() {
        let (mut mol, _base, _depth) = filter_envelope(200.0, 5000.0, SR);

        // Gate on
        mol.set_param("adsr.gate", 1.0);

        // Feed a rich signal (saw-like)
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for i in 0..4410 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin();
            mol.tick(&[input], &mut out);
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
        let (mut mol, _, _) = filter_envelope(200.0, 5000.0, SR);
        assert!(mol.set_param("adsr.gate", 1.0));
        assert!((mol.get_param("adsr.gate").unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn amp_envelope_gates_audio() {
        let mut mol = amp_envelope(SR);

        // Without gate, should be silent
        let mut out = [0.0f32];
        for _ in 0..100 {
            mol.tick(&[0.5], &mut out);
        }
        assert!(
            out[0].abs() < 0.01,
            "Without gate, amp envelope should be silent"
        );

        // Gate on
        mol.set_param("adsr.gate", 1.0);
        for _ in 0..500 {
            mol.tick(&[0.5], &mut out);
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
            mol.tick(&[0.5], &mut out);
        }
        assert!(out[0] > 0.1, "At sustain, should pass audio");

        // Gate off
        mol.set_param("adsr.gate", 0.0);
        // Run through release
        for _ in 0..15000 {
            mol.tick(&[0.5], &mut out);
        }
        assert!(
            out[0].abs() < 0.01,
            "After release, should be silent: {}",
            out[0]
        );
    }
}
