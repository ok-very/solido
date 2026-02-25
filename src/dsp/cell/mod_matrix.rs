use fundsp::prelude32::{shared, Shared};

use crate::dsp::atom::effects::EnvFollowAtom;
use crate::dsp::atom::envelopes::LfoAtom;
use crate::dsp::atom::DspAtom;
use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// MELO modulation routing cell.
///
/// Owns an LFO bank (3 LFOs: pwm, filter, vibrato) + EnvFollowAtom.
/// tick() computes modulation values and applies them to target Shared handles.
/// Output: mono (1ch) — envelope follower output for monitoring.
pub struct ModMatrix {
    lfo_pwm: LfoAtom,
    lfo_filter: LfoAtom,
    lfo_vibrato: LfoAtom,
    env_follow: EnvFollowAtom,
    pwm_rate: Shared,
    pwm_depth: Shared,
    filter_lfo_rate: Shared,
    vibrato_rate: Shared,
    vibrato_depth: Shared,
    /// Modulation output values (updated each tick).
    pub mod_pwm: f32,
    pub mod_filter: f32,
    pub mod_vibrato: f32,
    pub mod_env: f32,
    sample_rate: f32,
}

impl ModMatrix {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let pr = param_or(dna, "pwm_rate", 2.0);
        let pd = param_or(dna, "pwm_depth", 0.2);
        let flr = param_or(dna, "filter_lfo_rate", 0.5);
        let vr = param_or(dna, "vibrato_rate", 5.0);
        let vd = param_or(dna, "vibrato_depth", 10.0);

        let pwm_rate = shared(pr);
        let pwm_depth = shared(pd);
        let filter_lfo_rate = shared(flr);
        let vibrato_rate = shared(vr);
        let vibrato_depth = shared(vd);

        let lfo_pwm = LfoAtom::new(pr, pd, sr);
        let lfo_filter = LfoAtom::new(flr, 1.0, sr);
        let lfo_vibrato = LfoAtom::new(vr, vd, sr);
        let env_follow = EnvFollowAtom::new(0.01, sr);

        let handles = vec![
            ("pwm_rate".into(), pwm_rate.clone()),
            ("pwm_depth".into(), pwm_depth.clone()),
            ("filter_lfo_rate".into(), filter_lfo_rate.clone()),
            ("vibrato_rate".into(), vibrato_rate.clone()),
            ("vibrato_depth".into(), vibrato_depth.clone()),
        ];

        let cell = ModMatrix {
            lfo_pwm,
            lfo_filter,
            lfo_vibrato,
            env_follow,
            pwm_rate,
            pwm_depth,
            filter_lfo_rate,
            vibrato_rate,
            vibrato_depth,
            mod_pwm: 0.0,
            mod_filter: 0.0,
            mod_vibrato: 0.0,
            mod_env: 0.0,
            sample_rate: sr,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for ModMatrix {
    fn tick(&mut self, output: &mut [f32]) {
        // Update LFO rates from shared handles
        self.lfo_pwm.set_param("rate", self.pwm_rate.value());
        self.lfo_pwm.set_param("depth", self.pwm_depth.value());
        self.lfo_filter
            .set_param("rate", self.filter_lfo_rate.value());
        self.lfo_vibrato.set_param("rate", self.vibrato_rate.value());
        self.lfo_vibrato
            .set_param("depth", self.vibrato_depth.value());

        // Tick all LFOs
        let mut pwm_out = [0.0f32];
        let mut filter_out = [0.0f32];
        let mut vib_out = [0.0f32];

        self.lfo_pwm.tick(&[], &mut pwm_out);
        self.lfo_filter.tick(&[], &mut filter_out);
        self.lfo_vibrato.tick(&[], &mut vib_out);

        // Store modulation outputs
        self.mod_pwm = pwm_out[0];
        self.mod_filter = filter_out[0];
        self.mod_vibrato = vib_out[0];

        // Envelope follower gets the filter LFO as input (for amplitude tracking)
        let mut env_out = [0.0f32];
        self.env_follow.tick(&[filter_out[0]], &mut env_out);
        self.mod_env = env_out[0];

        // Output envelope follower value for monitoring
        output[0] = env_out[0];
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis {
            rms: self.mod_env,
            peak: self.mod_env,
        }
    }

    fn output_channels(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.lfo_pwm.reset();
        self.lfo_filter.reset();
        self.lfo_vibrato.reset();
        self.env_follow.reset();
        self.mod_pwm = 0.0;
        self.mod_filter = 0.0;
        self.mod_vibrato = 0.0;
        self.mod_env = 0.0;
    }

    fn name(&self) -> &str {
        "mod_matrix"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("pwm_rate".into(), 2.0);
        params.insert("pwm_depth".into(), 0.2);
        params.insert("filter_lfo_rate".into(), 0.5);
        params.insert("vibrato_rate".into(), 5.0);
        params.insert("vibrato_depth".into(), 10.0);
        CellDna {
            cell_type: "mod_matrix".into(),
            params,
        }
    }

    #[test]
    fn mod_matrix_produces_modulation_values() {
        let (mut cell, _) = ModMatrix::from_dna(&make_dna(), SR).unwrap();

        let mut out = [0.0f32];
        let mut has_nonzero = false;
        for _ in 0..44100 {
            cell.tick(&mut out);
            if out[0].abs() > 0.001 {
                has_nonzero = true;
            }
        }

        assert!(
            has_nonzero,
            "ModMatrix should produce nonzero modulation output"
        );
    }

    #[test]
    fn mod_matrix_is_mono() {
        let (cell, _) = ModMatrix::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
