use std::collections::HashMap;

use crate::dsp::shared::{shared, HandleId, Shared};

use crate::dsp::cell::{build_cell, find_range, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand, MAX_MICRO_DEGREES};
use crate::organism::dna::{OrganismDna, WireMode, WireType};

/// Merge threshold: micro degrees within this many cents of a 12-TET degree
/// replace the 12-TET position (micro wins position, weight stacks).
const MICRO_MERGE_TOLERANCE: f32 = 20.0;

/// Combined tuning: 12-TET base + up to 12 microtonal overlay degrees merged.
#[derive(Clone, Debug)]
struct CombinedTuning {
    cents: [f32; 24],
    weights: [f32; 24],
    count: u8,
}

/// Named shared handle map for the control thread.
pub type SharedHandles = HashMap<String, Shared>;

/// Audio-thread container that owns all cells for a single organism.
///
/// Built from OrganismDna via build_cell(). Processes cells in wiring
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
    /// Indexed shared handles for the audio thread — direct Vec indexing via HandleId.
    /// Shares the same Arc<AtomicU32> backing as the control-thread SharedHandles.
    handle_vec: Vec<Shared>,
    /// Previous sample values for trigger edge detection (one per cell).
    #[cfg_attr(test, allow(dead_code))]  // Accessed in tests
    pub(crate) trigger_prev: Vec<f32>,
    /// Preallocated trigger command buffer — cleared and reused each tick.
    /// Avoids Vec::new() allocation on the audio thread.
    trigger_commands: Vec<(usize, DspCommand)>,
    /// Precomputed modulation wires: key strings and param ranges resolved at from_dna().
    /// Avoids format!() and HashMap lookups on the audio thread.
    mod_wires: Vec<ModWire>,
    #[allow(dead_code)]
    sample_rate: f32,
    /// Cell type name → first index. Built at from_dna(), O(1) lookup, allocated once.
    #[allow(dead_code)]
    cell_indices: HashMap<String, usize>,
    /// Precomputed cell indices for RT-safe bridge data access (no HashMap lookup on hot path).
    seq_cell_idx: Option<usize>,
    env_cell_idx: Option<usize>,
    logic_seq_cell_idx: Option<usize>,
    /// Precomputed handle IDs for generative param feedback (chaos, density).
    seq_chaos_handle_id: Option<HandleId>,
    logic_density_handle_id: Option<HandleId>,
    /// Precomputed freq handle indices for osc/saw_bank cells — used by spectral centroid.
    /// Built at from_dna() to avoid format!() and HashMap lookup on the audio thread.
    osc_freq_handles: Vec<(usize, HandleId)>, // (cell_index, HandleId for "cell{N}.freq")
    /// Scale gravity weights (12 pitch classes) for audio-thread quantization.
    /// Received via SetScaleWeights DspCommand from OrganismModule.
    scale_weights: [f32; 12],
    /// Blend factor [0,1] for scale quantization (scale_affinity × fidelity).
    scale_blend: f32,
    /// Microtonal overlay cents positions.
    micro_cents: [f32; MAX_MICRO_DEGREES],
    /// Microtonal overlay per-degree weights.
    micro_weights: [f32; MAX_MICRO_DEGREES],
    /// Active micro degree count.
    micro_count: u8,
    /// Micro tuning blend [0,1].
    micro_blend: f32,
    /// Combined tuning (rebuilt when either layer changes).
    combined: CombinedTuning,
    /// True when combined needs rebuild (set on SetScaleWeights or SetMicroTuning).
    combined_dirty: bool,
}

/// Simplified wire tag for runtime dispatch.
#[derive(Debug, Clone)]
enum WireTag {
    Audio { gain: f32, mode: WireMode },
    Trigger,
    Modulation { target_param: String, gain: f32, mode: WireMode },
}

/// Precomputed modulation wire — all strings and ranges resolved at from_dna() time.
///
/// Eliminates `format!()` and HashMap lookups from the audio thread hot path.
/// Lives on the OrganismDsp struct; built once, read every tick.
#[derive(Debug)]
struct ModWire {
    src: usize,
    dst: usize,
    /// Pre-resolved index into handle_vec — direct array access, no hashing.
    handle_id: HandleId,
    /// Raw param name for `get_param_base()`, e.g. `"cutoff"`
    param_name: String,
    gain: f32,
    mode: WireMode,
    /// Clamping range from cell's PARAM_RANGES, pre-fetched at construction.
    param_range: Option<(f32, f32)>,
}

impl OrganismDsp {
    /// Build from DNA blueprint. Returns (OrganismDsp, SharedHandles).
    /// SharedHandles are cloned Shared references for the control thread.
    pub fn from_dna(dna: &OrganismDna, sr: f32) -> Option<(Self, SharedHandles)> {
        let mut cells: Vec<Box<dyn DspCell>> = Vec::new();
        let mut all_handles = SharedHandles::new();
        let mut handle_vec: Vec<Shared> = Vec::new();
        let mut handle_map: HashMap<String, HandleId> = HashMap::new();

        let mut bypassed = Vec::new();
        for (i, cell_dna) in dna.cells.iter().enumerate() {
            let (cell, handles) = build_cell(cell_dna, sr)?;
            // Prefix handles with cell index for uniqueness
            for (name, handle) in handles {
                let key = format!("cell{}.{}", i, name);
                let id = HandleId(handle_vec.len() as u16);
                handle_vec.push(handle.clone());
                handle_map.insert(key.clone(), id);
                all_handles.insert(key, handle);
            }
            // Per-cell bypass: 0.0 = active (default), 1.0 = bypassed
            let bp = shared(0.0);
            let key = format!("cell{}.bypass", i);
            let id = HandleId(handle_vec.len() as u16);
            handle_vec.push(bp.clone());
            handle_map.insert(key.clone(), id);
            all_handles.insert(key, bp.clone());
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
                        mode: w.mode.clone(),
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

        // Helper: check if cell is control signal (not audio)
        let is_control_signal = |cell_name: &str| {
            matches!(
                cell_name,
                "seq_cell" | "env_cell" | "slew_cell" | "lfo_cell" | "accent_env_cell" | "func_gen_cell" | "logic_seq_cell" | "walk_cell"
            )
        };

        // Terminal cells: produce audio AND have no outgoing audio wires
        // Exclude control signal cells (seq, env, lfo, etc.) from mixing
        let mut terminal_cells: Vec<usize> = (0..cell_count)
            .filter(|&i| {
                cells[i].output_channels() > 0
                && !has_outgoing_audio[i]
                && !is_control_signal(cells[i].name())
            })
            .collect();

        // Fallback: if all audio cells feed into wires (e.g. cycles), mix all audio cells
        // Still exclude control signals
        if terminal_cells.is_empty() {
            terminal_cells = (0..cell_count)
                .filter(|&i| cells[i].output_channels() > 0 && !is_control_signal(cells[i].name()))
                .collect();
        }

        // Build cell type → index map (first occurrence wins)
        let mut cell_indices = HashMap::new();
        for (i, cell) in cells.iter().enumerate() {
            cell_indices.entry(cell.name().to_string()).or_insert(i);
        }

        // Precompute direct cell indices for RT-safe bridge data access
        let seq_cell_idx = cell_indices.get("seq_cell").copied();
        let env_cell_idx = cell_indices.get("env_cell").copied();
        let logic_seq_cell_idx = cell_indices.get("logic_seq_cell").copied();

        // Precompute handle IDs for generative param feedback
        let seq_chaos_handle_id = seq_cell_idx
            .and_then(|idx| handle_map.get(&format!("cell{}.chaos", idx)).copied());
        let logic_density_handle_id = logic_seq_cell_idx
            .and_then(|idx| handle_map.get(&format!("cell{}.density", idx)).copied());

        // Precompute osc freq handle indices for spectral centroid
        let mut osc_freq_handles = Vec::new();
        for (i, cell) in cells.iter().enumerate() {
            let name = cell.name();
            if name == "osc_cell" || name == "saw_bank_cell" {
                let key = format!("cell{}.freq", i);
                if let Some(&id) = handle_map.get(&key) {
                    osc_freq_handles.push((i, id));
                }
            }
        }

        let trigger_prev = vec![0.0; cell_count];

        // Preallocate trigger_commands buffer — capacity covers all trigger wires
        // so push() never reallocates on the audio thread.
        let trigger_wire_count = wiring.iter()
            .filter(|(_, _, tag)| matches!(tag, WireTag::Trigger))
            .count();
        let trigger_commands = Vec::with_capacity(trigger_wire_count.max(cell_count));

        // Build ModWires: resolve handle indices and param ranges NOW, at construction time.
        // This keeps apply_modulation() allocation-free on the audio thread.
        let mut mod_wires: Vec<ModWire> = Vec::new();
        for (src, dst, tag) in &wiring {
            if let WireTag::Modulation { target_param, gain, mode } = tag {
                let param_key = format!("cell{}.{}", dst, target_param);
                let handle_id = match handle_map.get(&param_key) {
                    Some(&id) => id,
                    None => continue, // skip wires referencing non-existent handles
                };
                let param_range = find_range(cells[*dst].param_ranges(), target_param);
                mod_wires.push(ModWire {
                    src: *src,
                    dst: *dst,
                    handle_id,
                    param_name: target_param.clone(),
                    gain: *gain,
                    mode: mode.clone(),
                    param_range,
                });
            }
        }

        Some((
            OrganismDsp {
                cells,
                wiring,
                scratch,
                bypassed,
                output: [0.0; 2],
                tick_order,
                terminal_cells,
                handle_vec,
                trigger_prev,
                trigger_commands,
                mod_wires,
                sample_rate: sr,
                cell_indices,
                seq_cell_idx,
                env_cell_idx,
                logic_seq_cell_idx,
                seq_chaos_handle_id,
                logic_density_handle_id,
                osc_freq_handles,
                scale_weights: [0.0; 12],
                scale_blend: 0.0,
                micro_cents: [0.0; MAX_MICRO_DEGREES],
                micro_weights: [0.0; MAX_MICRO_DEGREES],
                micro_count: 0,
                micro_blend: 0.0,
                combined: CombinedTuning {
                    cents: [0.0; 24],
                    weights: [0.0; 24],
                    count: 0,
                },
                combined_dirty: false,
            },
            all_handles,
        ))
    }

    /// Process one sample. Cells tick in topological order, audio/mod wires applied.
    pub fn tick(&mut self, output: &mut [f32]) {
        // FIRST: Apply modulation from previous tick's cell outputs (still in scratch)
        // This must happen BEFORE clearing scratch and BEFORE cells tick
        self.apply_modulation();

        // Clear scratch for current tick's outputs
        for s in &mut self.scratch {
            *s = [0.0; 2];
        }

        // Tick cells in topological order with audio wire accumulation
        for &i in &self.tick_order {
            // Accumulate audio inputs from incoming audio wires
            let mut cell_input = [0.0f32; 2];
            for (src, dst, tag) in &self.wiring {
                if *dst != i {
                    continue;
                }
                // Note: do NOT skip bypassed sources here. A bypassed cell stores its
                // pass-through audio in scratch[src], and downstream cells should still
                // receive it. Bypass means "transparent", not "silent".
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
                        WireMode::Replace => {
                            cell_input[0] = self.scratch[*src][0] * gain;
                            cell_input[1] = self.scratch[*src][1] * gain;
                        }
                    }
                }
            }

            // Bypass: pass through input unchanged, don't process
            if self.bypassed[i].value() > 0.5 {
                self.scratch[i] = cell_input;
                continue;
            }

            let ch = self.cells[i].output_channels();
            let mut cell_out = [0.0f32; 2];
            self.cells[i].tick(&cell_input, &mut cell_out[..ch.max(1)]);
            self.scratch[i] = cell_out;
        }

