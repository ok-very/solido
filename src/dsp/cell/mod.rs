pub mod arpeggiator;
pub mod harmonic_bed;
pub mod mod_matrix;
pub mod pattern_gen;
pub mod shimmer_layer;
pub mod strike_voice;
pub mod timbre_voice;

use std::collections::HashMap;

use fundsp::prelude32::Shared;

use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// A functional DSP unit with parameter interface. Runs on audio thread.
/// Owns molecules, exposes musical parameters via Shared handles.
pub trait DspCell: Send {
    /// Process one sample. Cells call their molecules' tick() internally.
    /// Output is interleaved stereo (2 channels) or mono (1 channel).
    fn tick(&mut self, output: &mut [f32]);

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

/// Registry that maps cell type strings to factory functions.
pub struct CellRegistry {
    factories: HashMap<String, CellFactory>,
}

impl CellRegistry {
    /// Create a new registry with all known cell types registered.
    pub fn new() -> Self {
        let mut reg = Self {
            factories: HashMap::new(),
        };
        reg.register(
            "strike_voice",
            Box::new(|dna, sr| strike_voice::StrikeVoice::from_dna(dna, sr)),
        );
        reg.register(
            "pattern_gen",
            Box::new(|dna, sr| pattern_gen::PatternGen::from_dna(dna, sr)),
        );
        reg.register(
            "harmonic_bed",
            Box::new(|dna, sr| harmonic_bed::HarmonicBed::from_dna(dna, sr)),
        );
        reg.register(
            "shimmer_layer",
            Box::new(|dna, sr| shimmer_layer::ShimmerLayer::from_dna(dna, sr)),
        );
        reg.register(
            "arpeggiator",
            Box::new(|dna, sr| arpeggiator::Arpeggiator::from_dna(dna, sr)),
        );
        reg.register(
            "timbre_voice",
            Box::new(|dna, sr| timbre_voice::TimbreVoice::from_dna(dna, sr)),
        );
        reg.register(
            "mod_matrix",
            Box::new(|dna, sr| mod_matrix::ModMatrix::from_dna(dna, sr)),
        );
        reg
    }

    fn register(&mut self, name: &str, factory: CellFactory) {
        self.factories.insert(name.into(), factory);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn registry_builds_all_cell_types() {
        let reg = CellRegistry::new();
        let types = [
            "strike_voice",
            "pattern_gen",
            "harmonic_bed",
            "shimmer_layer",
            "arpeggiator",
            "timbre_voice",
            "mod_matrix",
        ];
        for ty in &types {
            let dna = CellDna {
                cell_type: ty.to_string(),
                params: BTreeMap::new(),
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
    fn registry_unknown_type_returns_none() {
        let reg = CellRegistry::new();
        let dna = CellDna {
            cell_type: "nonexistent_cell".into(),
            params: BTreeMap::new(),
        };
        assert!(reg.build(&dna, 44100.0).is_none());
    }
}
