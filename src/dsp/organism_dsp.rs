use std::collections::HashMap;

use crate::dsp::shared::{shared, Shared};

use crate::dsp::cell::{CellRegistry, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::{OrganismDna, WireMode, WireType};

/// Named shared handle map for the control thread.
pub type SharedHandles = HashMap<String, Shared>;

/// Audio-thread container that owns all cells for a single organism.
///
/// Built from OrganismDna via CellRegistry. Processes cells in wiring
/// order and mixes to stereo output.
pub struct OrganismDsp {
    cells: Vec<Box<dyn DspCell>>,
    /// Wiring info: (src_cell_idx, dst_cell_idx, wire_type_tag)
    wiring: Vec<(usize, usize, WireTag)>,
    /// Per-cell scratch output buffer (max 2 channels per cell).
    scratch: Vec<[f32; 2]>,
    /// Per-cell bypass flag (0.0 = active, >0.5 = bypassed). Lock-free via Shared.
    bypassed: Vec<Shared>,
    /// Mixed stereo output from last tick.
    output: [f32; 2],
    /// Topological order for cell ticking (audio wire DAG).
    tick_order: Vec<usize>,
    /// Cells with no outgoing audio wires — these mix to organism output.
    pub(crate) terminal_cells: Vec<usize>,
    /// Shared handles clone for modulation wire writes.
    mod_handles: SharedHandles,
    sample_rate: f32,
}

/// Simplified wire tag for runtime dispatch.
#[derive(Debug, Clone)]
enum WireTag {
    Audio { gain: f32, mode: WireMode },
    Trigger,
    Modulation { target_param: String, gain: f32 },
}

impl OrganismDsp {
    /// Build from DNA blueprint. Returns (OrganismDsp, SharedHandles).
    /// SharedHandles are cloned Shared references for the control thread.
    pub fn from_dna(dna: &OrganismDna, sr: f32) -> Option<(Self, SharedHandles)> {
        let registry = CellRegistry::new();
        let mut cells: Vec<Box<dyn DspCell>> = Vec::new();
        let mut all_handles = SharedHandles::new();

        let mut bypassed = Vec::new();
        for (i, cell_dna) in dna.cells.iter().enumerate() {
            let (cell, handles) = registry.build(cell_dna, sr)?;
            // Prefix handles with cell index for uniqueness
            for (name, handle) in handles {
                let key = format!("cell{}.{}", i, name);
                all_handles.insert(key, handle);
            }
            // Per-cell bypass: 0.0 = active (default), 1.0 = bypassed
            let bp = shared(0.0);
            all_handles.insert(format!("cell{}.bypass", i), bp.clone());
            bypassed.push(bp);
            cells.push(cell);
        }

        let wiring: Vec<(usize, usize, WireTag)> = dna
            .cell_wiring
            .iter()
            .filter(|w| w.src_cell < cells.len() && w.dst_cell < cells.len())
            .map(|w| {
                let tag = match &w.wire_type {
                    WireType::Audio => WireTag::Audio {
                        gain: w.gain,
                        mode: w.mode.clone(),
                    },
                    WireType::Trigger => WireTag::Trigger,
                    WireType::Modulation { target_param } => WireTag::Modulation {
                        target_param: target_param.clone(),
                        gain: w.gain,
                    },
                };
                (w.src_cell, w.dst_cell, tag)
            })
            .collect();

        let scratch = vec![[0.0f32; 2]; cells.len()];

        // Topological sort (Kahn's algorithm) on audio wires only
        let cell_count = cells.len();
        let mut adj: Vec<Vec<usize>> = vec![vec![]; cell_count];
        let mut in_degree = vec![0usize; cell_count];
        let mut has_outgoing_audio = vec![false; cell_count];

        for (src, dst, tag) in &wiring {
            if matches!(tag, WireTag::Audio { .. }) {
                adj[*src].push(*dst);
                in_degree[*dst] += 1;
                has_outgoing_audio[*src] = true;
            }
        }

        let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for i in 0..cell_count {
            if in_degree[i] == 0 {
                queue.push_back(i);
            }
        }

        let mut tick_order = Vec::with_capacity(cell_count);
        while let Some(node) = queue.pop_front() {
            tick_order.push(node);
            for &next in &adj[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        // Cycle handling: append remaining cells (they'll use previous frame's scratch)
        if tick_order.len() < cell_count {
            let in_order: std::collections::HashSet<usize> = tick_order.iter().copied().collect();
            for i in 0..cell_count {
                if !in_order.contains(&i) {
                    tick_order.push(i);
                }
            }
        }

        // Terminal cells: have audio output channels AND no outgoing audio wires
        let mut terminal_cells: Vec<usize> = (0..cell_count)
            .filter(|&i| cells[i].output_channels() > 0 && !has_outgoing_audio[i])
            .collect();

        // Fallback: if all audio cells feed into wires (e.g. cycles), mix all audio cells
        if terminal_cells.is_empty() {
            terminal_cells = (0..cell_count)
                .filter(|&i| cells[i].output_channels() > 0)
                .collect();
        }

        let mod_handles = all_handles.clone();

        Some((
            OrganismDsp {
                cells,
                wiring,
                scratch,
                bypassed,
                output: [0.0; 2],
                tick_order,
                terminal_cells,
                mod_handles,
                sample_rate: sr,
            },
            all_handles,
        ))
    }

    /// Process one sample. Cells tick in topological order, audio/mod wires applied.
    pub fn tick(&mut self, output: &mut [f32]) {
        // Clear scratch
        for s in &mut self.scratch {
            *s = [0.0; 2];
        }

        // Tick cells in topological order with audio wire accumulation
        for &i in &self.tick_order {
            if self.bypassed[i].value() > 0.5 {
                self.scratch[i] = [0.0; 2];
                continue;
            }

            // Accumulate audio inputs from incoming audio wires
            let mut cell_input = [0.0f32; 2];
            for (src, dst, tag) in &self.wiring {
                if *dst != i {
                    continue;
                }
                if self.bypassed[*src].value() > 0.5 {
                    continue;
                }
                if let WireTag::Audio { gain, mode } = tag {
                    match mode {
                        WireMode::Add => {
                            cell_input[0] += self.scratch[*src][0] * gain;
                            cell_input[1] += self.scratch[*src][1] * gain;
                        }
                        WireMode::Multiply => {
                            cell_input[0] *= self.scratch[*src][0] * gain;
                            cell_input[1] *= self.scratch[*src][1] * gain;
                        }
                    }
                }
            }

            let ch = self.cells[i].output_channels();
            let mut cell_out = [0.0f32; 2];
            self.cells[i].tick(&cell_input[..ch.max(1)], &mut cell_out[..ch.max(1)]);
            self.scratch[i] = cell_out;
        }

        // Process trigger wires
        let mut trigger_commands: Vec<(usize, DspCommand)> = Vec::new();
        for (src, dst, tag) in &self.wiring {
            if self.bypassed[*src].value() > 0.5 || self.bypassed[*dst].value() > 0.5 {
                continue;
            }
            if let WireTag::Trigger = tag {
                if self.scratch[*src][0] > 0.5 {
                    let vel = self.scratch[*src][0].clamp(0.0, 1.0);
                    trigger_commands.push((
                        *dst,
                        DspCommand::NoteOn {
                            freq: 0.0,
                            velocity: vel,
                        },
                    ));
                }
            }
        }

        for (dst_idx, cmd) in trigger_commands {
            if dst_idx < self.cells.len() {
                self.cells[dst_idx].handle_command(&cmd);
            }
        }

        // Process modulation wires: write src scratch to target shared handle
        for (src, dst, tag) in &self.wiring {
            if self.bypassed[*src].value() > 0.5 || self.bypassed[*dst].value() > 0.5 {
                continue;
            }
            if let WireTag::Modulation { target_param, gain } = tag {
                let key = format!("cell{}.{}", dst, target_param);
                if let Some(handle) = self.mod_handles.get(&key) {
                    handle.set(self.scratch[*src][0] * gain);
                }
            }
        }

        // Mix only terminal cells to stereo output with equal-power scaling
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        let terminal_count = self.terminal_cells.len().max(1);
        let scale = 1.0 / (terminal_count as f32).sqrt();

        for &i in &self.terminal_cells {
            if self.bypassed[i].value() > 0.5 {
                continue;
            }
            let ch = self.cells[i].output_channels();
            match ch {
                1 => {
                    left += self.scratch[i][0] * scale * 0.707;
                    right += self.scratch[i][0] * scale * 0.707;
                }
                2 => {
                    left += self.scratch[i][0] * scale;
                    right += self.scratch[i][1] * scale;
                }
                _ => {}
            }
        }

        // Soft clip
        self.output = [soft_clip(left), soft_clip(right)];
        output[0] = self.output[0];
        if output.len() > 1 {
            output[1] = self.output[1];
        }
    }

    /// Dispatch a command to all cells.
    pub fn handle_command(&mut self, cmd: DspCommand) {
        for cell in &mut self.cells {
            cell.handle_command(&cmd);
        }
    }

    /// Collect aggregate analysis from all cells.
    pub fn analysis(&self) -> DspAnalysis {
        let mut rms_sum = 0.0f32;
        let mut peak = 0.0f32;
        let mut count = 0;
        for cell in &self.cells {
            let a = cell.analysis();
            rms_sum += a.rms * a.rms;
            peak = peak.max(a.peak);
            count += 1;
        }
        let rms = if count > 0 {
            (rms_sum / count as f32).sqrt()
        } else {
            0.0
        };
        DspAnalysis { rms, peak }
    }

    /// Reset all cells (clears envelopes, oscillators, accumulators).
    pub fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
        for s in &mut self.scratch {
            *s = [0.0; 2];
        }
        self.output = [0.0; 2];
    }

    /// Number of cells in this organism.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

/// Soft-clip using tanh. Smooth saturation, linear below ±0.5, approaches ±1.
fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism::dna::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dron_dna() -> OrganismDna {
        let mut bed_params = BTreeMap::new();
        bed_params.insert("root_hz".into(), 110.0);
        bed_params.insert("det".into(), 7.0);
        bed_params.insert("cutoff".into(), 800.0);
        bed_params.insert("res".into(), 0.3);
        bed_params.insert("lfo_rate".into(), 0.07);
        bed_params.insert("lfo_depth".into(), 0.3);
        bed_params.insert("osc_mix".into(), 0.7);

        OrganismDna {
            name: "dron-test".into(),
            species: "dron".into(),
            seed: 99,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "drone_bed".into(),
                    params: bed_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    #[test]
    fn wiremode_serde_roundtrip() {
        let wire = CellWire {
            src_cell: 0,
            dst_cell: 1,
            wire_type: WireType::Audio,
            gain: 0.7,
            mode: WireMode::Multiply,
        };
        let json = serde_json::to_string(&wire).unwrap();
        let loaded: CellWire = serde_json::from_str(&json).unwrap();
        assert!((loaded.gain - 0.7).abs() < 0.001);
        assert_eq!(loaded.mode, WireMode::Multiply);
    }
}