        // Process trigger wires with RISING EDGE detection.
        // trigger_commands is preallocated on the struct — no Vec::new() on audio thread.
        self.trigger_commands.clear();
        for (src, dst, tag) in &self.wiring {
            if self.bypassed[*src].value() > 0.5 || self.bypassed[*dst].value() > 0.5 {
                continue;
            }
            if let WireTag::Trigger = tag {
                let prev = self.trigger_prev[*src];
                let curr = self.scratch[*src][0];

                // Rising edge: prev was low, curr is high
                if curr > 0.5 && prev <= 0.5 {
                    let vel = curr.clamp(0.0, 1.0);
                    self.trigger_commands.push((
                        *dst,
                        DspCommand::NoteOn {
                            freq: 0.0,
                            velocity: vel,
                        },
                    ));
                }

                self.trigger_prev[*src] = curr;
            }
        }

        for (dst_idx, cmd) in self.trigger_commands.drain(..) {
            if dst_idx < self.cells.len() {
                self.cells[dst_idx].handle_command(&cmd);
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
                    // Mono signal goes equally to both channels — VoiceBus
                    // applies constant-power panning downstream.
                    left += self.scratch[i][0] * scale;
                    right += self.scratch[i][0] * scale;
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

    /// Apply modulation from previous tick's outputs to parameters.
    /// Must be called BEFORE cells tick so they read current modulated values.
    ///
    /// Uses precomputed `mod_wires` — no `format!()`, no lookups,
    /// no heap allocation. RT-safe.
    fn apply_modulation(&mut self) {
        // Pass 1: Reset all modulated params to base values.
        // Skip only when DESTINATION is bypassed (don't mess with bypassed cell params).
        // When SOURCE is bypassed, still reset — so param reverts to base (Replace fallback).
        for wire in &self.mod_wires {
            if self.bypassed[wire.dst].value() > 0.5 {
                continue;
            }
            let handle = &self.handle_vec[wire.handle_id.0 as usize];
            if let Some(base) = self.cells[wire.dst].get_param_base(&wire.param_name) {
                handle.set(base);
            }
        }

        // Rebuild combined tuning before modulation pass (lazy, only when dirty)
        if self.combined_dirty {
            self.rebuild_combined();
        }

        // Pass 2: Apply modulation (Add or Multiply mode) with clamping.
        // param_range pre-fetched from cell PARAM_RANGES at construction — no lookup needed.
        for wire in &self.mod_wires {
            if self.bypassed[wire.src].value() > 0.5 || self.bypassed[wire.dst].value() > 0.5 {
                continue;
            }
            let handle = &self.handle_vec[wire.handle_id.0 as usize];
            let base = handle.value();
            let mod_signal = self.scratch[wire.src][0];

            let modulated = match wire.mode {
                WireMode::Add => base + (mod_signal * wire.gain),
                WireMode::Multiply => base * (mod_signal * wire.gain),
                WireMode::Replace => mod_signal * wire.gain,
            };

            let clamped = if let Some((min, max)) = wire.param_range {
                modulated.clamp(min, max)
            } else {
                modulated
            };

            // Quantize freq targets to scale/micro tuning when active
            let final_val = if wire.mode == WireMode::Replace
                && wire.param_name == "freq"
                && (self.scale_blend > 0.01 || self.micro_blend > 0.01)
                && self.combined.count > 0
            {
                let blend = self.scale_blend.max(self.micro_blend);
                quantize_to_tuning(clamped, &self.combined, blend)
            } else {
                clamped
            };

            handle.set(final_val);
        }
    }

    /// Dispatch a command to all cells (or handle organism-level commands).
    pub fn handle_command(&mut self, cmd: DspCommand) {
        match cmd {
            DspCommand::SetScaleWeights(weights, blend) => {
                self.scale_weights = weights;
                self.scale_blend = blend;
                self.combined_dirty = true;
            }
            DspCommand::SetMicroTuning { cents, weights, count, blend } => {
                self.micro_cents = cents;
                self.micro_weights = weights;
                self.micro_count = count;
                self.micro_blend = blend;
                self.combined_dirty = true;
            }
            _ => {
                for cell in &mut self.cells {
                    cell.handle_command(&cmd);
                }
            }
        }
    }

    /// Rebuild the combined tuning from 12-TET base + micro overlay.
    /// Called lazily when combined_dirty is set, before quantization.
    fn rebuild_combined(&mut self) {
        self.combined_dirty = false;
        let mut count: u8 = 0;

        // 1. Add active 12-TET degrees (weight >= 0.1) at 0, 100, 200...1100 cents
        for i in 0..12 {
            if self.scale_weights[i] < 0.1 {
                continue;
            }
            if count >= 24 { break; }
            self.combined.cents[count as usize] = i as f32 * 100.0;
            self.combined.weights[count as usize] = self.scale_weights[i];
            count += 1;
        }

        // 2. For each micro degree: merge or add
        for m in 0..self.micro_count as usize {
            if m >= MAX_MICRO_DEGREES { break; }
            let mc = self.micro_cents[m];
            let mw = self.micro_weights[m];
            if mw < 0.01 { continue; }

            // Check if within MICRO_MERGE_TOLERANCE of an existing degree
            let mut merged = false;
            for j in 0..count as usize {
                let dist = cents_distance(self.combined.cents[j], mc);
                if dist < MICRO_MERGE_TOLERANCE {
                    // Micro wins position, take max weight
                    self.combined.cents[j] = mc;
                    self.combined.weights[j] = self.combined.weights[j].max(mw);
                    merged = true;
                    break;
                }
            }
            if !merged && count < 24 {
                self.combined.cents[count as usize] = mc;
                self.combined.weights[count as usize] = mw;
                count += 1;
            }
        }

        self.combined.count = count;
    }

    /// Collect aggregate analysis from all cells, including cell-level bridge data.
    /// RT-safe: uses precomputed indices via bridge_data(), no format!() or allocation.
    #[allow(dead_code)]
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

        let bridge = self.bridge_data();

        DspAnalysis {
            rms,
            peak,
            seq_pitch_hz: bridge.seq_pitch_hz,
            seq_gate: bridge.seq_gate,
            env_level: bridge.env_level,
            spectral_centroid: bridge.spectral_centroid,
            seq_chaos: bridge.seq_chaos,
            logic_density: bridge.logic_density,
        }
    }

    /// Reset all cells (clears envelopes, oscillators, accumulators).
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Last stereo output from tick() — for diagnostics.
    #[allow(dead_code)]
    pub fn last_output(&self) -> [f32; 2] {
        self.output
    }


    /// RT-safe bridge data: cell-level signals for the control thread.
    ///
    /// Uses precomputed cell indices — no HashMap lookup, no format!(), no allocation.
    /// Call after tick() to read current cell outputs from scratch.
    pub fn bridge_data(&self) -> BridgeData {
        let seq_pitch_hz = self.seq_cell_idx
            .map(|idx| self.scratch[idx][1]) // ch1 = pitch
            .unwrap_or(0.0);
        let seq_gate = self.seq_cell_idx
            .map(|idx| self.scratch[idx][0] > 0.5)
            .unwrap_or(false);
        let env_level = self.env_cell_idx
            .map(|idx| self.scratch[idx][0])
            .unwrap_or(0.0);

        // Spectral centroid: Σ(freq_i × energy_i) / Σ(energy_i) across osc cells
        let mut freq_energy_sum = 0.0f32;
        let mut energy_sum = 0.0f32;
        for &(cell_idx, handle_id) in &self.osc_freq_handles {
            let energy = self.scratch[cell_idx][0].abs() + self.scratch[cell_idx][1].abs();
            if energy > 0.001 {
                let freq = self.handle_vec[handle_id.0 as usize].value();
                freq_energy_sum += freq * energy;
                energy_sum += energy;
            }
        }
        let spectral_centroid = if energy_sum > 0.001 {
            freq_energy_sum / energy_sum
        } else {
            0.0
        };

        // Generative param feedback — read from precomputed handle IDs
        let seq_chaos = self.seq_chaos_handle_id
            .map(|id| self.handle_vec[id.0 as usize].value())
            .unwrap_or(0.0);
        let logic_density = self.logic_density_handle_id
            .map(|id| self.handle_vec[id.0 as usize].value())
            .unwrap_or(0.0);

        BridgeData {
            seq_pitch_hz,
            seq_gate,
            env_level,
            spectral_centroid,
            seq_chaos,
            logic_density,
        }
    }
}

/// Cell-level bridge data returned by `OrganismDsp::bridge_data()`.
/// Stack-allocated, RT-safe.
#[derive(Clone, Copy, Debug)]
pub struct BridgeData {
    pub seq_pitch_hz: f32,
    pub seq_gate: bool,
    pub env_level: f32,
    pub spectral_centroid: f32,
    pub seq_chaos: f32,
    pub logic_density: f32,
}

/// Soft-clip using tanh. Smooth saturation, linear below ±0.5, approaches ±1.
fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

/// Circular distance in cents within one octave (1200 cents period).
#[inline]
fn cents_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 1200.0;
    d.min(1200.0 - d)
}

