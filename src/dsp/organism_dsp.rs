use std::collections::HashMap;

use fundsp::prelude32::{shared, Shared};

use crate::dsp::cell::arpeggiator::Arpeggiator;
use crate::dsp::cell::mod_matrix::ModMatrix;
use crate::dsp::cell::pattern_gen::PatternGen;
use crate::dsp::cell::{CellRegistry, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::{OrganismDna, WireType};

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
    sample_rate: f32,
}

/// Simplified wire tag for runtime dispatch.
#[derive(Debug, Clone)]
enum WireTag {
    Audio,
    Trigger,
    Modulation { target_param: String },
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
                    WireType::Audio => WireTag::Audio,
                    WireType::Trigger => WireTag::Trigger,
                    WireType::Modulation { target_param } => WireTag::Modulation {
                        target_param: target_param.clone(),
                    },
                };
                (w.src_cell, w.dst_cell, tag)
            })
            .collect();

        let scratch = vec![[0.0f32; 2]; cells.len()];

        Some((
            OrganismDsp {
                cells,
                wiring,
                scratch,
                bypassed,
                output: [0.0; 2],
                sample_rate: sr,
            },
            all_handles,
        ))
    }

    /// Process one sample. Cells tick in order, wiring is applied.
    pub fn tick(&mut self, output: &mut [f32]) {
        // Clear scratch
        for s in &mut self.scratch {
            *s = [0.0; 2];
        }

        // Tick cells in order and store outputs (skip bypassed cells)
        for i in 0..self.cells.len() {
            if self.bypassed[i].value() > 0.5 {
                self.scratch[i] = [0.0; 2];
                continue;
            }
            let ch = self.cells[i].output_channels();
            let mut cell_out = [0.0f32; 2];
            self.cells[i].tick(&mut cell_out[..ch]);
            self.scratch[i] = cell_out;
        }

        // Process wiring: dispatch triggers and modulation from src to dst
        // We need to collect trigger events first, then dispatch them
        // Since we can't borrow self.cells mutably while iterating wiring,
        // collect events into a temporary buffer.
        let mut trigger_commands: Vec<(usize, DspCommand)> = Vec::new();

        for (src, dst, tag) in &self.wiring {
            // Skip wires involving bypassed cells
            if self.bypassed[*src].value() > 0.5 || self.bypassed[*dst].value() > 0.5 {
                continue;
            }
            match tag {
                WireTag::Trigger => {
                    // If src cell output > 0.5, send a trigger to dst
                    if self.scratch[*src][0] > 0.5 {
                        // Use the trigger value as velocity
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
                WireTag::Audio => {
                    // Audio wiring is mixed at the organism level output stage
                }
                WireTag::Modulation { .. } => {
                    // Modulation is applied via shared handles by ModMatrix
                }
            }
        }

        // Dispatch trigger commands
        for (dst_idx, cmd) in trigger_commands {
            if dst_idx < self.cells.len() {
                self.cells[dst_idx].handle_command(&cmd);
            }
        }

        // Mix all audio-producing cells to stereo output with equal-power scaling
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        let audio_cells = self.cells.iter()
            .filter(|c| c.output_channels() > 0).count().max(1);
        let scale = 1.0 / (audio_cells as f32).sqrt();

        for i in 0..self.cells.len() {
            let ch = self.cells[i].output_channels();
            match ch {
                1 => {
                    // Mono: center-pan at -3dB per side
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
        strike_params.insert("bandwidth".into(), 60.0);
        strike_params.insert("click_mix".into(), 0.3);
        strike_params.insert("body_feedback".into(), 0.4);

        OrganismDna {
            name: "tblk-test".into(),
            species: "tblk".into(),
            seed: 42,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "pattern_gen".into(),
                    params: pat_params,
                },
                CellDna {
                    cell_type: "strike_voice".into(),
                    params: strike_params,
                },
            ],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Trigger,
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
        bed_params.insert("detune_cents".into(), 5.0);
        bed_params.insert("cutoff".into(), 800.0);
        bed_params.insert("resonance".into(), 0.707);
        bed_params.insert("pan_rate".into(), 0.05);

        let mut shimmer_params = BTreeMap::new();
        shimmer_params.insert("shimmer_amount".into(), 0.3);
        shimmer_params.insert("diffusion".into(), 0.5);
        shimmer_params.insert("feedback".into(), 0.3);

        OrganismDna {
            name: "dron-test".into(),
            species: "dron".into(),
            seed: 99,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "harmonic_bed".into(),
                    params: bed_params,
                },
                CellDna {
                    cell_type: "shimmer_layer".into(),
                    params: shimmer_params,
                },
            ],
            cell_wiring: vec![],
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
        voice_params.insert("pulse_width".into(), 0.5);
        voice_params.insert("filter_base".into(), 200.0);
        voice_params.insert("filter_depth".into(), 5000.0);
        voice_params.insert("filter_q".into(), 0.707);
        voice_params.insert("attack_ms".into(), 5.0);
        voice_params.insert("decay_ms".into(), 100.0);
        voice_params.insert("sustain".into(), 0.7);
        voice_params.insert("release_ms".into(), 200.0);

        let mut mod_params = BTreeMap::new();
        mod_params.insert("pwm_rate".into(), 2.0);
        mod_params.insert("pwm_depth".into(), 0.2);
        mod_params.insert("filter_lfo_rate".into(), 0.5);
        mod_params.insert("vibrato_rate".into(), 5.0);
        mod_params.insert("vibrato_depth".into(), 10.0);

        OrganismDna {
            name: "melo-test".into(),
            species: "melo".into(),
            seed: 77,
            version: 1,
            cells: vec![
                CellDna {
                    cell_type: "arpeggiator".into(),
                    params: arp_params,
                },
                CellDna {
                    cell_type: "timbre_voice".into(),
                    params: voice_params,
                },
                CellDna {
                    cell_type: "mod_matrix".into(),
                    params: mod_params,
                },
            ],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 1,
                wire_type: WireType::Trigger,
            }],
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
}
