pub mod dron;
pub mod melo;
pub mod tblk;

use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::Shared;

use super::atom::DspAtom;

/// A molecule is a small fixed-wired combination of atoms or a fused FunDSP graph.
///
/// Two variants:
/// - **Fused**: entire molecule is one FunDSP AudioUnit with Shared param handles.
///   Best performance — single `tick()` call processes the whole chain.
/// - **Wired**: individual `DspAtom` objects with explicit routing and scratch buffers.
///   For molecules needing custom atoms (AdsrAtom, ClockAtom, GateAtom).
pub enum Molecule {
    Fused {
        name: String,
        unit: Box<dyn AudioUnit>,
        params: Vec<(String, Shared)>,
        audio_inputs: usize,
        audio_outputs: usize,
    },
    Wired {
        name: String,
        atoms: Vec<(String, Box<dyn DspAtom>)>,
        /// (src_atom_idx, src_ch, dst_atom_idx, dst_ch)
        wiring: Vec<(usize, usize, usize, usize)>,
        /// Topological processing order, computed once at construction.
        process_order: Vec<usize>,
        /// Per-atom scratch output buffers.
        scratch: Vec<Vec<f32>>,
        /// Which (atom_idx, channel) receive external inputs.
        external_inputs: Vec<(usize, usize)>,
        /// Which (atom_idx, channel) provide external outputs.
        external_outputs: Vec<(usize, usize)>,
    },
}

