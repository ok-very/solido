pub mod arpeggiator;
pub mod chorus_cell;
pub mod delay_cell;
pub mod graph_cell;
pub mod hexo_cell;
pub mod pattern_gen;
pub mod reverb_cell;

use std::collections::HashMap;

use crate::dsp::shared::Shared;

use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// A functional DSP unit with parameter interface. Runs on audio thread.
/// Owns molecules, exposes musical parameters via Shared handles.
pub trait DspCell: Send {
    /// Process one sample. Cells call their molecules' tick() internally.
    /// `input` carries audio from upstream cells via audio wires (empty if none).
    /// Output is interleaved stereo (2 channels) or mono (1 channel).
    fn tick(&mut self, input: &[f32], output: &mut [f32]);

    /// Handle a discrete command from the control thread.
    fn handle_command(&mut self, cmd: &DspCommand);

    /// Return current analysis (RMS, peak) for the control thread.
    fn analysis(&self) -> DspAnalysis;

    /// Number of output channels (1 for mono cells, 2 for stereo).
    fn output_channels(&self) -> usize;

    /// Reset all internal state (molecules, envelopes, accumulators).
    fn reset(&mut self);

    /// Cell type name (matches CellDna.cell_type).
    fn name(&self) -> &str;
}

/// Factory function type: takes cell DNA + sample rate, returns cell + shared handles.
type CellFactory = Box<dyn Fn(&CellDna, f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)>>;

/// Registry that maps cell type strings to factory functions and param ranges.
pub struct CellRegistry {
    factories: HashMap<String, CellFactory>,
    /// Param ranges per cell type: cell_type → { param_name → (min, max) }.
    param_ranges: HashMap<String, HashMap<String, (f32, f32)>>,
}

impl CellRegistry {
    /// Create a new registry with all known cell types registered.
    pub fn new() -> Self {
        let mut reg = Self {
            factories: HashMap::new(),
            param_ranges: HashMap::new(),
        };
        reg.register(
            "pattern_gen",
            Box::new(|dna, sr| pattern_gen::PatternGen::from_dna(dna, sr)),
        );
        reg.register(
            "arpeggiator",
            Box::new(|dna, sr| arpeggiator::Arpeggiator::from_dna(dna, sr)),
        );
        reg.register(
            "reverb",
            Box::new(|dna, sr| reverb_cell::ReverbCell::from_dna(dna, sr)),
        );
        reg.register(
            "delay",
            Box::new(|dna, sr| delay_cell::DelayCell::from_dna(dna, sr)),
        );
        reg.register(
            "chorus",
            Box::new(|dna, sr| chorus_cell::ChorusCell::from_dna(dna, sr)),
        );
        reg.register(
            "hexo_timbre",
            Box::new(|dna, sr| graph_cell::GraphCell::from_dna(dna, sr)),
        );
        reg.register(
            "hexo_strike",
            Box::new(|dna, sr| graph_cell::GraphCell::from_dna(dna, sr)),
        );
        reg.register(
            "hexo_bed",
            Box::new(|dna, sr| graph_cell::GraphCell::from_dna(dna, sr)),
        );
        reg.register(
            "graph",
            Box::new(|dna, sr| graph_cell::GraphCell::from_dna(dna, sr)),
        );

        // Register param ranges for bounded mutation
        reg.register_ranges("pattern_gen", &[
            ("bpm", 30.0, 300.0),
            ("steps", 1.0, 16.0),
            ("hits", 1.0, 16.0),
            ("accent_depth", 0.0, 1.0),
            ("swing", 0.0, 0.5),
        ]);
        reg.register_ranges("arpeggiator", &[
            ("rate_hz", 0.5, 20.0),
            ("pattern", 0.0, 4.0),
            ("octaves", 1.0, 3.0),
            ("gate_length", 0.05, 1.0),
            ("swing", 0.0, 0.5),
        ]);
        reg.register_ranges("reverb", &[
            ("predly", 0.0, 100.0),
            ("size", 0.0, 1.0),
            ("dcy", 0.0, 1.0),
            ("damp", 0.0, 1.0),
            ("mix", 0.0, 1.0),
        ]);
        reg.register_ranges("delay", &[
            ("time", 1.0, 2000.0),
            ("fb", 0.0, 0.9),
            ("mix", 0.0, 1.0),
        ]);
        reg.register_ranges("chorus", &[
            ("time", 2.0, 20.0),
            ("g", 0.0, 0.75),
            ("mix", 0.0, 1.0),
        ]);
        reg.register_ranges("hexo_timbre", &[
            ("freq", 20.0, 8000.0),
            ("det", 0.0, 50.0),
            ("cutoff", 20.0, 16000.0),
            ("res", 0.0, 1.0),
            ("atk", 0.5, 1000.0),
            ("dcy", 5.0, 1000.0),
            ("sus", 0.0, 1.0),
            ("rel", 5.0, 1000.0),
            ("env_depth", 0.0, 12000.0),
            ("env_decay", 10.0, 2000.0),
            ("osc_mix", 0.0, 1.0),
        ]);
        reg.register_ranges("hexo_strike", &[
            ("membrane_freq", 40.0, 800.0),
            ("bandwidth", 0.0, 1.0),
            ("atk", 0.1, 50.0),
            ("dcy", 10.0, 1000.0),
            ("pitch_sweep", 1.0, 10.0),
            ("sweep_decay", 5.0, 500.0),
        ]);
        reg.register_ranges("hexo_bed", &[
            ("root_hz", 20.0, 1000.0),
            ("det", 0.0, 50.0),
            ("cutoff", 20.0, 16000.0),
            ("res", 0.0, 1.0),
            ("lfo_rate", 0.01, 2.0),
            ("lfo_depth", 0.0, 1.0),
            ("osc_mix", 0.0, 1.0),
        ]);
        reg
    }