/// RT-safe cents-based quantization: snap Hz to nearest degree in CombinedTuning.
/// Works in cents space for microtonal accuracy. Pure f32 math — no alloc, no locks.
#[inline]
fn quantize_to_tuning(raw_hz: f32, tuning: &CombinedTuning, blend: f32) -> f32 {
    if blend < 0.01 || raw_hz < 20.0 || tuning.count == 0 {
        return raw_hz;
    }
    // Convert Hz → cents from C4 (261.63 Hz)
    let raw_cents = 1200.0 * (raw_hz / 261.63).log2();
    // Reduce to [0, 1200) within octave
    let octave_cents = ((raw_cents % 1200.0) + 1200.0) % 1200.0;
    let octave_base = raw_cents - octave_cents;

    let mut best_cents = octave_cents;
    let mut best_dist = f32::MAX;
    for i in 0..tuning.count as usize {
        let w = tuning.weights[i];
        if w < 0.01 { continue; }
        let dist = cents_distance(octave_cents, tuning.cents[i]);
        let weighted_dist = dist / w;
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_cents = tuning.cents[i];
        }
    }

    let quantized_cents = octave_base + best_cents;
    let quantized_hz = 261.63 * 2.0f32.powf(quantized_cents / 1200.0);
    raw_hz * (1.0 - blend) + quantized_hz * blend
}

