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

        // Register param ranges for bounded mutation
        reg.register_ranges("strike_voice", &[
            ("membrane_freq", 40.0, 800.0),
            ("bandwidth", 10.0, 200.0),
            ("click_mix", 0.0, 1.0),
            ("body_feedback", 0.0, 0.95),
        ]);
        reg.register_ranges("pattern_gen", &[
            ("bpm", 30.0, 300.0),
            ("steps", 1.0, 16.0),
            ("hits", 1.0, 16.0),
            ("accent_depth", 0.0, 1.0),
            ("swing", 0.0, 0.5),
        ]);
        reg.register_ranges("harmonic_bed", &[
            ("root_hz", 20.0, 1000.0),
            ("detune_cents", 0.0, 50.0),
            ("cutoff", 50.0, 16000.0),
            ("resonance", 0.1, 4.0),
            ("pan_rate", 0.01, 2.0),
        ]);
        reg.register_ranges("shimmer_layer", &[
            ("shimmer_amount", 0.0, 1.0),
            ("diffusion", 0.0, 1.0),
            ("feedback", 0.0, 0.9),
        ]);
        reg.register_ranges("arpeggiator", &[
            ("rate_hz", 0.5, 20.0),
            ("pattern", 0.0, 4.0),
            ("octaves", 1.0, 3.0),
            ("gate_length", 0.05, 1.0),
            ("swing", 0.0, 0.5),
        ]);
        reg.register_ranges("timbre_voice", &[
            ("freq", 20.0, 8000.0),
            ("pulse_width", 0.05, 0.95),
            ("filter_base", 20.0, 16000.0),
            ("filter_depth", 0.0, 16000.0),
            ("filter_q", 0.1, 4.0),
            ("attack_ms", 0.5, 500.0),
            ("decay_ms", 5.0, 2000.0),
            ("sustain", 0.0, 1.0),
            ("release_ms", 5.0, 5000.0),
        ]);
        reg.register_ranges("mod_matrix", &[
            ("pwm_rate", 0.1, 20.0),
            ("pwm_depth", 0.0, 1.0),
            ("filter_lfo_rate", 0.01, 10.0),
            ("vibrato_rate", 0.5, 15.0),
            ("vibrato_depth", 0.0, 50.0),
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
                string_params: BTreeMap::new(),
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
        let (min, max) = reg.param_range("strike_voice", "membrane_freq").unwrap();
        assert!(min < max, "Range should be valid: [{min}, {max}]");
        assert!((min - 40.0).abs() < 0.01);
        assert!((max - 800.0).abs() < 0.01);

        // Unknown param returns None
        assert!(reg.param_range("strike_voice", "nonexistent").is_none());
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
        };
        assert!(reg.build(&dna, 44100.0).is_none());
    }
}