    fn register(&mut self, name: &str, factory: CellFactory) {
        self.factories.insert(name.into(), factory);
    }

    fn register_ranges(&mut self, cell_type: &str, ranges: &[(&str, f32, f32)]) {
        let map: HashMap<String, (f32, f32)> = ranges
            .iter()
            .map(|(name, min, max)| (name.to_string(), (*min, *max)))
            .collect();
        self.param_ranges.insert(cell_type.into(), map);
    }

    /// Get the valid range for a cell param. Returns None if unknown.
    pub fn param_range(&self, cell_type: &str, param_name: &str) -> Option<(f32, f32)> {
        self.param_ranges
            .get(cell_type)
            .and_then(|m| m.get(param_name))
            .copied()
    }

    /// Build a cell from DNA. Returns cell + shared handles, or None if type is unknown.
    pub fn build(
        &self,
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let factory = self.factories.get(&dna.cell_type)?;
        factory(dna, sr)
    }
}

/// Helper: read a param from CellDna, returning default if missing.
pub(crate) fn param_or(dna: &CellDna, name: &str, default: f32) -> f32 {
    dna.params.get(name).copied().unwrap_or(default)
}

/// Helper: read a string param from CellDna, returning default if missing.
pub(crate) fn string_param_or<'a>(dna: &'a CellDna, name: &str, default: &'a str) -> &'a str {
    dna.string_params.get(name).map(|s| s.as_str()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn registry_builds_all_cell_types() {
        let reg = CellRegistry::new();
        let types = [
            "pattern_gen",
            "arpeggiator",
            "reverb",
            "delay",
            "chorus",
            "hexo_timbre",
            "hexo_strike",
            "hexo_bed",
            // "graph" requires inline graph DNA, tested separately
        ];
        for ty in &types {
            let dna = CellDna {
                cell_type: ty.to_string(),
                params: BTreeMap::new(),
                string_params: BTreeMap::new(),
                graph: None,
            };
            let result = reg.build(&dna, 44100.0);
            assert!(
                result.is_some(),
                "CellRegistry should build cell type '{}'",
                ty
            );
        }
    }

    #[test]
    fn registry_has_param_ranges() {
        let reg = CellRegistry::new();
        // Known range
        let (min, max) = reg.param_range("hexo_strike", "membrane_freq").unwrap();
        assert!(min < max, "Range should be valid: [{min}, {max}]");
        assert!((min - 40.0).abs() < 0.01);
        assert!((max - 800.0).abs() < 0.01);

        // Unknown param returns None
        assert!(reg.param_range("hexo_strike", "nonexistent").is_none());
        // Unknown cell type returns None
        assert!(reg.param_range("unknown_cell", "freq").is_none());
    }

    #[test]
    fn registry_unknown_type_returns_none() {
        let reg = CellRegistry::new();
        let dna = CellDna {
            cell_type: "nonexistent_cell".into(),
            params: BTreeMap::new(),
            string_params: BTreeMap::new(),
            graph: None,
        };
        assert!(reg.build(&dna, 44100.0).is_none());
    }
}
