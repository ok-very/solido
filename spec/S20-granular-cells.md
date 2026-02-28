# S20 — Granular Cell Kit + DRON Rebuild

**Layer**: L4 (Cells)
**Depends on**: S19 (dialogue architecture)
**Status**: Spec

## Goal

Establish the **composable cell pattern** — one function per cell, wired by DNA. Four new cells (osc, filter, lfo, mixer) replace the monolithic `drone_bed`. DRON is rebuilt from granular parts. Critical fix: modulation wires become additive.

## Why Granular Cells

`drone_bed` is a monolith: dual oscs + filter + LFO in one cell. This prevents:
- Reusing the filter with different oscillator configurations
- Adding modulation targets dynamically
- Building new organisms from existing parts

Granular cells solve this: `osc_cell → filter_cell`, with `lfo_cell` modulating filter cutoff via additive modulation wires.

---

## New Cells

### osc_cell

**File**: `src/dsp/cell/osc_cell.rs`
**I/O**: none → stereo (dual detuned oscillator)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| freq | 20–2000 | 110.0 | Fundamental frequency (Hz) |
| det | 0–50 | 7.0 | Detune between osc1/osc2 (cents) |
| gain | 0–1 | 0.5 | Output level |
| wtype | string | "soft_saw" | Waveform: soft_saw, saw, sine, square, triangle |

**FunDSP graph**:
```
osc1 = var(&freq_shared) >> waveform()
osc2 = var(&freq_det_shared) >> waveform()
output = (osc1 + osc2) * 0.5 * gain
```

Stereo output: osc1 slightly left, osc2 slightly right via tick() manual pan.

### filter_cell

**File**: `src/dsp/cell/filter_cell.rs`
**I/O**: stereo → stereo (audio filter)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| cutoff | 20–20000 | 1200.0 | Filter cutoff frequency (Hz) |
| res | 0–1 | 0.25 | Resonance / Q |
| ftype | string | "moog" | Filter type: moog, lowpass, highpass, bandpass |

**FunDSP graph** (moog mode):
```
input → moog(cutoff, Q) → output
```

Filter receives audio from upstream cell via audio wire. `tick(input, output)` passes input through the FunDSP filter graph.

### lfo_cell

**File**: `src/dsp/cell/lfo_cell.rs`
**I/O**: none → mono (control signal)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| rate | 0.01–10 | 0.07 | LFO frequency (Hz) |
| depth | 0–1 | 0.3 | Modulation depth |
| shape | string | "sine" | Shape: sine, tri, square, saw |

**Output**: bipolar signal (-depth..+depth) at `rate` Hz. Used as modulation source via modulation wires.

### mixer_cell

**File**: `src/dsp/cell/mixer_cell.rs`
**I/O**: stereo → stereo (terminal mix point)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| gain | 0–1 | 0.8 | Output level |
| pan | -1–1 | 0.0 | Stereo pan (-1=left, +1=right) |

Terminal cell — `OrganismDsp` sums terminal cells to stereo output. Mixer applies gain and pan.

---

## Critical Fix: Additive Modulation

### Current Bug

In `organism_dsp.rs` (around line 237), modulation wires **replace** the base value:

```rust
// WRONG: overwrites base cutoff with LFO output
dst_cell.set_param("cutoff", mod_value);
```

### Fix

Modulation must be **additive** — base value + mod signal * depth:

```rust
// RIGHT: add modulation to base value
let base = dst_cell.get_param_base("cutoff");
let modulated = base + mod_value * mod_gain;
dst_cell.set_param("cutoff", modulated.clamp(param_min, param_max));
```

This requires:
1. Each cell stores a **base value** per param (from Shared handle / DNA default)
2. Modulation wires add to the base, not replace it
3. The sum is clamped to the param's valid range

### Implementation

Add to `DspCell` trait:
```rust
fn get_param_base(&self, name: &str) -> f32;
```

`OrganismDsp::tick()` modulation wire processing:
```rust
for wire in &self.mod_wires {
    let mod_signal = scratch[wire.src_cell][0]; // LFO output
    let base = self.cells[wire.dst_cell].get_param_base(&wire.target_param);
    let modulated = base + mod_signal * wire.gain;
    let (min, max) = self.param_range(&wire.target_param);
    self.cells[wire.dst_cell].set_param(&wire.target_param, modulated.clamp(min, max));
}
```

---

## DRON Rebuild: Composable DNA

### Current DNA (`dron-alpha.json`)

Single `drone_bed` cell with 7 params.

### New DNA (`dron-composable.json`)

