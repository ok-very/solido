use std::collections::HashMap;

use crate::dsp::shared::{shared, HandleId, Shared};

use crate::dsp::cell::{build_cell, find_range, DspCell};
use crate::dsp::combined_tuning::{
    cents_distance, quantize_to_tuning, CombinedTuning, MICRO_MERGE_TOLERANCE,
};
use crate::dsp::command::{DspAnalysis, DspCommand, MAX_MICRO_DEGREES};
use crate::organism::dna::{OrganismDna, WireMode, WireType};
use crate::samples::SampleRegistry;

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
    /// Precomputed index for call_response_cell (bridge data: state + phrase length).
    cr_cell_idx: Option<usize>,
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
    Modulation { target_param: String, gain: f32, mode: WireMode, source_channel: usize },
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
    /// Which output channel of the source cell to read (0 or 1).
    source_channel: usize,
}

impl OrganismDsp {
    /// Build from DNA blueprint. Returns (OrganismDsp, SharedHandles).
    /// SharedHandles are cloned Shared references for the control thread.
    /// Pass `registry` to resolve `sample_uri` references in sample_cells.
    pub fn from_dna(
        dna: &OrganismDna,
        sr: f32,
        mut registry: Option<&mut SampleRegistry>,
    ) -> Option<(Self, SharedHandles)> {
        let mut cells: Vec<Box<dyn DspCell>> = Vec::new();
        let mut all_handles = SharedHandles::new();
        let mut handle_vec: Vec<Shared> = Vec::new();
        let mut handle_map: HashMap<String, HandleId> = HashMap::new();

        let mut bypassed = Vec::new();
        for (i, cell_dna) in dna.cells.iter().enumerate() {
            let (cell, handles) = build_cell(cell_dna, sr, registry.as_deref_mut())?;
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
                    WireType::Modulation { target_param, source_channel } => WireTag::Modulation {
                        target_param: target_param.clone(),
                        gain: w.gain,
                        mode: w.mode.clone(),
                        source_channel: *source_channel as usize,
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
                "seq_cell" | "env_cell" | "slew_cell" | "lfo_cell" | "accent_env_cell" | "func_gen_cell" | "logic_seq_cell" | "walk_cell" | "xy_pad_cell" | "melodic_cell" | "call_response_cell"
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
        let cr_cell_idx = cell_indices.get("call_response_cell").copied();

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
            if let WireTag::Modulation { target_param, gain, mode, source_channel } = tag {
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
                    source_channel: *source_channel,
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
                cr_cell_idx,
                seq_chaos_handle_id,
                logic_density_handle_id,
                osc_freq_handles,
                scale_weights: [0.0; 12],
                scale_blend: 0.0,
                micro_cents: [0.0; MAX_MICRO_DEGREES],
                micro_weights: [0.0; MAX_MICRO_DEGREES],
                micro_count: 0,
                micro_blend: 0.0,
                combined: CombinedTuning::new(),
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
        //
        // Threshold is low (0.01) to support velocity-encoded gates from call_response_cell,
        // where gate value = velocity [0.05..1.0] instead of binary 0/1.
        // All legacy cells output 0.0 or 1.0, so lowering from 0.5 to 0.01 is backward-compatible.
        const TRIGGER_GATE_THRESHOLD: f32 = 0.01;

        self.trigger_commands.clear();
        for (src, dst, tag) in &self.wiring {
            if self.bypassed[*src].value() > 0.5 || self.bypassed[*dst].value() > 0.5 {
                continue;
            }
            if let WireTag::Trigger = tag {
                let prev = self.trigger_prev[*src];
                let curr = self.scratch[*src][0];

                // Rising edge: prev was low, curr is high → NoteOn
                if curr > TRIGGER_GATE_THRESHOLD && prev <= TRIGGER_GATE_THRESHOLD {
                    let vel = curr.clamp(0.0, 1.0);
                    // Forward pitch from source ch1 (seq_cell outputs gate on ch0, pitch Hz on ch1)
                    let freq = self.scratch[*src][1];
                    self.trigger_commands.push((
                        *dst,
                        DspCommand::NoteOn {
                            freq,
                            velocity: vel,
                        },
                    ));
                }

                // Falling edge: prev was high, curr is low → NoteOff
                // Gives sample_cell crisp gate-off for rhythmic articulation.
                if curr <= TRIGGER_GATE_THRESHOLD && prev > TRIGGER_GATE_THRESHOLD {
                    self.trigger_commands.push((*dst, DspCommand::NoteOff));
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
            let mod_signal = self.scratch[wire.src][wire.source_channel.min(1)];

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
                // Forward to cells so walk_cell/melodic_cell can use gravity
                for cell in &mut self.cells {
                    cell.handle_command(&cmd);
                }
            }
            DspCommand::SetMicroTuning { cents, weights, count, blend } => {
                self.micro_cents = cents;
                self.micro_weights = weights;
                self.micro_count = count;
                self.micro_blend = blend;
                self.combined_dirty = true;
                // Forward to cells for cell-level quantization
                for cell in &mut self.cells {
                    cell.handle_command(&cmd);
                }
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
            cr_state: bridge.cr_state,
            cr_phrase_len: bridge.cr_phrase_len,
            cr_listening: bridge.cr_listening,
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

        // Call-response cell bridge data
        let (cr_state, cr_phrase_len, cr_listening) = self.cr_cell_idx
            .map(|idx| {
                let cell = &self.cells[idx];
                if let Some(cr) = cell.as_any().downcast_ref::<crate::dsp::cell::call_response_cell::CallResponseCell>() {
                    (cr.state_code(), cr.phrase_len(), cr.listening())
                } else {
                    (0u8, 0u8, false)
                }
            })
            .unwrap_or((0, 0, false));

        BridgeData {
            seq_pitch_hz,
            seq_gate,
            env_level,
            spectral_centroid,
            seq_chaos,
            logic_density,
            cr_state,
            cr_phrase_len,
            cr_listening,
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
    /// Call-response cell state: 0=Idle, 1=Listen, 2=Respond
    pub cr_state: u8,
    /// Number of notes in captured phrase
    pub cr_phrase_len: u8,
    /// Whether call-response cell is in capture-armed mode
    pub cr_listening: bool,
}

/// Soft-clip using tanh. Smooth saturation, linear below ±0.5, approaches ±1.
fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

#[cfg(test)]
#[path = "organism_dsp_tests.rs"]
mod organism_dsp_tests;
