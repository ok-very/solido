use crate::dsp::shared::Shared;

use crate::dsp::cell::hexo_cell::HexoCellBuilder;
use crate::dsp::cell::{param_or, DspCell};
use crate::organism::dna::CellDna;

/// Delay effect cell backed by HexoDSP's Delay node.
///
/// Graph: Inp(sig1) → Delay(inp, sig) → Out(ch1)
///
/// Params: time (ms, 0-2000), fb (feedback, 0-0.9), mix (0-1)
pub struct DelayCell;

impl DelayCell {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let time_ms = param_or(dna, "time", 250.0);
        let fb = param_or(dna, "fb", 0.3);
        let mix = param_or(dna, "mix", 0.3);

        // HexoDSP Delay time is in seconds
        let time_s = time_ms / 1000.0;

        HexoCellBuilder::new("delay", sr)
            .inputs(1)
            .outputs(1)
            .chain_out("inp", "sig1", &[("vol", 1.0)])
            .chain_io("delay", "inp", "sig", &[
                ("time", time_s),
                ("fb", fb.clamp(0.0, 0.9)),
                ("mix", mix),
            ])
            .chain_inp("out", "ch1")
            .shared_param("time", 1, "time", time_s)
            .shared_param("fb", 1, "fb", fb)
            .shared_param("mix", 1, "mix", mix)
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
        params.insert("time".into(), 100.0);
        params.insert("fb".into(), 0.3);
        params.insert("mix".into(), 0.5);
        CellDna {
            cell_type: "delay".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn delay_cell_builds() {
        let result = DelayCell::from_dna(&make_dna(), SR);
        assert!(result.is_some(), "DelayCell should build from DNA");
    }

    #[test]
    fn delay_cell_echoes_input() {
        let (mut cell, _handles) = DelayCell::from_dna(&make_dna(), SR).unwrap();

        // Feed a short impulse
        let mut out = [0.0f32];
        cell.tick(&[1.0], &mut out);
        for _ in 0..128 {
            cell.tick(&[0.0], &mut out);
        }

        // Collect output — delayed echo should appear after ~100ms = 4410 samples
        let mut buf = Vec::new();
        for _ in 0..8820 {
            cell.tick(&[0.0], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "DelayCell should produce echoes: rms={rms}"
        );
    }

    #[test]
    fn delay_cell_is_mono() {
        let (cell, _) = DelayCell::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