```json
{
  "species": "dron",
  "fidelity": 0.3,
  "cells": [
    { "type": "osc_cell", "params": { "freq": 110, "det": 7, "gain": 0.5, "wtype": "soft_saw" } },
    { "type": "filter_cell", "params": { "cutoff": 1200, "res": 0.25, "ftype": "moog" } },
    { "type": "lfo_cell", "params": { "rate": 0.07, "depth": 0.3, "shape": "sine" } },
    { "type": "mixer_cell", "params": { "gain": 0.8, "pan": 0.0 } }
  ],
  "wires": [
    { "type": "Audio", "src": 0, "dst": 1 },
    { "type": "Audio", "src": 1, "dst": 3 },
    { "type": "Modulation", "src": 2, "dst": 1, "target_param": "cutoff", "gain": 400.0 }
  ],
  "sends": {
    "reverb": { "type": "reverb_stereo", "send": 0.4, "params": { "size": 0.7, "dcy": 0.7, "damp": 0.4 } }
  }
}
```

Wire graph:
```
osc_cell[0] ──audio──→ filter_cell[1] ──audio──→ mixer_cell[3]
lfo_cell[2] ──mod(cutoff, gain=400)──→ filter_cell[1]
```

The LFO modulates filter cutoff by +/-400 Hz around the base value of 1200 Hz.

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/dsp/cell/osc_cell.rs` | Create |
| `src/dsp/cell/filter_cell.rs` | Create |
| `src/dsp/cell/lfo_cell.rs` | Create |
| `src/dsp/cell/mixer_cell.rs` | Create |
| `src/dsp/cell/mod.rs` | Modify — register 4 new cells in CellRegistry |
| `src/dsp/cell_registry.rs` | Modify — factory functions + param ranges |
| `src/dsp/organism_dsp.rs` | Modify — additive modulation fix, audio wire routing |
| `src/dsp/cell.rs` | Modify — add `get_param_base()` to DspCell trait |
| `assets/dna/dron-composable.json` | Create |

---

## Test Plan (~30 tests)

### osc_cell
- `osc_produces_signal`: non-zero stereo output
- `osc_freq_tracks_shared`: changing freq Shared changes pitch
- `osc_detune_spreads`: det > 0 creates beating
- `osc_wtype_changes_timbre`: each waveform produces distinct output
- `osc_gain_scales`: gain=0 → silence, gain=1 → louder

### filter_cell
- `filter_passes_audio`: input → output with signal present
- `filter_cutoff_shapes_spectrum`: low cutoff reduces high frequencies
- `filter_res_peaks`: high resonance creates peak at cutoff
- `filter_type_switch`: moog/lowpass/highpass/bandpass produce different responses
- `filter_silence_on_zero_input`: no input → no output (no self-oscillation at default res)

### lfo_cell
- `lfo_produces_bipolar`: output swings between -depth and +depth
- `lfo_rate_changes_speed`: faster rate → more zero crossings per second
- `lfo_shapes_differ`: sine/tri/square/saw have distinct waveforms
- `lfo_depth_zero_is_silent`: depth=0 → output is 0

### mixer_cell
- `mixer_passes_audio`: input → output
- `mixer_gain_scales`: gain=0 → silence
- `mixer_pan_left`: pan=-1 → right channel silent
- `mixer_pan_right`: pan=+1 → left channel silent
- `mixer_pan_center`: pan=0 → equal both channels

### Additive Modulation
- `mod_adds_to_base`: LFO output adds to base cutoff, not replaces
- `mod_clamps_to_range`: modulated value stays within param range
- `mod_gain_scales_depth`: higher wire gain → larger modulation swing
- `mod_zero_depth_no_change`: LFO depth=0 → cutoff stays at base

### Integration
- `dron_composable_loads`: new DNA preset loads without error
- `dron_composable_sounds`: organism produces non-zero audio
- `dron_composable_lfo_audible`: filter cutoff modulation is measurable
- `dron_composable_matches_old`: RMS level comparable to old drone_bed
- `wire_topological_order`: cells tick in correct dependency order
- `audio_wire_routes_signal`: osc output reaches filter input

---

## Verification Criteria

- [ ] All 4 new cells registered in CellRegistry with correct param ranges
- [ ] osc_cell produces audio with all 5 waveform types
- [ ] filter_cell processes input audio through selected filter type
- [ ] lfo_cell produces bipolar control signal
- [ ] mixer_cell applies gain + pan to audio
- [ ] Modulation wires are additive (base + mod*gain), not replacement
- [ ] `dron-composable.json` loads and plays audio
- [ ] DRON sounds comparable to old drone_bed (warm, evolving drone)
- [ ] `cargo test` — all tests pass
