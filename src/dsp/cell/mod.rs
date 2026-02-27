pub mod drone_bed;

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

        // drone_bed: dual detuned saws → moog filter → LFO
        reg.register("drone_bed", Box::new(|dna: &CellDna, sr: f32| {
            drone_bed::DroneBed::new(dna, sr)
        }));
        reg.register_ranges("drone_bed", &[
            ("root_hz", 20.0, 2000.0),
            ("det", 0.0, 50.0),
            ("cutoff", 20.0, 20000.0),
            ("res", 0.0, 1.0),
            ("lfo_rate", 0.01, 10.0),
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
