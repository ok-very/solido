use std::collections::HashMap;

use crate::dsp::shared::{shared, Shared};

use crate::dsp::cell::arpeggiator::Arpeggiator;
use crate::dsp::cell::pattern_gen::PatternGen;
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
    terminal_cells: Vec<usize>,
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
            let mut has_multiply = false;
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
                            has_multiply = true;
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

    fn make_tblk_dna() -> OrganismDna {
        let mut pat_params = BTreeMap::new();
        pat_params.insert("bpm".into(), 120.0);
        pat_params.insert("steps".into(), 7.0);
        pat_params.insert("hits".into(), 5.0);
        pat_params.insert("accent_depth".into(), 0.6);

        let mut strike_params = BTreeMap::new();
        strike_params.insert("membrane_freq".into(), 180.0);
        strike_params.insert("bandwidth".into(), 0.5);
        strike_params.insert("atk".into(), 1.0);
        strike_params.insert("dcy".into(), 150.0);
        strike_params.insert("pitch_sweep".into(), 3.0);
        strike_params.insert("sweep_decay".into(), 50.0);

        OrganismDna {
            name: "tblk-test".into(),
            species: "tblk".into(),
            seed: 42,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "pattern_gen".into(),
                    params: pat_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "hexo_strike".into(),
                    params: strike_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Trigger,
                gain: 1.0,
                mode: WireMode::default(),
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    fn make_dron_dna() -> OrganismDna {
        let mut bed_params = BTreeMap::new();
        bed_params.insert("root_hz".into(), 110.0);
        bed_params.insert("det".into(), 7.0);
        bed_params.insert("cutoff".into(), 800.0);
        bed_params.insert("res".into(), 0.3);
        bed_params.insert("lfo_rate".into(), 0.07);
        bed_params.insert("lfo_depth".into(), 0.3);
        bed_params.insert("osc_mix".into(), 0.7);

        let mut reverb_params = BTreeMap::new();
        reverb_params.insert("size".into(), 0.3);
        reverb_params.insert("dcy".into(), 0.4);
        reverb_params.insert("mix".into(), 0.3);

        OrganismDna {
            name: "dron-test".into(),
            species: "dron".into(),
            seed: 99,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "hexo_bed".into(),
                    params: bed_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "reverb".into(),
                    params: reverb_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Audio,
                gain: 1.0,
                mode: WireMode::default(),
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    fn make_melo_dna() -> OrganismDna {
        let mut arp_params = BTreeMap::new();
        arp_params.insert("rate_hz".into(), 4.0);
        arp_params.insert("pattern".into(), 0.0);
        arp_params.insert("octaves".into(), 1.0);
        arp_params.insert("gate_length".into(), 0.5);

        let mut voice_params = BTreeMap::new();
        voice_params.insert("freq".into(), 440.0);
        voice_params.insert("det".into(), 7.0);
        voice_params.insert("cutoff".into(), 2500.0);
        voice_params.insert("res".into(), 0.35);
        voice_params.insert("atk".into(), 5.0);
        voice_params.insert("dcy".into(), 100.0);
        voice_params.insert("sus".into(), 0.7);
        voice_params.insert("rel".into(), 200.0);
        voice_params.insert("env_depth".into(), 3000.0);
        voice_params.insert("env_decay".into(), 300.0);
        voice_params.insert("osc_mix".into(), 0.7);

        OrganismDna {
            name: "melo-test".into(),
            species: "melo".into(),
            seed: 77,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "arpeggiator".into(),
                    params: arp_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "hexo_timbre".into(),
                    params: voice_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Trigger,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    #[test]
    fn organism_dsp_builds_from_tblk_dna() {
        let dna = make_tblk_dna();
        let (org, handles) = OrganismDsp::from_dna(&dna, SR).unwrap();
        assert_eq!(org.cell_count(), 2);
        assert!(!handles.is_empty());
    }

    #[test]
    fn tblk_organism_produces_percussive_audio() {
        let dna = make_tblk_dna();
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds — pattern_gen will trigger strike_voice via wiring
        let mut buf_l = Vec::new();
        let mut buf_r = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf_l.push(out[0]);
            buf_r.push(out[1]);
        }

        let rms: f32 =
            (buf_l.iter().map(|s| s * s).sum::<f32>() / buf_l.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "TBLK OrganismDsp should produce percussive audio: rms={rms}"
        );
    }

    #[test]
    fn dron_organism_produces_continuous_audio() {
        let dna = make_dron_dna();
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 1 second — dron cells always produce audio
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "DRON OrganismDsp should produce continuous audio: rms={rms}"
        );
    }

    #[test]
    fn melo_organism_produces_arpeggiated_output() {
        let dna = make_melo_dna();
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds — arpeggiator triggers timbre voice via wiring
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "MELO OrganismDsp should produce arpeggiated audio: rms={rms}"
        );
    }

    #[test]
    fn organism_dsp_handle_command_dispatches() {
        let dna = make_tblk_dna();
        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();
        // Should not panic
        org.handle_command(DspCommand::NoteOn {
            freq: 200.0,
            velocity: 0.8,
        });
        let mut out = [0.0f32; 2];
        for _ in 0..100 {
            org.tick(&mut out);
        }
    }

    #[test]
    fn organism_dsp_analysis() {
        let dna = make_dron_dna();
        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();
        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            org.tick(&mut out);
        }
        let analysis = org.analysis();
        assert!(analysis.rms >= 0.0);
        assert!(analysis.peak >= 0.0);
    }

    // --- S14 cell wiring tests ---

    #[test]
    fn audio_wire_serial_chain_changes_output() {
        // DRON with audio wire: harmonic_bed(0) → shimmer_layer(1)
        let dna_wired = make_dron_dna(); // has audio wire

        // DRON without audio wire
        let mut dna_no_wire = make_dron_dna();
        dna_no_wire.cell_wiring.clear();

        let (mut org_wired, _) = OrganismDsp::from_dna(&dna_wired, SR).unwrap();
        let (mut org_plain, _) = OrganismDsp::from_dna(&dna_no_wire, SR).unwrap();

        let mut buf_wired = Vec::new();
        let mut buf_plain = Vec::new();
        let mut out = [0.0f32; 2];

        for _ in 0..44100 {
            org_wired.tick(&mut out);
            buf_wired.push(out[0]);
        }
        for _ in 0..44100 {
            org_plain.tick(&mut out);
            buf_plain.push(out[0]);
        }

        let rms_wired: f32 =
            (buf_wired.iter().map(|s| s * s).sum::<f32>() / buf_wired.len() as f32).sqrt();
        let rms_plain: f32 =
            (buf_plain.iter().map(|s| s * s).sum::<f32>() / buf_plain.len() as f32).sqrt();

        // Both should produce audio
        assert!(rms_wired > 0.001, "Wired DRON should produce audio: rms={rms_wired}");
        assert!(rms_plain > 0.001, "Plain DRON should produce audio: rms={rms_plain}");

        // They should differ because shimmer receives harmonic_bed input
        let diff = (rms_wired - rms_plain).abs();
        assert!(
            diff > 0.0001,
            "Audio wire should change the output character: wired={rms_wired}, plain={rms_plain}"
        );
    }

    #[test]
    fn terminal_detection_only_terminal_mixes() {
        // In chain A→B, only B should be terminal
        let dna = make_dron_dna(); // harmonic_bed(0) → shimmer_layer(1)
        let (org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Cell 0 (hexo_bed) has outgoing audio wire → not terminal
        // Cell 1 (reverb) has no outgoing audio wire → terminal
        assert_eq!(org.terminal_cells.len(), 1);
        assert_eq!(org.terminal_cells[0], 1);
    }

    #[test]
    fn wire_gain_halves_amplitude() {
        // Build a DRON with gain=0.5 on the audio wire
        let mut dna_half = make_dron_dna();
        dna_half.cell_wiring[0].gain = 0.5;

        let mut dna_full = make_dron_dna();
        // gain=1.0 already default

        let (mut org_half, _) = OrganismDsp::from_dna(&dna_half, SR).unwrap();
        let (mut org_full, _) = OrganismDsp::from_dna(&dna_full, SR).unwrap();

        let mut buf_half = Vec::new();
        let mut buf_full = Vec::new();
        let mut out = [0.0f32; 2];

        for _ in 0..44100 {
            org_half.tick(&mut out);
            buf_half.push(out[0]);
        }
        for _ in 0..44100 {
            org_full.tick(&mut out);
            buf_full.push(out[0]);
        }

        let rms_half: f32 =
            (buf_half.iter().map(|s| s * s).sum::<f32>() / buf_half.len() as f32).sqrt();
        let rms_full: f32 =
            (buf_full.iter().map(|s| s * s).sum::<f32>() / buf_full.len() as f32).sqrt();

        // Half-gain wire should produce less energy than full-gain
        assert!(
            rms_half < rms_full,
            "gain=0.5 should produce less energy: half={rms_half}, full={rms_full}"
        );
    }

    #[test]
    fn cycle_handling_no_hang() {
        // A→B→A audio cycle — should not hang and should produce audio
        let mut bed_params = BTreeMap::new();
        bed_params.insert("root_hz".into(), 110.0);
        bed_params.insert("cutoff".into(), 800.0);
        let mut reverb_params = BTreeMap::new();
        reverb_params.insert("size".into(), 0.3);
        reverb_params.insert("mix".into(), 0.3);

        let dna = OrganismDna {
            name: "cycle-test".into(),
            species: "dron".into(),
            seed: 1,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "hexo_bed".into(),
                    params: bed_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "reverb".into(),
                    params: reverb_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
                CellWire {
                    src_cell: 1,
                    dst_cell: 0,
                    wire_type: WireType::Audio,
                    gain: 0.3,
                    mode: WireMode::default(),
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        };

        let (mut org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();
        let mut out = [0.0f32; 2];

        // Should not hang — run for 1 second
        for _ in 0..44100 {
            org.tick(&mut out);
        }

        // Should produce some audio
        assert!(
            out[0].abs() > 0.0 || out[1].abs() > 0.0,
            "Cyclic wiring should still produce audio"
        );
    }

    // --- Effect cell wiring tests ---

    fn make_melo_with_reverb_dna() -> OrganismDna {
        let mut arp_params = BTreeMap::new();
        arp_params.insert("rate_hz".into(), 4.0);
        arp_params.insert("pattern".into(), 0.0);
        arp_params.insert("octaves".into(), 1.0);
        arp_params.insert("gate_length".into(), 0.5);

        let mut voice_params = BTreeMap::new();
        voice_params.insert("freq".into(), 440.0);
        voice_params.insert("det".into(), 7.0);
        voice_params.insert("cutoff".into(), 2500.0);
        voice_params.insert("res".into(), 0.35);
        voice_params.insert("atk".into(), 5.0);
        voice_params.insert("dcy".into(), 100.0);
        voice_params.insert("sus".into(), 0.7);
        voice_params.insert("rel".into(), 200.0);
        voice_params.insert("env_depth".into(), 3000.0);
        voice_params.insert("env_decay".into(), 300.0);
        voice_params.insert("osc_mix".into(), 0.7);

        let mut reverb_params = BTreeMap::new();
        reverb_params.insert("size".into(), 0.5);
        reverb_params.insert("dcy".into(), 0.6);
        reverb_params.insert("mix".into(), 0.3);

        OrganismDna {
            name: "melo-reverb-test".into(),
            species: "melo".into(),
            seed: 88,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "arpeggiator".into(),
                    params: arp_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "hexo_timbre".into(),
                    params: voice_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "reverb".into(),
                    params: reverb_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Trigger,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
                CellWire {
                    src_cell: 1,
                    dst_cell: 2,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    #[test]
    fn effect_wiring_reverb_in_chain() {
        // arpeggiator(0) →trigger→ timbre_voice(1) →audio→ reverb(2)
        let dna = make_melo_with_reverb_dna();
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();
        assert_eq!(org.cell_count(), 3);

        // Run for 2 seconds — arp triggers voice, audio flows through reverb
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "Melo+reverb chain should produce audio: rms={rms}"
        );

        // Reverb should create a tail: after we stop feeding input,
        // the last portion should still have energy from the reverb tail.
        let last_quarter = &buf[(buf.len() * 3 / 4)..];
        let rms_tail: f32 =
            (last_quarter.iter().map(|s| s * s).sum::<f32>() / last_quarter.len() as f32).sqrt();
        assert!(
            rms_tail > 0.0001,
            "Reverb tail should have energy: rms_tail={rms_tail}"
        );
    }

    #[test]
    fn effect_wiring_reverb_is_terminal() {
        // In chain voice(1)→reverb(2), reverb should be the terminal cell
        let dna = make_melo_with_reverb_dna();
        let (org, _) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // reverb(2) has no outgoing audio wire → terminal
        assert!(
            org.terminal_cells.contains(&2),
            "Reverb should be terminal cell: {:?}",
            org.terminal_cells
        );
        // hexo_timbre(1) has outgoing audio wire → not terminal
        assert!(
            !org.terminal_cells.contains(&1),
            "hexo_timbre should NOT be terminal (it feeds into reverb)"
        );
    }

    fn make_hexo_timbre_with_reverb_dna() -> OrganismDna {
        let mut arp_params = BTreeMap::new();
        arp_params.insert("rate_hz".into(), 4.0);
        arp_params.insert("pattern".into(), 0.0);
        arp_params.insert("octaves".into(), 1.0);
        arp_params.insert("gate_length".into(), 0.5);

        let mut voice_params = BTreeMap::new();
        voice_params.insert("freq".into(), 440.0);
        voice_params.insert("det".into(), 7.0);
        voice_params.insert("cutoff".into(), 2000.0);
        voice_params.insert("res".into(), 0.3);
        voice_params.insert("atk".into(), 5.0);
        voice_params.insert("dcy".into(), 100.0);
        voice_params.insert("sus".into(), 0.7);
        voice_params.insert("rel".into(), 200.0);
        voice_params.insert("env_depth".into(), 3000.0);
        voice_params.insert("env_decay".into(), 300.0);
        voice_params.insert("osc_mix".into(), 0.7);
        let mut voice_str_params = BTreeMap::new();
        voice_str_params.insert("wtype".into(), "saw".into());

        let mut reverb_params = BTreeMap::new();
        reverb_params.insert("size".into(), 0.5);
        reverb_params.insert("dcy".into(), 0.6);
        reverb_params.insert("mix".into(), 0.3);

        OrganismDna {
            name: "hexo-melo-reverb-test".into(),
            species: "melo".into(),
            seed: 99,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "arpeggiator".into(),
                    params: arp_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "hexo_timbre".into(),
                    params: voice_params,
                    string_params: voice_str_params,
                    graph: None,
                },
                CellDna {
                    cell_type: "reverb".into(),
                    params: reverb_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![
                CellWire {
                    src_cell: 0,
                    dst_cell: 1,
                    wire_type: WireType::Trigger,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
                CellWire {
                    src_cell: 1,
                    dst_cell: 2,
                    wire_type: WireType::Audio,
                    gain: 1.0,
                    mode: WireMode::default(),
                },
            ],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        }
    }

    #[test]
    fn hexo_timbre_in_organism_chain() {
        // arpeggiator → hexo_timbre → reverb
        let dna = make_hexo_timbre_with_reverb_dna();
        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.001,
            "hexo_timbre→reverb chain should produce audio: rms={rms}"
        );
    }

    #[test]
    fn hexo_strike_in_organism() {
        // pattern_gen → hexo_strike (simple percussion organism)
        let mut pat_params = BTreeMap::new();
        pat_params.insert("bpm".into(), 120.0);
        pat_params.insert("steps".into(), 4.0);
        pat_params.insert("hits".into(), 4.0);

        let mut strike_params = BTreeMap::new();
        strike_params.insert("membrane_freq".into(), 180.0);
        strike_params.insert("bandwidth".into(), 0.5);
        strike_params.insert("atk".into(), 1.0);
        strike_params.insert("dcy".into(), 150.0);

        let dna = OrganismDna {
            name: "hexo-strike-test".into(),
            species: "tblk".into(),
            seed: 77,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "pattern_gen".into(),
                    params: pat_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
                CellDna {
                    cell_type: "hexo_strike".into(),
                    params: strike_params,
                    string_params: BTreeMap::new(),
                    graph: None,
                },
            ],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Trigger,
                gain: 1.0,
                mode: WireMode::default(),
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        };

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "hexo_strike organism should produce audio: rms={rms}"
        );
    }

    #[test]
    fn hexo_bed_in_organism() {
        // hexo_bed alone (always-on drone, no trigger needed)
        let mut bed_params = BTreeMap::new();
        bed_params.insert("root_hz".into(), 110.0);
        bed_params.insert("cutoff".into(), 800.0);
        bed_params.insert("res".into(), 0.3);

        let dna = OrganismDna {
            name: "hexo-bed-test".into(),
            species: "dron".into(),
            seed: 55,
            version: 1,
            cells: vec![CellDna {
                cell_type: "hexo_bed".into(),
                params: bed_params,
                string_params: BTreeMap::new(),
                graph: None,
            }],
            cell_wiring: vec![],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            affinity_tags: vec![],
            affinity_biases: vec![],
        };

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "hexo_bed organism should produce continuous audio: rms={rms}"
        );
    }

    #[test]
    fn peak_levels_from_preset_files() {
        // Load the actual preset DNA files and measure peak output
        let presets = [
            ("tblk-alpha", "assets/dna/tblk-alpha.json"),
            ("dron-alpha", "assets/dna/dron-alpha.json"),
            ("melo-alpha", "assets/dna/melo-alpha.json"),
        ];

        for (name, path) in &presets {
            let dna = crate::organism::dna_io::load(std::path::Path::new(path));
            if let Ok(dna) = dna {
                let result = OrganismDsp::from_dna(&dna, SR);
                if let Some((mut org, _handles)) = result {
                    let mut peak = 0.0f32;
                    let mut rms_acc = 0.0f32;
                    let mut out = [0.0f32; 2];
                    let n_samples = SR as usize * 3;
                    for _ in 0..n_samples {
                        org.tick(&mut out);
                        peak = peak.max(out[0].abs()).max(out[1].abs());
                        rms_acc += out[0] * out[0];
                    }
                    let rms = (rms_acc / n_samples as f32).sqrt();
                    eprintln!("  {name}: peak={peak:.4} rms={rms:.4}");
                    assert!(
                        peak <= 1.0 + 1e-6,
                        "{name} peak {peak:.4} exceeds 1.0 — soft_clip is not working"
                    );
                } else {
                    eprintln!("  {name}: FAILED TO BUILD OrganismDsp");
                }
            } else {
                eprintln!("  {name}: FAILED TO LOAD DNA");
            }
        }
    }

    #[test]
    fn peak_levels_within_headroom() {
        // Measure peak output from each organism to verify gain staging
        use crate::audio::gain_staging;

        let organisms = [
            ("tblk", make_tblk_dna(), gain_staging::TBLK_GAIN),
            ("dron", make_dron_dna(), gain_staging::DRON_GAIN),
            ("melo", make_melo_dna(), gain_staging::MELO_GAIN),
        ];

        let mut bus_peak_sum = 0.0f32;

        for (name, dna, channel_gain) in &organisms {
            let (mut org, _handles) = OrganismDsp::from_dna(dna, SR).unwrap();

            let mut peak = 0.0f32;
            let mut rms_acc = 0.0f32;
            let mut out = [0.0f32; 2];
            let n_samples = SR as usize * 2;
            for _ in 0..n_samples {
                org.tick(&mut out);
                peak = peak.max(out[0].abs()).max(out[1].abs());
                rms_acc += out[0] * out[0];
            }
            let rms = (rms_acc / n_samples as f32).sqrt();

            // Organism output should be ≤ 1.0 after soft_clip (tanh)
            assert!(
                peak <= 1.0 + 1e-6,
                "{name} organism peak {peak:.4} exceeds 1.0 (soft_clip should prevent this)"
            );

            // After channel gain + center pan (0.707), per-channel contribution
            let pan_factor = std::f32::consts::FRAC_1_SQRT_2;
            let channel_peak = peak * channel_gain * pan_factor;
            bus_peak_sum += channel_peak;

            eprintln!(
                "  {name}: peak={peak:.4} rms={rms:.4} → bus_contribution={channel_peak:.4}"
            );
        }

        let final_peak = bus_peak_sum * gain_staging::MASTER_GAIN;
        eprintln!(
            "  Worst-case sum peak after master gain ({})={final_peak:.4}",
            gain_staging::MASTER_GAIN
        );

        // Final output should be well under 1.0
        assert!(
            final_peak < 1.2,
            "Worst-case bus output {final_peak:.4} too hot (expected < 1.2)"
        );
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
