//! DNA-defined CV patch points — parameter exports (CV output jacks) and
//! imports (CV input jacks) that let organisms modulate each other through
//! the affinity graph.

use crate::dsp::organism_dsp::SharedHandles;
use crate::dsp::shared::Shared;
use crate::module::port::{Port, PortRate};
use crate::module::schema::ModuleSchema;
use crate::module::signal::{Signal, SignalType};
use crate::module::{PortId, SignalError};

use super::super::dna::OrganismDna;
use super::OrganismModule;

/// How an import modulation combines with the base value.
#[derive(Debug, Clone, Copy)]
pub(super) enum ModulationMode {
    Add,
    Multiply,
}

/// Runtime state for a DNA-defined parameter export (CV output jack).
pub(super) struct ParamExportState {
    pub port_id: PortId,
    /// What to read: "rms", "rhythm_density", or "cell{N}.{param}"
    pub source: String,
    /// If source is a cell param, the cached Shared handle
    pub source_handle: Option<Shared>,
}

/// Runtime state for a DNA-defined parameter import (CV input jack).
pub(super) struct ParamImportState {
    pub port_id: PortId,
    pub target_handle: Shared,
    pub base_value: f32,
    pub gain: f32,
    pub mode: ModulationMode,
    pub range: (f32, f32),
    /// True if this import targets `cell0.chaos` — signals the unified chaos
    /// pipeline to read `chaos_interaction_pressure` instead of the Shared handle.
    pub is_chaos_target: bool,
}

/// Build interaction param ports from DNA, returning the export/import state
/// and the updated schema with the new ports added.
pub(super) fn build_interaction_ports(
    dna: &OrganismDna,
    shared_handles: &SharedHandles,
    mut schema: ModuleSchema,
) -> (Vec<ParamExportState>, Vec<ParamImportState>, ModuleSchema) {
    let mut param_exports = Vec::new();
    let mut param_imports = Vec::new();

    if let Some(ref ip) = dna.interaction_params {
        for export in &ip.exports {
            let port_name = format!("x:{}", export.name);
            let port = Port::output(&port_name, SignalType::Float, PortRate::Block)
                .with_range(0.0, 1.0)
                .with_description(&format!(
                    "Param export: {} (src: {})",
                    export.name, export.source
                ));
            let port_id = port.id;
            schema = schema.with_output(port);

            let source_handle = shared_handles.get(&export.source).cloned();
            param_exports.push(ParamExportState {
                port_id,
                source: export.source.clone(),
                source_handle,
            });
        }

        for import in &ip.imports {
            let port_name = format!("x:{}", import.name);
            let port = Port::input(&port_name, SignalType::Float, PortRate::Block)
                .with_range(0.0, 1.0)
                .with_description(&format!(
                    "Param import: {} (tgt: {})",
                    import.name, import.target
                ));
            let port_id = port.id;
            schema = schema.with_input(port);

            let mode = if import.mode == "Multiply" {
                ModulationMode::Multiply
            } else {
                ModulationMode::Add
            };

            if let Some(handle) = shared_handles.get(&import.target) {
                let base_value = handle.value();
                let is_chaos_target = import.target.ends_with(".chaos");
                param_imports.push(ParamImportState {
                    port_id,
                    target_handle: handle.clone(),
                    base_value,
                    gain: import.gain,
                    mode,
                    range: import.range,
                    is_chaos_target,
                });
            } else {
                log::warn!(
                    "[organism:{}] interaction import '{}' target '{}' not found in shared handles",
                    dna.name,
                    import.name,
                    import.target
                );
            }
        }
    }

    (param_exports, param_imports, schema)
}

impl OrganismModule {
    /// Emit interaction param exports into the signal buffer.
    pub(super) fn emit_param_exports(
        &self,
        buffer: &mut Vec<(PortId, Signal)>,
        rms: f32,
        rhythm_density: f32,
    ) {
        for export in &self.param_exports {
            let value = match export.source.as_str() {
                "rms" => rms,
                "rhythm_density" => rhythm_density,
                "seq_chaos" => self.current_seq_chaos,
                "logic_density" => self.current_logic_density,
                "melodic_contour" => self.current_melodic_contour,
                _ => {
                    // cell{N}.{param} — read from cached Shared handle
                    export.source_handle.as_ref().map_or(0.0, |h| h.value())
                }
            };
            buffer.push((export.port_id, Signal::Float(value)));
        }
    }

    /// Try to handle a signal on one of the CV import ports.
    /// Returns `Some(result)` if the port matched, `None` to fall through.
    pub(super) fn receive_import_signal(
        &mut self,
        port: PortId,
        signal: Signal,
    ) -> Option<Result<(), SignalError>> {
        for imp in &self.param_imports {
            if port == imp.port_id {
                if let Signal::Float(v) = signal {
                    let modulated = match imp.mode {
                        ModulationMode::Add => imp.base_value + v * imp.gain,
                        ModulationMode::Multiply => imp.base_value * (1.0 + v * imp.gain),
                    };
                    let clamped = modulated.clamp(imp.range.0, imp.range.1);
                    imp.target_handle.set(clamped);
                    // Record chaos interaction pressure for unified pipeline
                    if imp.is_chaos_target {
                        self.chaos_interaction_pressure = (v * imp.gain).max(0.0);
                    }
                    return Some(Ok(()));
                }
                return Some(Err(SignalError::WrongType {
                    expected: SignalType::Float,
                    got: signal.signal_type(),
                }));
            }
        }
        None
    }
}