/// RT-safe scale quantization: snap Hz to nearest active scale degree, blended.
/// Pure f32 math — no alloc, no locks. 12-iteration loop is O(1).
/// Kept for backward compatibility in non-DSP contexts (e.g. pitch module).
#[inline]
#[allow(dead_code)]
fn quantize_to_scale_fast(raw_hz: f32, gravity: &[f32; 12], blend: f32) -> f32 {
    if blend < 0.01 || raw_hz < 20.0 {
        return raw_hz;
    }
    let midi = 12.0 * (raw_hz / 440.0).log2() + 69.0;
    let octave = (midi / 12.0).floor();
    let degree = midi - octave * 12.0;
    let mut best_degree = degree;
    let mut best_dist = f32::MAX;
    for i in 0..12 {
        if gravity[i] < 0.1 {
            continue;
        }
        let d = i as f32;
        let dist = (degree - d).abs().min(12.0 - (degree - d).abs());
        let weighted_dist = dist / gravity[i];
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_degree = d;
        }
    }
    let quantized_midi = octave * 12.0 + best_degree;
    let quantized_hz = 440.0 * 2.0f32.powf((quantized_midi - 69.0) / 12.0);
    raw_hz * (1.0 - blend) + quantized_hz * blend
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism::dna::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

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

    // Modulation wire tests

    /// Helper: Create simple DNA with LFO→filter modulation wire
    fn make_modulation_test_dna(lfo_depth: f32, mod_gain: f32, base_cutoff: f32) -> OrganismDna {
        let mut lfo_params = BTreeMap::new();
        lfo_params.insert("rate".into(), 1.0); // 1 Hz for easy testing
        lfo_params.insert("depth".into(), lfo_depth);
        let mut lfo_string_params = BTreeMap::new();
        lfo_string_params.insert("shape".into(), "sine".into());

        let mut filter_params = BTreeMap::new();
        filter_params.insert("cutoff".into(), base_cutoff);
        filter_params.insert("res".into(), 0.1);
        let mut filter_string_params = BTreeMap::new();
        filter_string_params.insert("ftype".into(), "lowpass".into());

        let mut mixer_params = BTreeMap::new();
        mixer_params.insert("gain".into(), 1.0);
        mixer_params.insert("pan".into(), 0.0);

        OrganismDna {
            name: "mod-test".into(),
            species: "test".into(),
            active: true,
            seed: 123,
            version: 4,
            cells: vec![
                CellDna { cell_type: "lfo_cell".into(), params: lfo_params, string_params: lfo_string_params },
                CellDna { cell_type: "filter_cell".into(), params: filter_params, string_params: filter_string_params },
                CellDna { cell_type: "mixer_cell".into(), params: mixer_params, string_params: BTreeMap::new() },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Modulation { target_param: "cutoff".into() },
                    gain: mod_gain,
                    mode: WireMode::Add,
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.5,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        }
    }

    #[test]
    fn mod_adds_to_base() {
        // Test that modulation adds to base value instead of replacing it
        let base_cutoff = 1000.0;
        let lfo_depth = 1.0; // Full bipolar swing (-1 to +1)
        let mod_gain = 500.0; // Modulation range ±500 Hz
        let dna = make_modulation_test_dna(lfo_depth, mod_gain, base_cutoff);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).expect("build failed");

        let cutoff_handle = handles.get("cell1.cutoff").expect("cutoff handle missing");

        // Process a few samples to let LFO settle
        let mut output = [0.0f32; 2];
        for _ in 0..100 {
            org.tick(&mut output);
        }

        // LFO output at some point will be non-zero
        // Modulated cutoff should be: base + (lfo_output * gain)
        // NOT just: lfo_output * gain
        let modulated_cutoff = cutoff_handle.value();

        // Since LFO swings from -1 to +1 with gain 500, modulated cutoff should be in range:
        // base - mod_gain <= cutoff <= base + mod_gain
        // 1000 - 500 <= cutoff <= 1000 + 500
        assert!(
            modulated_cutoff >= base_cutoff - mod_gain - 1.0
                && modulated_cutoff <= base_cutoff + mod_gain + 1.0,
            "Expected cutoff in range [{}, {}], got {}",
            base_cutoff - mod_gain,
            base_cutoff + mod_gain,
            modulated_cutoff
        );
    }

    #[test]
    fn mod_clamps_to_range() {
        // Test that modulated values are clamped to param range (20-20000 Hz for cutoff)
        let base_cutoff = 100.0;
        let lfo_depth = 1.0;
        let mod_gain = 200.0; // Large gain that would push below 20 Hz
        let dna = make_modulation_test_dna(lfo_depth, mod_gain, base_cutoff);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).expect("build failed");

        let cutoff_handle = handles.get("cell1.cutoff").expect("cutoff handle missing");

        // Process samples
        let mut output = [0.0f32; 2];
        for _ in 0..1000 {
            org.tick(&mut output);
            let cutoff = cutoff_handle.value();
            // Cutoff should always be clamped to valid range [20, 20000]
            assert!(
                cutoff >= 20.0 && cutoff <= 20000.0,
                "Cutoff {} out of range [20, 20000]",
                cutoff
            );
        }
    }

    #[test]
    fn mod_gain_scales_depth() {
        // Test that higher wire gain → larger modulation swing
        let base_cutoff = 1000.0;
        let lfo_depth = 1.0;

        // Test with two different gains
        let small_gain = 100.0;
        let large_gain = 500.0;

        let dna_small = make_modulation_test_dna(lfo_depth, small_gain, base_cutoff);
        let (mut org_small, handles_small) = OrganismDsp::from_dna(&dna_small, SR).unwrap();
        let cutoff_small = handles_small.get("cell1.cutoff").unwrap();

        let dna_large = make_modulation_test_dna(lfo_depth, large_gain, base_cutoff);
        let (mut org_large, handles_large) = OrganismDsp::from_dna(&dna_large, SR).unwrap();
        let cutoff_large = handles_large.get("cell1.cutoff").unwrap();

        let mut output = [0.0f32; 2];
        let mut small_max_dev = 0.0f32;
        let mut large_max_dev = 0.0f32;

        // Collect max deviation from base over 1000 samples
        for _ in 0..1000 {
            org_small.tick(&mut output);
            org_large.tick(&mut output);
            small_max_dev = small_max_dev.max((cutoff_small.value() - base_cutoff).abs());
            large_max_dev = large_max_dev.max((cutoff_large.value() - base_cutoff).abs());
        }

        // Larger gain should produce larger deviation
        assert!(
            large_max_dev > small_max_dev * 2.0,
            "Expected large_gain deviation ({}) > small_gain deviation ({}) * 2",
            large_max_dev,
            small_max_dev
        );
    }

    #[test]
    fn mod_zero_depth_no_change() {
        // Test that LFO with depth=0 doesn't change the target param
        let base_cutoff = 1000.0;
        let lfo_depth = 0.0; // Zero depth
        let mod_gain = 500.0;
        let dna = make_modulation_test_dna(lfo_depth, mod_gain, base_cutoff);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let cutoff_handle = handles.get("cell1.cutoff").unwrap();

        let mut output = [0.0f32; 2];
        for _ in 0..1000 {
            org.tick(&mut output);
            let cutoff = cutoff_handle.value();
            // With zero depth, cutoff should stay at base value
            assert!(
                (cutoff - base_cutoff).abs() < 0.01,
                "Expected cutoff to stay at {}, got {}",
                base_cutoff,
                cutoff
            );
        }
    }

    // Integration tests for 4-cell DRON

    #[test]
    fn dron_alpha_loads() {
        // Test that upgraded dron-alpha.json loads and builds OrganismDsp
        let json = std::fs::read_to_string("assets/dna/dron-alpha.json")
            .expect("Failed to read dron-alpha.json");
        let dna: OrganismDna = serde_json::from_str(&json)
            .expect("Failed to parse dron-alpha.json");

        assert_eq!(dna.version, 4, "Expected version 4");
        assert_eq!(dna.cells.len(), 4, "Expected 4 cells");
        assert_eq!(dna.cell_wiring.len(), 3, "Expected 3 wires");

        let result = OrganismDsp::from_dna(&dna, SR);
        assert!(result.is_some(), "Expected OrganismDsp to build from dron-alpha DNA");
    }

    #[test]
    fn dron_alpha_sounds() {
        // Test that the 4-cell DRON produces non-zero audio
        let json = std::fs::read_to_string("assets/dna/dron-alpha.json")
            .expect("Failed to read dron-alpha.json");
        let dna: OrganismDna = serde_json::from_str(&json)
            .expect("Failed to parse dron-alpha.json");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut rms_acc = 0.0f32;

        // Tick for 1 second (44100 samples)
        for _ in 0..SR as usize {
            org.tick(&mut output);
            rms_acc += (output[0] * output[0] + output[1] * output[1]) * 0.5;
        }

        let rms = (rms_acc / SR).sqrt();
        assert!(
            rms > 0.005,
            "Expected RMS > 0.005 for sounding organism, got {}",
            rms
        );
    }

    #[test]
    fn dron_alpha_lfo_audible() {
        // Test that LFO modulation is audible (RMS variance across time windows)
        let json = std::fs::read_to_string("assets/dna/dron-alpha.json")
            .expect("Failed to read dron-alpha.json");
        let dna: OrganismDna = serde_json::from_str(&json)
            .expect("Failed to parse dron-alpha.json");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let window_size = 4410; // 0.1 second windows
        let window_count = 10;
        let mut window_rms = Vec::new();

        for _ in 0..window_count {
            let mut rms_acc = 0.0f32;
            for _ in 0..window_size {
                org.tick(&mut output);
                rms_acc += (output[0] * output[0] + output[1] * output[1]) * 0.5;
            }
            window_rms.push((rms_acc / window_size as f32).sqrt());
        }

        // Calculate variance of RMS across windows
        let mean_rms: f32 = window_rms.iter().sum::<f32>() / window_rms.len() as f32;
        let variance: f32 = window_rms
            .iter()
            .map(|rms| (rms - mean_rms).powi(2))
            .sum::<f32>()
            / window_rms.len() as f32;

        // LFO modulation should create variance in RMS across windows
        assert!(
            variance > 0.000004,
            "Expected RMS variance > 0.000004 indicating LFO modulation, got {}",
            variance
        );
    }

    #[test]
    fn four_cell_chain_works() {
        // Test minimal 4-cell chain: osc → filter → mixer (no LFO)
        let osc_dna = CellDna {
            cell_type: "osc_cell".into(),
            params: [("freq", 440.0), ("det", 0.0), ("gain", 0.5)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("wtype", "sine")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let filter_dna = CellDna {
            cell_type: "filter_cell".into(),
            params: [("cutoff", 2000.0), ("res", 0.1)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("ftype", "lowpass")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let mixer_dna = CellDna {
            cell_type: "mixer_cell".into(),
            params: [("gain", 1.0), ("pan", 0.0)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: BTreeMap::new(),
        };

        let dna = OrganismDna {
            name: "test-chain".into(),
            species: "test".into(),
            active: true,
            seed: 1,
            version: 4,
            cells: vec![osc_dna, filter_dna, mixer_dna],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::Add,
                },
                CellWire {
                    src_cell: 1,
                    dst_cell: 2,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::Add,
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        };

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut any_nonzero = false;

        // Warmup
        for _ in 0..100 {
            org.tick(&mut output);
        }

        // Check for non-zero output
        for _ in 0..1000 {
            org.tick(&mut output);
            if output[0].abs() > 0.001 || output[1].abs() > 0.001 {
                any_nonzero = true;
                break;
            }
        }

        assert!(any_nonzero, "Expected 4-cell chain to produce audio");
    }

    #[test]
    fn wire_topological_order() {
        // Test that cells are processed in topological order
        // Create a chain: osc[0] → filter[1] → mixer[2]
        // If processed out of order, mixer would read stale filter output

        let osc_dna = CellDna {
            cell_type: "osc_cell".into(),
            params: [("freq", 440.0), ("det", 0.0), ("gain", 0.5)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("wtype", "sine")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let filter_dna = CellDna {
            cell_type: "filter_cell".into(),
            params: [("cutoff", 10000.0), ("res", 0.0)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("ftype", "lowpass")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let mixer_dna = CellDna {
            cell_type: "mixer_cell".into(),
            params: [("gain", 1.0), ("pan", 0.0)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: BTreeMap::new(),
        };

        let dna = OrganismDna {
            name: "topo-test".into(),
            species: "test".into(),
            active: true,
            seed: 2,
            version: 4,
            cells: vec![osc_dna, filter_dna, mixer_dna],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::Add,
                },
                CellWire {
                    src_cell: 1,
                    dst_cell: 2,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::Add,
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        };

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // If topological order is correct, output should be non-zero immediately
        // (osc → filter → mixer in one tick)
        for _ in 0..100 {
            org.tick(&mut output);
        }

        let mut peak = 0.0f32;
        for _ in 0..1000 {
            org.tick(&mut output);
            peak = peak.max(output[0].abs()).max(output[1].abs());
        }

        // Expect signal to pass through all 3 cells in the same tick
        assert!(
            peak > 0.01,
            "Expected peak > 0.01 with correct topological order, got {}",
            peak
        );
    }

    #[test]
    fn audio_wire_routes_signal() {
        // Bypassing a source cell with no inputs propagates silence downstream
        // (its pass-through = [0,0] since it has no audio inputs).
        // A separate test (bypass_passthrough_middle_cell) checks bypass on a middle cell.
        let osc_dna = CellDna {
            cell_type: "osc_cell".into(),
            params: [("freq", 440.0), ("det", 0.0), ("gain", 0.5)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("wtype", "sine")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let mixer_dna = CellDna {
            cell_type: "mixer_cell".into(),
            params: [("gain", 1.0), ("pan", 0.0)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: BTreeMap::new(),
        };

        let dna = OrganismDna {
            name: "bypass-test".into(),
            species: "test".into(),
            active: true,
            seed: 3,
            version: 4,
            cells: vec![osc_dna, mixer_dna],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Audio,
                gain: 1.0,
                mode: WireMode::Add,
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        };

        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Get bypass handle for osc_cell
        let bypass_handle = handles.get("cell0.bypass").unwrap();

        let mut output = [0.0f32; 2];

        // First verify audio is produced normally
        for _ in 0..100 {
            org.tick(&mut output);
        }
        let mut peak_normal = 0.0f32;
        for _ in 0..1000 {
            org.tick(&mut output);
            peak_normal = peak_normal.max(output[0].abs());
        }
        assert!(peak_normal > 0.01, "Expected audio before bypass");

        // Now bypass the oscillator
        bypass_handle.set(1.0);

        // Allow bypass to take effect
        for _ in 0..10 {
            org.tick(&mut output);
        }

        // Check that output is now silent
        let mut peak_bypassed = 0.0f32;
        for _ in 0..1000 {
            org.tick(&mut output);
            peak_bypassed = peak_bypassed.max(output[0].abs());
        }

        assert!(
            peak_bypassed < 0.001,
            "Expected silence after bypass, got peak {}",
            peak_bypassed
        );
    }

    #[test]
    fn bypass_passthrough_middle_cell() {
        // Bypassing a middle cell (filter) should be transparent — audio from upstream
        // still reaches downstream cells via the bypassed cell's pass-through scratch.
        let osc_dna = CellDna {
            cell_type: "osc_cell".into(),
            params: [("freq", 440.0), ("det", 0.0), ("gain", 0.5)]
                .iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            string_params: [("wtype", "sine")]
                .iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        let filter_dna = CellDna {
            cell_type: "filter_cell".into(),
            params: [("cutoff", 8000.0), ("res", 0.0)]
                .iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            string_params: [("ftype", "lowpass")]
                .iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        let mixer_dna = CellDna {
            cell_type: "mixer_cell".into(),
            params: [("gain", 1.0), ("pan", 0.0)]
                .iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            string_params: BTreeMap::new(),
        };
        let dna = OrganismDna {
            name: "bypass-middle-test".into(),
            species: "test".into(),
            active: true,
            seed: 7,
            version: 4,
            cells: vec![osc_dna, filter_dna, mixer_dna],
            cell_wiring: vec![
                CellWire { src_cell: 0, dst_cell: 1, wire_type: WireType::Audio, gain: 1.0, mode: WireMode::Add },
                CellWire { src_cell: 1, dst_cell: 2, wire_type: WireType::Audio, gain: 1.0, mode: WireMode::Add },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        };

        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();
        let filter_bypass = handles.get("cell1.bypass").unwrap().clone();

        let mut output = [0.0f32; 2];

        // Warmup
        for _ in 0..100 { org.tick(&mut output); }

        // Baseline: audio with filter active
        let mut peak_normal = 0.0f32;
        for _ in 0..1000 {
            org.tick(&mut output);
            peak_normal = peak_normal.max(output[0].abs());
        }
        assert!(peak_normal > 0.01, "Expected audio before bypass, got {}", peak_normal);

        // Bypass the filter — audio should still pass through
        filter_bypass.set(1.0);
        for _ in 0..10 { org.tick(&mut output); }

        let mut peak_bypassed = 0.0f32;
        for _ in 0..1000 {
            org.tick(&mut output);
            peak_bypassed = peak_bypassed.max(output[0].abs());
        }
        assert!(
            peak_bypassed > 0.01,
            "Expected audio after filter bypass (pass-through), got peak {}",
            peak_bypassed
        );
    }

    #[test]
    fn trigger_edge_detection() {
        // Test that trigger wires fire on rising edge only, not continuously
        // Use LFO with very slow rate to create controlled gate pattern

        let lfo_dna = CellDna {
            cell_type: "lfo_cell".into(),
            params: [("rate", 0.1), ("depth", 1.0)]  // Slow square wave
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: [("shape", "square")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let mixer_dna = CellDna {
            cell_type: "mixer_cell".into(),
            params: [("gain", 1.0), ("pan", 0.0)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            string_params: BTreeMap::new(),
        };

        let dna = OrganismDna {
            name: "trigger-edge-test".into(),
            species: "test".into(),
            active: true,
            seed: 99,
            version: 4,
            cells: vec![lfo_dna, mixer_dna],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Trigger,
                gain: 1.0,
                mode: WireMode::Add,
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        };

        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Tick until LFO crosses 0.5 threshold (rising edge)
        // Square wave LFO at 0.1 Hz with depth 1.0 outputs: -1.0 or +1.0
        // Need to find the rising edge: -1.0 → +1.0

        // Process samples and track when trigger_prev changes
        let mut found_rising_edge = false;
        let mut rising_edge_tick = 0;

        for tick in 0..1000 {
            org.tick(&mut output);

            // Check trigger_prev for cell 0 (LFO)
            let prev = org.trigger_prev[0];

            // LFO square wave will transition from negative to positive
            // When output goes from < 0.5 to > 0.5, that's a rising edge
            if org.scratch[0][0] > 0.5 && !found_rising_edge {
                found_rising_edge = true;
                rising_edge_tick = tick;

                // At this point, trigger_prev should have been updated
                assert!(
                    prev > 0.5,
                    "trigger_prev should be updated after rising edge at tick {}, got {}",
                    tick,
                    prev
                );
                break;
            }
        }

        assert!(found_rising_edge, "Should have found LFO rising edge within 1000 ticks");

        // Continue ticking while signal stays high
        // Verify trigger_prev tracks the signal but doesn't retrigger
        for _ in 0..100 {
            let prev_before = org.trigger_prev[0];
            org.tick(&mut output);
            let prev_after = org.trigger_prev[0];
            let current = org.scratch[0][0];

            // If signal is high, trigger_prev should track it
            if current > 0.5 {
                assert!(
                    (prev_after - current).abs() < 0.001,
                    "trigger_prev should track current signal when high"
                );
            }
        }

        // The test verifies:
        // 1. trigger_prev is updated when signal crosses threshold
        // 2. trigger_prev continues to track signal while high
        // 3. No crash or panic occurs (edge detection logic is sound)
        //
        // Note: We can't directly verify trigger commands aren't re-sent
        // because they're dispatched to mixer which ignores them. But the
        // edge detection logic ensures `curr > 0.5 && prev <= 0.5` is only
        // true once per rising edge.
    }

    // ========================================================================
    // HOSO Integration Tests (S21)
    // ========================================================================

    #[test]
    fn hoso_loads_dna() {
        // HOSO DNA should successfully build an OrganismDsp
        let json = std::fs::read_to_string("assets/dna/hoso-malabar.json")
            .expect("Failed to read hoso-malabar.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse hoso-malabar.json");

        assert_eq!(dna.version, 4, "Expected version 4");
        assert_eq!(dna.cells.len(), 8, "HOSO should have 8 cells");

        let result = OrganismDsp::from_dna(&dna, SR);
        assert!(
            result.is_some(),
            "Expected OrganismDsp to build from HOSO DNA"
        );
    }

    #[test]
    fn hoso_produces_audio() {
        // HOSO should produce audible signal from sequencer + osc + filter + mixer chain
        let json = std::fs::read_to_string("assets/dna/hoso-malabar.json")
            .expect("Failed to read hoso-malabar.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse hoso-malabar.json");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Tick for 1 second (sequencer should produce multiple steps)
        for _ in 0..(SR as usize) {
            org.tick(&mut output);
        }

        // Check RMS from mixer cell (cell 7)
        let rms = org.cells[7].analysis().rms;
        assert!(
            rms > 0.005,
            "HOSO should produce audible signal, got RMS={:.6}",
            rms
        );
    }

    #[test]
    fn hoso_seq_triggers_env() {
        // seq_cell should trigger env_cell via trigger wire
        let json = std::fs::read_to_string("assets/dna/hoso-malabar.json")
            .expect("Failed to read hoso-malabar.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse hoso-malabar.json");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Tick until first gate rising edge (seq_cell starts at step 0 with gate=1)
        // At 130 BPM, each step is ~0.46s = ~20000 samples
        // We should see env_cell[2] output rise within first step
        for _ in 0..5000 {
            org.tick(&mut output);
        }

        // env_cell[2] should be in attack/decay/sustain (value > 0)
        let env_output = org.scratch[2][0];
        assert!(
            env_output > 0.1,
            "Envelope should be active after seq trigger, got {:.6}",
            env_output
        );
    }

    #[test]
    fn hoso_accent_boosts_filter() {
        // accent_env_cell should fire on accented steps and modulate filter cutoff
        let json = std::fs::read_to_string("assets/dna/hoso-malabar.json")
            .expect("Failed to read hoso-malabar.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse hoso-malabar.json");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];

        // Tick through first accent step (step 0 has accent=1)
        // At 130 BPM, step duration ~0.46s = ~20000 samples
        // accent_env_cell[4] should fire immediately and decay
        for _ in 0..2000 {
            org.tick(&mut output);
        }

        // accent_env[4] should output > 0 during decay
        let accent_output = org.scratch[4][0];
        assert!(
            accent_output > 0.1,
            "Accent envelope should fire on accent step, got {:.6}",
            accent_output
        );
    }

    // SPGL Integration Tests (S22)

    #[test]
    fn spgl_loads_dna() {
        // SPGL DNA should successfully build an OrganismDsp
        let json = std::fs::read_to_string("assets/dna/spgl-kepler.json")
            .expect("Failed to read spgl-kepler.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse spgl-kepler.json");

        assert_eq!(dna.cells.len(), 8, "SPGL should have 8 cells");
        assert_eq!(dna.species, "spgl");

        let (org, _handles) = OrganismDsp::from_dna(&dna, SR).expect(
            "Expected OrganismDsp to build from SPGL DNA"
        );
        assert!(org.cells.len() > 0, "SPGL should have cells");
    }

    #[test]
    fn spgl_produces_audio() {
        // SPGL should produce non-zero stereo audio from seq → osc → filter → mixer chain
        let json = std::fs::read_to_string("assets/dna/spgl-kepler.json")
            .expect("Failed to read spgl-kepler.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse spgl-kepler.json");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut rms = 0.0f32;

        // Run for 2 seconds — sequencer needs time to trigger envelope
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut output);
            rms += output[0] * output[0] + output[1] * output[1];
        }

        rms = (rms / (SR as f32 * 4.0)).sqrt();
        assert!(
            rms > 0.001,
            "SPGL should produce audible signal, got RMS={:.6}",
            rms
        );
    }

    #[test]
    fn spgl_evolves_slowly() {
        // SPGL's audio character should change measurably over 30+ seconds
        // (func_gen[3] modulating filter cutoff with gain=500, period=120s)
        let json = std::fs::read_to_string("assets/dna/spgl-kepler.json")
            .expect("Failed to read spgl-kepler.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse spgl-kepler.json");

        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Sample filter cutoff at start (filter is cell 2)
        let cutoff_handle = handles.get("cell2.cutoff").expect("filter cutoff missing");
        let cutoff_start = cutoff_handle.value();

        let mut output = [0.0f32; 2];

        // Run for 30 seconds
        for _ in 0..(SR as usize * 30) {
            org.tick(&mut output);
        }

        // Filter cutoff should have changed (func_gen[3] modulates it)
        let cutoff_end = cutoff_handle.value();
        let cutoff_change = (cutoff_end - cutoff_start).abs();

        assert!(
            cutoff_change > 50.0,
            "SPGL cutoff should evolve over 30s (func_gen modulation), changed by {:.1} Hz",
            cutoff_change
        );
    }

    #[test]
    fn spgl_responds_to_scale() {
        // SPGL with low fidelity (generative drift) and high scale_affinity
        let json = std::fs::read_to_string("assets/dna/spgl-kepler.json")
            .expect("Failed to read spgl-kepler.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse spgl-kepler.json");

        assert!(
            dna.fidelity <= 0.2,
            "SPGL should have low fidelity (generative, not sequenced)"
        );
        assert_eq!(
            dna.scale_affinity, 0.9,
            "SPGL should have high scale_affinity"
        );
    }

    #[test]
    fn spgl_func_gen_modulates_filter() {
        // Verify that func_gen[3] modulates the filter cutoff (gain=500, depth=0.5)
        let json = std::fs::read_to_string("assets/dna/spgl-kepler.json")
            .expect("Failed to read spgl-kepler.json");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("Failed to parse spgl-kepler.json");

        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Get cutoff handle (filter is cell 2)
        let cutoff_handle = handles.get("cell2.cutoff").expect("filter cutoff missing");

        let mut output = [0.0f32; 2];
        let mut cutoff_min = f32::MAX;
        let mut cutoff_max = f32::MIN;

        // Run for 30 seconds — func_gen has 120s period, need enough for visible movement
        for _ in 0..(SR as usize * 30) {
            org.tick(&mut output);
            let cutoff = cutoff_handle.value();
            cutoff_min = cutoff_min.min(cutoff);
            cutoff_max = cutoff_max.max(cutoff);
        }

        // func_gen[3] modulates cutoff with gain=500, depth=0.5
        // Expected swing over 30s: partial cosine_sum cycle
        let swing = cutoff_max - cutoff_min;
        assert!(
            swing > 50.0,
            "SPGL func_gen should modulate filter cutoff, observed swing={:.1} Hz",
            swing
        );
    }

    #[test]
    fn dron_produces_audio() {
        // DRON: osc(soft_saw, 110Hz) → moog_filter → mixer → output
        let json = std::fs::read_to_string("assets/dna/dron-alpha.json")
            .expect("dron-alpha.json must exist");
        let dna: OrganismDna = serde_json::from_str(&json).expect("parse");
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut peak = 0.0f32;

        // Run 1 second
        for _ in 0..(SR as usize) {
            org.tick(&mut output);
            peak = peak.max(output[0].abs()).max(output[1].abs());
        }

        assert!(
            peak > 0.05,
            "DRON should produce significant audio, peak={:.6}",
            peak
        );
    }

    #[test]
    fn acid_loads_dna() {
        // acid-kinoko.json should parse and construct without error
        let json = std::fs::read_to_string("assets/dna/acid-kinoko.json")
            .expect("acid-kinoko.json must exist");
        let dna: OrganismDna = serde_json::from_str(&json).expect("acid-kinoko.json must parse");
        let result = OrganismDsp::from_dna(&dna, SR);
        assert!(result.is_some(), "ACID organism should construct from DNA");
    }

    #[test]
    fn acid_produces_audio() {
        // ACID organism should generate non-zero audio (squelchy 303 bass line)
        let json = std::fs::read_to_string("assets/dna/acid-kinoko.json")
            .expect("acid-kinoko.json must exist");
        let dna: OrganismDna = serde_json::from_str(&json).expect("acid-kinoko.json must parse");
        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut peak = 0.0f32;

        // At 138 BPM, first step = ~19100 samples. Run 2 seconds to hear multiple notes.
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut output);
            peak = peak.max(output[0].abs()).max(output[1].abs());
        }

        assert!(
            peak > 0.01,
            "ACID organism should produce audible audio within 2 seconds, peak={}",
            peak
        );
    }

    // ========================================================================
    // WireMode::Replace tests
    // ========================================================================

    /// Helper: build DNA with LFO→filter modulation using a specific WireMode
    fn make_replace_test_dna(mode: WireMode, base_cutoff: f32) -> OrganismDna {
        let mut lfo_params = BTreeMap::new();
        lfo_params.insert("rate".into(), 1.0);
        lfo_params.insert("depth".into(), 1.0);
        let mut lfo_string_params = BTreeMap::new();
        lfo_string_params.insert("shape".into(), "sine".into());

        let mut filter_params = BTreeMap::new();
        filter_params.insert("cutoff".into(), base_cutoff);
        filter_params.insert("res".into(), 0.1);
        let mut filter_string_params = BTreeMap::new();
        filter_string_params.insert("ftype".into(), "lowpass".into());

        let mut mixer_params = BTreeMap::new();
        mixer_params.insert("gain".into(), 1.0);
        mixer_params.insert("pan".into(), 0.0);

        OrganismDna {
            name: "replace-test".into(),
            species: "test".into(),
            active: true,
            seed: 200,
            version: 4,
            cells: vec![
                CellDna { cell_type: "lfo_cell".into(), params: lfo_params, string_params: lfo_string_params },
                CellDna { cell_type: "filter_cell".into(), params: filter_params, string_params: filter_string_params },
                CellDna { cell_type: "mixer_cell".into(), params: mixer_params, string_params: BTreeMap::new() },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Modulation { target_param: "cutoff".into() },
                    gain: 500.0,
                    mode,
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.5,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
            base_chaos: 0.0,
            chaos_sensitivity: 0.0,
        }
    }

    #[test]
    fn mod_replace_ignores_base() {
        // Replace mode: cutoff = mod_signal * gain, base is ignored
        let base_cutoff = 5000.0;
        let dna = make_replace_test_dna(WireMode::Replace, base_cutoff);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).expect("build failed");
        let cutoff_handle = handles.get("cell1.cutoff").expect("cutoff handle");

        let mut output = [0.0f32; 2];
        // Run enough to let LFO produce non-zero output
        for _ in 0..200 {
            org.tick(&mut output);
        }

        let cutoff = cutoff_handle.value();
        // With Replace mode and gain=500, cutoff = lfo_output * 500
        // LFO sine at 1Hz swings [-1,1], so cutoff should be in [-500, 500] clamped to [20, 20000]
        // Crucially, it should NOT be near 5000 (the base value)
        assert!(
            cutoff < 1000.0,
            "Replace mode should ignore base (5000), got cutoff={}",
            cutoff
        );
    }

    #[test]
    fn mod_replace_clamps_to_range() {
        // Replace mode still respects param range clamping
        let dna = make_replace_test_dna(WireMode::Replace, 1000.0);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).expect("build failed");
        let cutoff_handle = handles.get("cell1.cutoff").expect("cutoff handle");

        let mut output = [0.0f32; 2];
        for _ in 0..2000 {
            org.tick(&mut output);
            let cutoff = cutoff_handle.value();
            assert!(
                cutoff >= 20.0 && cutoff <= 20000.0,
                "Replace mode cutoff {} out of range [20, 20000]",
                cutoff
            );
        }
    }

    #[test]
    fn mod_replace_falls_back_to_base_when_bypassed() {
        // When source is bypassed, Replace mode should revert to base value
        let base_cutoff = 3000.0;
        let dna = make_replace_test_dna(WireMode::Replace, base_cutoff);
        let (mut org, handles) = OrganismDsp::from_dna(&dna, SR).expect("build failed");
        let cutoff_handle = handles.get("cell1.cutoff").expect("cutoff handle");
        let bypass_handle = handles.get("cell0.bypass").expect("bypass handle");

        let mut output = [0.0f32; 2];
        // Run to let modulation take effect
        for _ in 0..500 {
            org.tick(&mut output);
        }
        let cutoff_modulated = cutoff_handle.value();
        assert!(
            (cutoff_modulated - base_cutoff).abs() > 10.0,
            "Cutoff should be modulated away from base, got {}",
            cutoff_modulated
        );

        // Bypass the LFO source
        bypass_handle.set(1.0);
        for _ in 0..10 {
            org.tick(&mut output);
        }
        let cutoff_bypassed = cutoff_handle.value();
        // Should revert to base because Pass 1 resets to base when source is NOT bypassed,
        // and skips when source IS bypassed — so the param stays at base from last reset
        assert!(
            (cutoff_bypassed - base_cutoff).abs() < 1.0,
            "Bypassed source should revert to base ({}), got {}",
            base_cutoff,
            cutoff_bypassed
        );
    }

    #[test]
    fn isao_loads_dna() {
        let json = std::fs::read_to_string("assets/dna/isao-tomita.json")
            .expect("isao-tomita.json must exist");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("isao-tomita.json must parse");
        let result = OrganismDsp::from_dna(&dna, SR);
        assert!(result.is_some(), "ISAO organism should construct from DNA");
    }

    #[test]
    fn isao_produces_audio() {
        // ISAO: seq → slew → saw_bank → diode_filter → mixer
        // Melodic lead must produce audible output.
        //
        // Key: slew_cell receives 2-ch input from seq_cell (ch0=gate, ch1=pitch).
        // Before the input truncation fix, slew_cell (output_channels=1) only got
        // ch0 (gate 0/1), so saw_bank freq stayed at 20Hz (clamp min) = silence.
        let json = std::fs::read_to_string("assets/dna/isao-tomita.json")
            .expect("isao-tomita.json must exist");
        let dna: OrganismDna =
            serde_json::from_str(&json).expect("isao-tomita.json must parse");
        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut rms = 0.0f32;
        let mut peak = 0.0f32;
        let warmup = (SR * 0.1) as usize; // 100ms warmup

        // Run 2 seconds, measure RMS and peak after warmup
        let total = SR as usize * 2;
        for i in 0..total {
            org.tick(&mut output);
            if i >= warmup {
                rms += output[0] * output[0] + output[1] * output[1];
                let sample_peak = output[0].abs().max(output[1].abs());
                if sample_peak > peak {
                    peak = sample_peak;
                }
            }
        }

        let samples = (total - warmup) as f32;
        rms = (rms / (samples * 2.0)).sqrt();
        assert!(
            rms > 0.01,
            "ISAO should produce audible melodic signal, got RMS={:.6}",
            rms
        );
        assert!(
            peak > 0.05,
            "ISAO should have audible peaks, got peak={:.6}",
            peak
        );
    }

    #[test]
    fn wiremode_replace_serde_roundtrip() {
        let wire = CellWire {
            src_cell: 0,
            dst_cell: 1,
            wire_type: WireType::Audio,
            gain: 1.0,
            mode: WireMode::Replace,
        };
        let json = serde_json::to_string(&wire).unwrap();
        let loaded: CellWire = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.mode, WireMode::Replace);
    }
}

#[cfg(test)]
mod click_diagnostic {
    use super::*;

    const SR: f32 = 44100.0;

    #[test]
    fn click_detector_all_organisms() {
        let dna_files = [
            "assets/dna/dron-alpha.json",
            "assets/dna/hoso-malabar.json",
            "assets/dna/spgl-kepler.json",
            "assets/dna/acid-kinoko.json",
            "assets/dna/kkit-909.json",
            "assets/dna/tblk-dha.json",
            "assets/dna/isao-tomita.json",
        ];

        for path in &dna_files {
            let json = match std::fs::read_to_string(path) {
                Ok(j) => j,
                Err(_) => { println!("SKIP: {} not found", path); continue; }
            };
            let dna: OrganismDna = serde_json::from_str(&json)
                .expect(&format!("Failed to parse {}", path));

            let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR)
                .expect(&format!("Failed to build {}", path));

            let mut output = [0.0f32; 2];
            let mut prev_l = 0.0f32;
            let mut prev_r = 0.0f32;
            let mut click_count = 0u32;
            let mut max_jump = 0.0f32;
            let mut peak = 0.0f32;
            let mut nan_count = 0u32;
            let mut inf_count = 0u32;
            let click_threshold = 0.3; // Sample-to-sample jump > 0.3 = click

            let total_samples = SR as usize * 3; // 3 seconds
            for i in 0..total_samples {
                org.tick(&mut output);

                if output[0].is_nan() || output[1].is_nan() { nan_count += 1; }
                if output[0].is_infinite() || output[1].is_infinite() { inf_count += 1; }

                peak = peak.max(output[0].abs()).max(output[1].abs());

                if i > 0 {
                    let jump_l = (output[0] - prev_l).abs();
                    let jump_r = (output[1] - prev_r).abs();
                    let jump = jump_l.max(jump_r);
                    max_jump = max_jump.max(jump);
                    if jump > click_threshold {
                        click_count += 1;
                    }
                }

                prev_l = output[0];
                prev_r = output[1];
            }

            let name = std::path::Path::new(path).file_stem().unwrap().to_str().unwrap();
            println!("\n=== {} ===", name);
            println!("  peak:       {:.4}", peak);
            println!("  max_jump:   {:.4}", max_jump);
            println!("  clicks:     {} (threshold {})", click_count, click_threshold);
            println!("  NaN:        {}", nan_count);
            println!("  Inf:        {}", inf_count);

            assert_eq!(nan_count, 0, "{} produced NaN!", name);
            assert_eq!(inf_count, 0, "{} produced Inf!", name);
        }
    }
}

#[cfg(test)]
mod micro_tuning_tests {
    use super::*;

    #[test]
    fn cents_distance_basic() {
        assert!((cents_distance(0.0, 100.0) - 100.0).abs() < 0.01);
        assert!((cents_distance(100.0, 0.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn cents_distance_wraps() {
        // 1150 to 50: direct = 1100, wrapped = 100
        assert!((cents_distance(1150.0, 50.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn combined_tuning_merge() {
        // 12-TET C# at 100 cents + raga komal Re at 112 → merged at 112 cents
        let mut dsp = make_micro_test_dsp();
        let mut weights = [0.0f32; 12];
        weights[0] = 1.0; // C at 0
        weights[1] = 1.0; // C# at 100
        dsp.scale_weights = weights;
        dsp.scale_blend = 1.0;

        let mut mc = [0.0f32; MAX_MICRO_DEGREES];
        let mut mw = [0.0f32; MAX_MICRO_DEGREES];
        mc[0] = 112.0; // komal Re
        mw[0] = 1.5;
        dsp.micro_cents = mc;
        dsp.micro_weights = mw;
        dsp.micro_count = 1;
        dsp.micro_blend = 1.0;
        dsp.combined_dirty = true;
        dsp.rebuild_combined();

        // Should have 2 degrees: C at 0, merged komal Re at 112 (replaced C# at 100)
        assert_eq!(dsp.combined.count, 2);
        // Find the merged degree — it should be at 112 (micro won position)
        let mut found_112 = false;
        for i in 0..dsp.combined.count as usize {
            if (dsp.combined.cents[i] - 112.0).abs() < 0.01 {
                found_112 = true;
                assert!(dsp.combined.weights[i] >= 1.5, "merged weight should be max");
            }
        }
        assert!(found_112, "komal Re at 112 should be present");
    }

    #[test]
    fn combined_tuning_independent() {
        // Raga degree at 550 cents should stay separate from F(500) and F#(600)
        // (50 cents from each, well beyond 20-cent merge tolerance)
        let mut dsp = make_micro_test_dsp();
        let mut weights = [0.0f32; 12];
        weights[5] = 1.0; // F at 500
        weights[6] = 1.0; // F# at 600
        dsp.scale_weights = weights;
        dsp.scale_blend = 1.0;

        let mut mc = [0.0f32; MAX_MICRO_DEGREES];
        let mut mw = [0.0f32; MAX_MICRO_DEGREES];
        mc[0] = 550.0; // 50 cents from each neighbor — stays separate
        mw[0] = 2.0;
        dsp.micro_cents = mc;
        dsp.micro_weights = mw;
        dsp.micro_count = 1;
        dsp.micro_blend = 1.0;
        dsp.combined_dirty = true;
        dsp.rebuild_combined();

        // Should have 3 degrees: F at 500, F# at 600, micro at 550
        assert_eq!(dsp.combined.count, 3);
    }

    #[test]
    fn combined_tuning_near_merge() {
        // Raga degree at 590 cents is within 20 cents of F#(600) — should merge
        let mut dsp = make_micro_test_dsp();
        let mut weights = [0.0f32; 12];
        weights[6] = 1.0; // F# at 600
        dsp.scale_weights = weights;
        dsp.scale_blend = 1.0;

        let mut mc = [0.0f32; MAX_MICRO_DEGREES];
        let mut mw = [0.0f32; MAX_MICRO_DEGREES];
        mc[0] = 590.0;
        mw[0] = 2.0;
        dsp.micro_cents = mc;
        dsp.micro_weights = mw;
        dsp.micro_count = 1;
        dsp.micro_blend = 1.0;
        dsp.combined_dirty = true;
        dsp.rebuild_combined();

        // Should merge: 1 degree at 590 (micro wins position), weight = max(1.0, 2.0) = 2.0
        assert_eq!(dsp.combined.count, 1);
        assert!((dsp.combined.cents[0] - 590.0).abs() < 0.01, "micro should win position");
        assert!((dsp.combined.weights[0] - 2.0).abs() < 0.01, "weight should be max");
    }

    #[test]
    fn quantize_to_tuning_exact_raga() {
        // Hz near komal Re (112 cents above C4) should snap to 112 not 100
        let tuning = CombinedTuning {
            cents: {
                let mut c = [0.0f32; 24];
                c[0] = 0.0;   // Sa
                c[1] = 112.0; // komal Re (raga position, NOT 12-TET 100)
                c
            },
            weights: {
                let mut w = [0.0f32; 24];
                w[0] = 1.0;
                w[1] = 1.5;
                w
            },
            count: 2,
        };

        // 115 cents above C4 ≈ 279.4 Hz — should snap to 112 cents
        let input_hz = 261.63 * 2.0f32.powf(115.0 / 1200.0);
        let result = quantize_to_tuning(input_hz, &tuning, 1.0);
        let expected_hz = 261.63 * 2.0f32.powf(112.0 / 1200.0);
        assert!(
            (result - expected_hz).abs() < 0.5,
            "should snap to 112 cents (komal Re), got {} vs expected {}",
            result, expected_hz
        );
    }

    #[test]
    fn quantize_to_tuning_non_raga_chromatic() {
        // Hz near D (200 cents) still quantizes via base scale
        let tuning = CombinedTuning {
            cents: {
                let mut c = [0.0f32; 24];
                c[0] = 0.0;   // C
                c[1] = 200.0; // D
                c[2] = 400.0; // E
                c
            },
            weights: {
                let mut w = [0.0f32; 24];
                w[0] = 1.0; w[1] = 1.0; w[2] = 1.0;
                w
            },
            count: 3,
        };

        // 190 cents → should snap to 200 (D)
        let input_hz = 261.63 * 2.0f32.powf(190.0 / 1200.0);
        let result = quantize_to_tuning(input_hz, &tuning, 1.0);
        let expected_hz = 261.63 * 2.0f32.powf(200.0 / 1200.0);
        assert!(
            (result - expected_hz).abs() < 0.5,
            "should snap to 200 cents (D), got {} vs expected {}",
            result, expected_hz
        );
    }

    #[test]
    fn quantize_bypass_when_blend_zero() {
        let tuning = CombinedTuning {
            cents: { let mut c = [0.0f32; 24]; c[0] = 0.0; c },
            weights: { let mut w = [0.0f32; 24]; w[0] = 1.0; w },
            count: 1,
        };
        let result = quantize_to_tuning(440.0, &tuning, 0.0);
        assert!((result - 440.0).abs() < 0.01, "blend=0 should pass through");
    }

    #[test]
    fn set_micro_tuning_marks_dirty() {
        let mut dsp = make_micro_test_dsp();
        assert!(!dsp.combined_dirty);
        dsp.handle_command(DspCommand::SetMicroTuning {
            cents: [0.0; MAX_MICRO_DEGREES],
            weights: [0.0; MAX_MICRO_DEGREES],
            count: 0,
            blend: 0.0,
        });
        assert!(dsp.combined_dirty);
    }

    /// Minimal OrganismDsp for testing micro tuning (no cells needed).
    fn make_micro_test_dsp() -> OrganismDsp {
        OrganismDsp {
            cells: vec![],
            wiring: vec![],
            scratch: vec![],
            bypassed: vec![],
            output: [0.0; 2],
            tick_order: vec![],
            terminal_cells: vec![],
            handle_vec: vec![],
            trigger_prev: vec![],
            trigger_commands: vec![],
            mod_wires: vec![],
            sample_rate: 44100.0,
            cell_indices: std::collections::HashMap::new(),
            seq_cell_idx: None,
            env_cell_idx: None,
            logic_seq_cell_idx: None,
            seq_chaos_handle_id: None,
            logic_density_handle_id: None,
            osc_freq_handles: vec![],
            scale_weights: [0.0; 12],
            scale_blend: 0.0,
            micro_cents: [0.0; MAX_MICRO_DEGREES],
            micro_weights: [0.0; MAX_MICRO_DEGREES],
            micro_count: 0,
            micro_blend: 0.0,
            combined: CombinedTuning {
                cents: [0.0; 24],
                weights: [0.0; 24],
                count: 0,
            },
            combined_dirty: false,
        }
    }
}
