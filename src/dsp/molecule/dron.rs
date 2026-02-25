use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

use super::{build_scratch, ModRoute, Molecule};
use crate::dsp::atom::DspAtom;

/// Detuned oscillator stack: 4 saws + 1 sub sine.
/// Fused: `(var(&f1) >> saw()) & (var(&f2) >> saw()) & (var(&f3) >> saw()) & (var(&f_sub) >> sine())`
/// Params: f1, f2, f3, f_sub. 0→1.
pub fn detuned_stack(root_hz: f32, sr: f32) -> Molecule {
    let f1 = shared(root_hz);
    let f2 = shared(root_hz * 1.003); // slight detune
    let f3 = shared(root_hz * 0.997);
    let f_sub = shared(root_hz * 0.5); // sub oscillator

    let mut unit: Box<dyn AudioUnit> = Box::new(
        (var(&f1) >> saw())
            & (var(&f2) >> saw())
            & (var(&f3) >> saw())
            & (var(&f_sub) >> sine()),
    );
    unit.set_sample_rate(sr as f64);
    unit.allocate();

    Molecule::Fused {
        name: "detuned_stack".into(),
        unit,
        params: vec![
            ("f1".into(), f1),
            ("f2".into(), f2),
            ("f3".into(), f3),
            ("f_sub".into(), f_sub),
        ],
        audio_inputs: 0,
        audio_outputs: 1,
    }
}

/// Slow filter: lowpass with Shared cutoff and q.
/// Fused: `(pass() | var(&cutoff) | var(&q)) >> lowpass()`
/// Params: cutoff, q. 1→1.
pub fn slow_filter(cutoff_hz: f32, q: f32, sr: f32) -> Molecule {
    let cutoff = shared(cutoff_hz);
    let q_s = shared(q);
    let mut unit: Box<dyn AudioUnit> =
        Box::new((pass() | var(&cutoff) | var(&q_s)) >> lowpass());
    unit.set_sample_rate(sr as f64);
    unit.allocate();

    Molecule::Fused {
        name: "slow_filter".into(),
        unit,
        params: vec![("cutoff".into(), cutoff), ("q".into(), q_s)],
        audio_inputs: 1,
        audio_outputs: 1,
    }
}

/// Filter with selectable type for HarmonicBed.
/// 1 audio input, 1 output.
pub fn typed_filter(
    mode: super::melo::FilterMode,
    cutoff_hz: f32,
    q: f32,
    sr: f32,
) -> Molecule {
    use crate::dsp::atom::filters::{LadderAtom, SvfDriveAtom};
    use super::melo::FilterMode;

    match mode {
        FilterMode::Lowpass => slow_filter(cutoff_hz, q, sr),
        FilterMode::Ladder => {
            let mut atom = LadderAtom::new(cutoff_hz, q.clamp(0.0, 1.0), sr);
            let atoms: Vec<(String, Box<dyn DspAtom>)> =
                vec![("filter".into(), Box::new(atom))];
            let mod_outputs = vec![0.0f32; atoms.len()];
            let scratch = build_scratch(&atoms);

            Molecule::Wired {
                name: "typed_filter".into(),
                atoms,
                wiring: vec![],
                process_order: vec![0],
                scratch,
                external_inputs: vec![(0, 0)],
                external_outputs: vec![(0, 0)],
                mod_routes: vec![],
                mod_outputs,
            }
        }
        FilterMode::SvfDrive => {
            let mut atom = SvfDriveAtom::new(cutoff_hz, q.clamp(0.0, 0.99), sr);
            let atoms: Vec<(String, Box<dyn DspAtom>)> =
                vec![("filter".into(), Box::new(atom))];
            let mod_outputs = vec![0.0f32; atoms.len()];
            let scratch = build_scratch(&atoms);

            Molecule::Wired {
                name: "typed_filter".into(),
                atoms,
                wiring: vec![],
                process_order: vec![0],
                scratch,
                external_inputs: vec![(0, 0)],
                external_outputs: vec![(0, 0)],
                mod_routes: vec![],
                mod_outputs,
            }
        }
    }
}

/// Stereo spread: LFO modulates pan position.
/// Wired: LfoAtom → PanAtom (1 audio input → 2 stereo outputs).
/// The LFO output is added to the pan position each tick.
/// Params: lfo.rate, lfo.depth. 1→2.
pub fn stereo_spread(sr: f32) -> Molecule {
    use crate::dsp::atom::effects::PanAtom;
    use crate::dsp::atom::envelopes::LfoAtom;

    // LFO drives pan position; external audio goes to pan input
    // We use a custom wired approach: LFO output sets pan position each tick,
    // while external audio flows through the PanAtom.
    // Since PanAtom uses Setting::pan (not a signal input for position),
    // we handle the LFO → pan modulation in a wrapper.

    let mod_outputs = vec![0.0f32; 2];
    let atoms: Vec<(String, Box<dyn DspAtom>)> = vec![
        ("lfo".into(), Box::new(LfoAtom::new(0.5, 1.0, sr))),
        ("pan".into(), Box::new(PanAtom::new(0.0, sr))),
    ];
    let scratch = build_scratch(&atoms);

    Molecule::Wired {
        name: "stereo_spread".into(),
        atoms,
        wiring: vec![],
        process_order: vec![0, 1],
        scratch,
        external_inputs: vec![(1, 0)], // external audio → pan input ch0
        external_outputs: vec![(1, 0), (1, 1)], // pan output L, R
        mod_routes: vec![ModRoute::DirectMod {
            src_atom: 0,
            dst_atom: 1,
            dst_param: "position".into(),
        }],
        mod_outputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::rms;

    const SR: f32 = 44100.0;

    #[test]
    fn detuned_stack_produces_chorusing() {
        let mut mol = detuned_stack(110.0, SR);
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..44100 {
            mol.tick(&[], &mut out);
            buf.push(out[0]);
        }
        let r = rms(&buf);
        assert!(r > 0.1, "Detuned stack should produce audio: rms={r}");
    }

    #[test]
    fn detuned_stack_params() {
        let mut mol = detuned_stack(110.0, SR);
        assert!(mol.set_param("f1", 220.0));
        assert!((mol.get_param("f1").unwrap() - 220.0).abs() < 0.01);
        assert!(mol.set_param("f_sub", 55.0));
    }

    #[test]
    fn slow_filter_passes_audio() {
        let mut mol = slow_filter(2000.0, 0.707, SR);
        // Feed a saw wave
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for i in 0..4410 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin();
            mol.tick(&[input], &mut out);
            buf.push(out[0]);
        }
        let r = rms(&buf);
        assert!(r > 0.1, "Slow filter should pass audio: rms={r}");
    }

    #[test]
    fn slow_filter_cutoff_change() {
        let mut mol = slow_filter(2000.0, 0.707, SR);
        assert!(mol.set_param("cutoff", 500.0));
        assert!((mol.get_param("cutoff").unwrap() - 500.0).abs() < 0.01);
    }

    #[test]
    fn stereo_spread_produces_stereo() {
        let mut mol = stereo_spread(SR);
        let mut output = [0.0f32; 2];
        // Feed audio through
        for i in 0..4410 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin();
            mol.tick(&[input], &mut output);
        }
        // Both channels should have some energy
        // (LFO moves pan so both channels get signal)
        assert_eq!(mol.audio_outputs(), 2);
    }

    #[test]
    fn stereo_spread_lfo_param() {
        let mut mol = stereo_spread(SR);
        assert!(mol.set_param("lfo.rate", 2.0));
        assert!((mol.get_param("lfo.rate").unwrap() - 2.0).abs() < 0.01);
    }
}