impl Molecule {
    pub fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        match self {
            Molecule::Fused { unit, .. } => {
                unit.tick(input, output);
            }
            Molecule::Wired {
                atoms,
                wiring,
                process_order,
                scratch,
                external_inputs,
                external_outputs,
                ..
            } => {
                // Clear scratch buffers
                for buf in scratch.iter_mut() {
                    for s in buf.iter_mut() {
                        *s = 0.0;
                    }
                }

                // Map external inputs into scratch buffers
                for (i, &(atom_idx, ch)) in external_inputs.iter().enumerate() {
                    if i < input.len() {
                        scratch[atom_idx][ch] = input[i];
                    }
                }

                // Process atoms in topological order
                for &atom_idx in process_order.iter() {
                    let n_in = atoms[atom_idx].1.audio_inputs();
                    let n_out = atoms[atom_idx].1.audio_outputs();

                    // Build input buffer from scratch (already has external + wired inputs)
                    let mut atom_input = vec![0.0f32; n_in];
                    for j in 0..n_in {
                        atom_input[j] = scratch[atom_idx][j];
                    }

                    // Tick atom
                    let mut atom_output = vec![0.0f32; n_out];
                    atoms[atom_idx].1.tick(&atom_input, &mut atom_output);

                    // Store output in scratch for downstream wiring
                    // Use indices beyond the input slots to store outputs
                    let out_base = n_in;
                    for j in 0..n_out {
                        if out_base + j < scratch[atom_idx].len() {
                            scratch[atom_idx][out_base + j] = atom_output[j];
                        }
                    }

                    // Route outputs to downstream atoms' input slots
                    for &(src, src_ch, dst, dst_ch) in wiring.iter() {
                        if src == atom_idx && src_ch < n_out {
                            scratch[dst][dst_ch] = atom_output[src_ch];
                        }
                    }
                }

                // Map outputs
                for (i, &(atom_idx, ch)) in external_outputs.iter().enumerate() {
                    if i < output.len() {
                        let n_in = atoms[atom_idx].1.audio_inputs();
                        let out_base = n_in;
                        output[i] = scratch[atom_idx][out_base + ch];
                    }
                }
            }
        }
    }

    pub fn set_param(&mut self, name: &str, value: f32) -> bool {
        match self {
            Molecule::Fused { params, .. } => {
                for (pname, shared) in params.iter() {
                    if pname == name {
                        shared.set(value);
                        return true;
                    }
                }
                false
            }
            Molecule::Wired { atoms, .. } => {
                // Try "atom_name.param_name" format first
                if let Some(dot) = name.find('.') {
                    let (atom_name, param_name) = name.split_at(dot);
                    let param_name = &param_name[1..]; // skip the dot
                    for (aname, atom) in atoms.iter_mut() {
                        if aname == atom_name {
                            return atom.set_param(param_name, value);
                        }
                    }
                }
                // Fall through: try each atom
                for (_aname, atom) in atoms.iter_mut() {
                    if atom.set_param(name, value) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn get_param(&self, name: &str) -> Option<f32> {
        match self {
            Molecule::Fused { params, .. } => {
                for (pname, shared) in params.iter() {
                    if pname == name {
                        return Some(shared.value());
                    }
                }
                None
            }
            Molecule::Wired { atoms, .. } => {
                if let Some(dot) = name.find('.') {
                    let (atom_name, param_name) = name.split_at(dot);
                    let param_name = &param_name[1..];
                    for (aname, atom) in atoms.iter() {
                        if aname == atom_name {
                            return atom.get_param(param_name);
                        }
                    }
                }
                for (_aname, atom) in atoms.iter() {
                    if let Some(v) = atom.get_param(name) {
                        return Some(v);
                    }
                }
                None
            }
        }
    }

    pub fn audio_inputs(&self) -> usize {
        match self {
            Molecule::Fused { audio_inputs, .. } => *audio_inputs,
            Molecule::Wired {
                external_inputs, ..
            } => external_inputs.len(),
        }
    }

    pub fn audio_outputs(&self) -> usize {
        match self {
            Molecule::Fused { audio_outputs, .. } => *audio_outputs,
            Molecule::Wired {
                external_outputs, ..
            } => external_outputs.len(),
        }
    }

    pub fn reset(&mut self) {
        match self {
            Molecule::Fused { unit, .. } => unit.reset(),
            Molecule::Wired { atoms, .. } => {
                for (_name, atom) in atoms.iter_mut() {
                    atom.reset();
                }
            }
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Molecule::Fused { name, .. } | Molecule::Wired { name, .. } => name,
        }
    }
}

/// Helper to build a Wired molecule's scratch buffers.
/// Each atom gets max(audio_inputs, audio_inputs + audio_outputs) slots.
pub fn build_scratch(atoms: &[(String, Box<dyn DspAtom>)]) -> Vec<Vec<f32>> {
    atoms
        .iter()
        .map(|(_, atom)| vec![0.0f32; atom.audio_inputs() + atom.audio_outputs()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::rms;
    use fundsp::prelude32::*;

    const SR: f32 = 44100.0;

    #[test]
    fn fused_chain_produces_audio() {
        // sine → lowpass fused graph
        let freq = shared(440.0);
        let cutoff = shared(2000.0);
        let q = shared(0.707);
        let mut unit: Box<dyn AudioUnit> = Box::new(
            var(&freq) >> sine() >> (pass() | var(&cutoff) | var(&q)) >> lowpass(),
        );
        unit.set_sample_rate(SR as f64);
        unit.allocate();

        let mut mol = Molecule::Fused {
            name: "test_fused".into(),
            unit,
            params: vec![
                ("freq".into(), freq),
                ("cutoff".into(), cutoff),
                ("q".into(), q),
            ],
            audio_inputs: 0,
            audio_outputs: 1,
        };

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            mol.tick(&[], &mut out);
            buf.push(out[0]);
        }
        assert!(rms(&buf) > 0.1, "Fused molecule should produce audio");
    }

    #[test]
    fn fused_param_forwarding() {
        let freq = shared(440.0);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> sine());
        unit.set_sample_rate(SR as f64);
        unit.allocate();

        let mut mol = Molecule::Fused {
            name: "test_param".into(),
            unit,
            params: vec![("freq".into(), freq)],
            audio_inputs: 0,
            audio_outputs: 1,
        };

        assert!(mol.set_param("freq", 880.0));
        assert!((mol.get_param("freq").unwrap() - 880.0).abs() < 0.01);
        assert!(!mol.set_param("nonexistent", 1.0));
    }

    #[test]
    fn wired_chain_produces_audio() {
        use crate::dsp::atom::oscillators::SineAtom;
        use crate::dsp::atom::filters::LowpassAtom;

        let atoms: Vec<(String, Box<dyn DspAtom>)> = vec![
            ("osc".into(), Box::new(SineAtom::new(440.0, SR))),
            ("filt".into(), Box::new(LowpassAtom::new(2000.0, 0.707, SR))),
        ];
        let scratch = build_scratch(&atoms);
        let mut mol = Molecule::Wired {
            name: "test_wired".into(),
            atoms,
            wiring: vec![(0, 0, 1, 0)], // osc output ch0 → filter input ch0
            process_order: vec![0, 1],
            scratch,
            external_inputs: vec![],
            external_outputs: vec![(1, 0)], // filter output ch0
        };

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            mol.tick(&[], &mut out);
            buf.push(out[0]);
        }
        assert!(rms(&buf) > 0.1, "Wired molecule should produce audio");
    }

    #[test]
    fn wired_param_forwarding() {
        use crate::dsp::atom::oscillators::SineAtom;
        use crate::dsp::atom::filters::LowpassAtom;

        let atoms: Vec<(String, Box<dyn DspAtom>)> = vec![
            ("osc".into(), Box::new(SineAtom::new(440.0, SR))),
            ("filt".into(), Box::new(LowpassAtom::new(2000.0, 0.707, SR))),
        ];
        let scratch = build_scratch(&atoms);
        let mut mol = Molecule::Wired {
            name: "test_wired_param".into(),
            atoms,
            wiring: vec![(0, 0, 1, 0)],
            process_order: vec![0, 1],
            scratch,
            external_inputs: vec![],
            external_outputs: vec![(1, 0)],
        };

        // Dotted name
        assert!(mol.set_param("osc.freq", 880.0));
        assert!((mol.get_param("osc.freq").unwrap() - 880.0).abs() < 0.01);

        // Fallthrough name
        assert!(mol.set_param("cutoff", 1000.0));
        assert!((mol.get_param("cutoff").unwrap() - 1000.0).abs() < 0.01);
    }

    #[test]
    fn molecule_reset() {
        let freq = shared(440.0);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> sine());
        unit.set_sample_rate(SR as f64);
        unit.allocate();

        let mut mol = Molecule::Fused {
            name: "test_reset".into(),
            unit,
            params: vec![("freq".into(), freq)],
            audio_inputs: 0,
            audio_outputs: 1,
        };

        let mut out = [0.0f32];
        for _ in 0..100 {
            mol.tick(&[], &mut out);
        }
        mol.reset();
        // Should not panic and continue producing audio
        for _ in 0..100 {
            mol.tick(&[], &mut out);
        }
    }

    #[test]
    fn molecule_io_counts() {
        let freq = shared(440.0);
        let mut unit: Box<dyn AudioUnit> = Box::new(var(&freq) >> sine());
        unit.set_sample_rate(SR as f64);
        unit.allocate();

        let mol = Molecule::Fused {
            name: "test_io".into(),
            unit,
            params: vec![],
            audio_inputs: 0,
            audio_outputs: 1,
        };
        assert_eq!(mol.audio_inputs(), 0);
        assert_eq!(mol.audio_outputs(), 1);
        assert_eq!(mol.name(), "test_io");
    }
}
