use crate::dsp::shared::Shared;

use crate::dsp::cell::hexo_cell::HexoCellBuilder;
use crate::dsp::cell::{param_or, DspCell};
use crate::organism::dna::CellDna;

/// Plate reverb cell backed by HexoDSP's PVerb (Dattorro algorithm).
///
/// Graph: Inp(sig1) → PVerb(in_l, sig_l) → Out(ch1)
/// Mono in → mono out (PVerb sums channels internally).
///
/// Params: predly (0-100ms), size (0-1), dcy (0-1), mix (0-1)
pub struct ReverbCell;

impl ReverbCell {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let predly = param_or(dna, "predly", 0.0);
        let size = param_or(dna, "size", 0.5);
        let dcy = param_or(dna, "dcy", 0.5);
        let mix = param_or(dna, "mix", 0.3);
        let damp = param_or(dna, "damp", 0.5);

        // rlpf controls reverb tank damping: lower = darker.
        // Map our "damp" 0-1 to rlpf frequency: 0 → 20000 (bright), 1 → 400 (dark)
        let rlpf = 20000.0 * (1.0 - damp * 0.98);

        HexoCellBuilder::new("reverb", sr)
            .inputs(1)
            .outputs(1)
            .chain_out("inp", "sig1", &[("vol", 1.0)])
            .chain_io("pverb", "in_l", "sig_l", &[
                ("predly", predly),
                ("size", size),
                ("dcy", dcy),
                ("mix", mix),
                ("rlpf", rlpf),
            ])
            .chain_inp("out", "ch1")
            // Expose runtime-adjustable params
            .shared_param("mix", 1, "mix", mix)
            .shared_param("size", 1, "size", size)
            .shared_param("dcy", 1, "dcy", dcy)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::command::DspCommand;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("size".into(), 0.5);
        params.insert("dcy".into(), 0.5);
        params.insert("mix".into(), 0.3);
        CellDna {
            cell_type: "reverb".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn reverb_cell_builds() {
        let result = ReverbCell::from_dna(&make_dna(), SR);
        assert!(result.is_some(), "ReverbCell should build from DNA");
    }

    #[test]
    fn reverb_cell_passes_audio() {
        let (mut cell, _handles) = ReverbCell::from_dna(&make_dna(), SR).unwrap();

        // Feed a short impulse
        let mut out = [0.0f32];
        cell.tick(&[1.0], &mut out);
        for _ in 0..256 {
            cell.tick(&[0.0], &mut out);
        }

        // Collect output after the impulse — reverb tail should have energy
        let mut buf = Vec::new();
        for _ in 0..4410 {
            cell.tick(&[0.0], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "ReverbCell should produce a reverb tail: rms={rms}"
        );
    }

    #[test]
    fn reverb_cell_is_mono() {
        let (cell, _) = ReverbCell::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
