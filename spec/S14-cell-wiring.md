# S14 — Cell-Level Audio Wiring

> The cells speak to each other. Sound flows through the body, not just out of it.

**Layer**: L4 (Cells) + L3 (Molecules)
**Depends on**: S12 (cell-dna), S13 (first organisms)
**Status**: Prospect

## Goal

Complete the cell-level wiring model so that DNA can express real signal flow graphs inside organisms. Currently, all cells mix directly to organism output — audio wires are declared in DNA but ignored at runtime. This spec makes audio and modulation wires functional, turning organisms from "bags of cells" into structured synthesis architectures.

## Ancestry (MAKE A BABY)

In the original Max/MSP patch, signal flow was explicit: `cycle~` → `svf~` → `delay~` → `tanh~`. The routing was the instrument. Solido's cell wiring brings this back: DNA encodes the patch, not just the parameters.

## The Problem

`OrganismDsp::tick()` (organism_dsp.rs:155-184) has three wire types defined but only one works:

| Wire Type | DNA Support | Runtime Support | Status |
|-----------|-------------|-----------------|--------|
| Trigger | `WireType::Trigger` | Fires NoteOn when src > 0.5 | Working |
| Audio | `WireType::Audio` | Comment: "mixed at organism level" | Dead code |
| Modulation | `WireType::Modulation { target_param }` | Comment: "via shared handles" | Dead code |

All cells with `output_channels() > 0` are summed with equal-power scaling (`1/sqrt(n)`), regardless of wiring topology. The wire graph in DNA is decorative.

## Architecture Decisions

### AD-1: Audio wires route cell output to cell input

Audio wires feed source cell output into destination cell input before ticking the destination. This requires **topological sort** of the cell graph at construction time (not per-sample). Cycles are broken by inserting a 1-sample delay buffer.

### AD-2: Cells gain an audio input interface

`DspCell::tick()` signature changes from `tick(&mut self, output: &mut [f32])` to `tick(&mut self, input: &[f32], output: &mut [f32])`. Cells that don't use input (oscillators, pattern generators) ignore it. Cells that process audio (filters, effects) read from input. Default input is silence.

### AD-3: Modulation wires use the existing SharedHandle system

Modulation wires don't inject audio — they write a control value from the source cell's output into a named SharedHandle on the destination cell. `ModMatrix` already works this way internally. Modulation wires generalize it: any cell's output can modulate any other cell's parameter.

### AD-4: Wire parameters extend CellWire

```rust
pub struct CellWire {
    pub src_cell: usize,
    pub dst_cell: usize,
    pub wire_type: WireType,
    pub gain: f32,          // default 1.0, scales signal on wire
    pub mode: WireMode,     // Add (default) or Multiply
}

pub enum WireMode {
    Add,       // dst_input += src_output * gain
    Multiply,  // dst_input *= src_output * gain
}
```

### AD-5: Only audio-sink cells contribute to organism output

Cells that have no outgoing audio wires are "terminal" — their output goes to the organism mix. Cells that feed into other cells via audio wires are "internal" — their output is consumed by the wire, not mixed to output. This prevents double-counting.

## Implementation

### 1. Topological sort at construction

In `OrganismDsp::from_dna()`, build a dependency graph from audio wires and compute a topological order. Store as `tick_order: Vec<usize>`. If a cycle exists, insert a 1-sample delay cell at the back-edge.

### 2. Add input to DspCell::tick

```rust
pub trait DspCell: Send {
    fn tick(&mut self, input: &[f32], output: &mut [f32]);
    // ... rest unchanged
}
```

All existing cells updated: oscillators/generators ignore input, filters/effects process it.

### 3. Audio wire dispatch in OrganismDsp::tick

```rust
// Tick cells in topological order
for &cell_idx in &self.tick_order {
    // Accumulate audio inputs from incoming wires
    let mut cell_input = [0.0f32; 2];
    for (src, dst, tag) in &self.wiring {
        if *dst == cell_idx {
            if let WireTag::Audio { gain, mode } = tag {
                match mode {
                    WireMode::Add => {
                        cell_input[0] += self.scratch[*src][0] * gain;
                        cell_input[1] += self.scratch[*src][1] * gain;
                    }
                    WireMode::Multiply => { /* ... */ }
                }
            }
        }
    }
    self.cells[cell_idx].tick(&cell_input, &mut self.scratch[cell_idx]);
}
```

### 4. Modulation wire dispatch

After all cells tick, modulation wires write source output to destination SharedHandles:

```rust
for (src, dst, tag) in &self.wiring {
    if let WireTag::Modulation { target_param, gain } = tag {
        let mod_value = self.scratch[*src][0] * gain;
        if let Some(handle) = self.shared_handles.get(target_param) {
            handle.set(mod_value);
        }
    }
}
```

### 5. Terminal cell detection

At construction, identify cells with no outgoing audio wires — only these mix to organism output.

## Files Modified

| File | Changes |
|------|---------|
| `src/dsp/cell/mod.rs` | `DspCell::tick` signature gains `input` param |
| `src/dsp/cell/*.rs` | All 7 cell types updated for new signature |
| `src/dsp/organism_dsp.rs` | Topological sort, audio/mod wire dispatch, terminal detection |
| `src/organism/dna.rs` | `CellWire` gains `gain`, `mode` fields |
| `src/organism/dna_io.rs` | Serialize new wire fields |

## Verification

- [ ] TBLK: pattern_gen triggers strike_voice (existing behavior preserved)
- [ ] Audio wire: harmonic_bed audio → shimmer_layer input (serial processing)
- [ ] Modulation wire: mod_matrix output → timbre_voice filter cutoff
- [ ] Terminal detection: only end-of-chain cells appear in organism mix
- [ ] Cycle detection: feedback loop inserts 1-sample delay, no hang
- [ ] Wire gain: setting gain=0.5 halves signal amplitude on wire
- [ ] All existing tests pass with new tick signature
