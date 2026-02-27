use crate::dsp::shared::Shared;

use crate::dsp::cell::hexo_cell::HexoCellBuilder;
use crate::dsp::cell::{param_or, DspCell};
use crate::organism::dna::CellDna;

/// Chorus effect cell backed by HexoDSP's Comb filter.
///
/// Uses a short comb filter delay (~2-10ms) to create a chorus-like effect.
/// For true chorus with LFO modulation, see Phase 2 upgrades.
///
/// Graph: Inp(sig1) → Comb(inp, sig) → Out(ch1)
///
/// Params: time (ms, 2-20), g (gain/feedback, 0-0.7), mix (0-1)
pub struct ChorusCell;

impl ChorusCell {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let time_ms = param_or(dna, "time", 5.0);
        let g = param_or(dna, "g", 0.5);
        let mix = param_or(dna, "mix", 0.35);

        // HexoDSP Comb time is in seconds
        let time_s = (time_ms / 1000.0).clamp(0.001, 0.05);

        HexoCellBuilder::new("chorus", sr)
            .inputs(1)
            .outputs(1)
            .chain_out("inp", "sig1", &[("vol", 1.0)])
            .chain_io("comb", "inp", "sig", &[
                ("time", time_s),
                ("g", g.clamp(0.0, 0.75)),
            ])
            .chain_inp("out", "ch1")
            .shared_param("time", 1, "time", time_s)
            .shared_param("g", 1, "g", g)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("time".into(), 5.0);
        params.insert("g".into(), 0.5);
        params.insert("mix".into(), 0.35);
        CellDna {
            cell_type: "chorus".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn chorus_cell_builds() {
        let result = ChorusCell::from_dna(&make_dna(), SR);
        assert!(result.is_some(), "ChorusCell should build from DNA");
    }

    #[test]
    fn chorus_cell_processes_audio() {
        let (mut cell, _handles) = ChorusCell::from_dna(&make_dna(), SR).unwrap();

        // Feed a sine-like signal
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for i in 0..4410 {
            let input = (i as f32 * 440.0 * std::f32::consts::TAU / 44100.0).sin();
            cell.tick(&[input], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.001,
            "ChorusCell should process audio: rms={rms}"
        );
    }

    #[test]
    fn chorus_cell_is_mono() {
        let (cell, _) = ChorusCell::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
