//! XY Pad — Kaoscillator-style 2D controller cell.
//!
//! Pure control signal generator with two outputs:
//! - Channel 0: X position [0, 1]
//! - Channel 1: Y position [0, 1]
//!
//! No audio processing — just forwards Shared handle values to output channels.
//! Intended for touch/mouse control routed to other cells via modulation wires
//! with `source_channel` selecting X (ch0) or Y (ch1).
//!
//! # Parameters
//! - `x`: X axis position [0, 1]
//! - `y`: Y axis position [0, 1]
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "xy_pad_cell",
//!   "params": { "x": 0.5, "y": 0.5 },
//!   "string_params": {}
//! }
//! ```

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for xy_pad_cell.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("x", 0.0, 1.0),
    ("y", 0.0, 1.0),
];

/// Kaoscillator-style 2D control surface cell.
///
/// Outputs X on channel 0 and Y on channel 1. Stateless — reads directly
/// from Shared handles each tick.
pub struct XyPadCell {
    x_handle: Shared,
    y_handle: Shared,
    base_values: HashMap<String, f32>,
}

impl XyPadCell {
    pub fn new(dna: &CellDna, _sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let x = param_or(dna, "x", 0.5);
        let y = param_or(dna, "y", 0.5);

        let x_handle = shared::shared(x);
        let y_handle = shared::shared(y);

        let handles = vec![
            ("x".into(), x_handle.clone()),
            ("y".into(), y_handle.clone()),
        ];

        let base_values = [("x", x), ("y", y)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();

        let cell = Self {
            x_handle,
            y_handle,
            base_values,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for XyPadCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        output[0] = self.x_handle.value().clamp(0.0, 1.0);
        if output.len() > 1 {
            output[1] = self.y_handle.value().clamp(0.0, 1.0);
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                // No internal state to reset
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize { 2 }

    fn reset(&mut self) {
        // Stateless — nothing to reset
    }

    fn name(&self) -> &str { "xy_pad_cell" }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_dna(x: f32, y: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("x".into(), x);
        params.insert("y".into(), y);
        CellDna {
            cell_type: "xy_pad_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    #[test]
    fn default_output() {
        let dna = make_dna(0.5, 0.5);
        let (mut cell, _handles) = XyPadCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);
        assert!((output[0] - 0.5).abs() < 0.001);
        assert!((output[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn handle_write_changes_output() {
        let dna = make_dna(0.5, 0.5);
        let (mut cell, handles) = XyPadCell::new(&dna, 44100.0).unwrap();

        // Write new values via handles
        for (name, handle) in &handles {
            match name.as_str() {
                "x" => handle.set(0.2),
                "y" => handle.set(0.8),
                _ => {}
            }
        }

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);
        assert!((output[0] - 0.2).abs() < 0.001);
        assert!((output[1] - 0.8).abs() < 0.001);
    }

    #[test]
    fn output_clamped_to_unit() {
        let dna = make_dna(0.5, 0.5);
        let (mut cell, handles) = XyPadCell::new(&dna, 44100.0).unwrap();

        // Set out-of-range values
        for (name, handle) in &handles {
            match name.as_str() {
                "x" => handle.set(-0.5),
                "y" => handle.set(1.5),
                _ => {}
            }
        }

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);
        assert!((output[0]).abs() < 0.001, "negative should clamp to 0");
        assert!((output[1] - 1.0).abs() < 0.001, "over 1 should clamp to 1");
    }

    #[test]
    fn channel_count() {
        let dna = make_dna(0.5, 0.5);
        let (cell, _) = XyPadCell::new(&dna, 44100.0).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn name_is_correct() {
        let dna = make_dna(0.5, 0.5);
        let (cell, _) = XyPadCell::new(&dna, 44100.0).unwrap();
        assert_eq!(cell.name(), "xy_pad_cell");
    }
}
