# S22 — Function Generator + Saw Bank + SPGL (Expanding Universe)

**Layer**: L4 + L5
**Depends on**: S20 (granular cells)
**Parallel with**: S21 (independent cell sets)
**Status**: Spec
**Aesthetic**: Laurie Spiegel — *The Expanding Universe* (1980)

## Goal

Three new cells (func_gen, saw_bank, logic_seq) for long-form evolving synthesis. Build **SPGL** — a slow, nebulous organism that mostly ignores external prompts and follows its own multi-minute function generators.

## Aesthetic Reference: The Expanding Universe

Spiegel's work is dense, slowly evolving, algorithmic. Nothing repeats for minutes. Pitch changes are glacial — drifting through harmonic fields rather than stepping through notes. Rhythm emerges from overlapping function generators, not from sequenced patterns. SPGL barely acknowledges the sequencer (fidelity=0.1) — its func_gen_cells dominate.

---

## New Cells

### func_gen_cell

**File**: `src/dsp/cell/func_gen_cell.rs`
**I/O**: none → mono (long-period control signal)
**Purpose**: Multi-minute mathematical curves for glacial modulation.

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| period | 1.0–600.0 | 120.0 | Full cycle duration (seconds) |
| depth | 0–1 | 0.5 | Output amplitude |
| shape | string | "sine" | Shape: sine, tri, ramp, exp_decay, log_rise, cosine_sum |

**Shapes**:
| Shape | Formula | Character |
|-------|---------|-----------|
| sine | `sin(2π * t/period)` | Smooth oscillation |
| tri | Linear ramp up + down | Angular drift |
| ramp | `t/period` sawtooth | One-way sweep |
| exp_decay | `e^(-3t/period)` | Burst then fade |
| log_rise | `1 - e^(-3t/period)` | Gradual swell |
| cosine_sum | `0.5*sin(2πt/P) + 0.3*sin(2πt/(P*1.618)) + 0.2*sin(2πt/(P*2.414))` | Complex, golden-ratio multi-frequency |

The `cosine_sum` shape is key to SPGL's character — three cosines at golden ratio intervals create patterns that never exactly repeat.

**Output**: bipolar (-depth..+depth) control signal. One function generator with a 120-second period takes 2 full minutes to sweep. Multiple func_gen_cells at different periods create dense, slowly-shifting modulation fields.

### saw_bank_cell

**File**: `src/dsp/cell/saw_bank_cell.rs`
**I/O**: none → stereo (detuned saw bank)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| freq | 20–2000 | 55.0 | Base frequency (Hz) |
| voices | 1–8 | 6 | Number of detuned saw oscillators |
| spread | 0–100 | 15.0 | Detune spread (cents between outermost voices) |
| gain | 0–1 | 0.4 | Output level |

**FunDSP implementation**: N saw oscillators spread symmetrically in pitch around `freq`. Each voice offset by `spread * (i - (N-1)/2) / ((N-1)/2)` cents. Voices panned across stereo field.

Normalized gain: `output *= 1.0 / sqrt(voices)` to prevent level increase with more voices.

### logic_seq_cell

**File**: `src/dsp/cell/logic_seq_cell.rs`
**I/O**: none → mono (trigger output)
**Purpose**: Algorithmic trigger patterns — euclidean, prime, fibonacci, polyrhythm.

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| rate | 0.1–20 | 2.0 | Trigger rate (Hz) — base clock speed |
| density | 0–1 | 0.5 | Pattern density / fill amount |
| algorithm | string | "euclidean" | Algorithm: euclidean, prime, fibonacci, polyrhythm |

**Algorithms**:

| Algorithm | Behavior |
|-----------|----------|
| euclidean | Bjorklund distribution: `density * steps` hits spread maximally across `steps` |
| prime | Trigger on prime-numbered steps (2, 3, 5, 7, 11, 13...) within cycle |
| fibonacci | Trigger spacing follows fibonacci sequence (1, 1, 2, 3, 5, 8...) |
| polyrhythm | Two overlapping euclidean patterns at density and 1-density |

**Output**: 1.0 pulse on trigger, 0.0 otherwise. Used to drive env_cell or strike_voice_cell.

---

## SPGL Organism: "Kepler's Harmony"

### Character

Dense, slowly evolving. Nothing repeats for minutes. Barely acknowledges the sequencer — its func_gen_cells and saw_bank dominate. Multiple function generators at different periods modulate pitch, filter cutoff, and stereo spread, creating a slowly-breathing harmonic field.

### DNA: `spgl-kepler.json`

```json
{
  "species": "spgl",
  "fidelity": 0.1,
  "cells": [
    { "type": "saw_bank_cell", "params": { "freq": 55, "voices": 6, "spread": 15, "gain": 0.4 } },
    { "type": "saw_bank_cell", "params": { "freq": 82.4, "voices": 4, "spread": 8, "gain": 0.3 } },
    { "type": "filter_cell", "params": { "cutoff": 600, "res": 0.3, "ftype": "lowpass" } },
    { "type": "func_gen_cell", "params": { "period": 120, "depth": 0.5, "shape": "cosine_sum" } },
    { "type": "func_gen_cell", "params": { "period": 90, "depth": 0.3, "shape": "sine" } },
    { "type": "func_gen_cell", "params": { "period": 200, "depth": 0.4, "shape": "tri" } },
    { "type": "logic_seq_cell", "params": { "rate": 0.5, "density": 0.3, "algorithm": "fibonacci" } },
    { "type": "mixer_cell", "params": { "gain": 0.6, "pan": 0.0 } }
  ],
  "wires": [
    { "type": "Audio", "src": 0, "dst": 2 },
    { "type": "Audio", "src": 1, "dst": 2 },
    { "type": "Audio", "src": 2, "dst": 7 },
    { "type": "Modulation", "src": 3, "dst": 2, "target_param": "cutoff", "gain": 500.0 },
    { "type": "Modulation", "src": 4, "dst": 0, "target_param": "freq", "gain": 10.0 },
    { "type": "Modulation", "src": 5, "dst": 0, "target_param": "spread", "gain": 20.0 },
    { "type": "Modulation", "src": 5, "dst": 1, "target_param": "freq", "gain": 5.0 }
  ],
  "sends": {
    "reverb": { "type": "reverb_stereo", "send": 0.6, "params": { "size": 0.9, "dcy": 0.9, "damp": 0.3 } }
  }
}
```

### Wire Graph

```
saw_bank[0] (55Hz, 6v) ──audio──┐
saw_bank[1] (82Hz, 4v) ──audio──┼──→ filter_cell[2] ──audio──→ mixer_cell[7]
                                 │
func_gen[3] (120s, cosine_sum) ──mod(cutoff, +/-500Hz)──→ filter_cell[2]
func_gen[4] (90s, sine) ──mod(freq, +/-10Hz)──→ saw_bank[0]
func_gen[5] (200s, tri) ──mod(spread, +/-20c)──→ saw_bank[0]
func_gen[5] (200s, tri) ──mod(freq, +/-5Hz)──→ saw_bank[1]

logic_seq[6] (fibonacci, 0.5Hz) — available for cross-organism dialogue
```

### Modulation Periods

| func_gen | Period | Target | Effect |
|----------|--------|--------|--------|
| [3] | 120s | filter cutoff | Filter opens and closes over 2 minutes |
| [4] | 90s | saw_bank[0] freq | Base pitch drifts +/-10Hz over 1.5 minutes |
| [5] | 200s | saw_bank[0] spread + [1] freq | Spread and pitch of both banks drift over 3+ minutes |

These periods are incommensurate — the modulation landscape never exactly repeats.

### Dialogue Personality

SPGL with fidelity=0.1 barely acknowledges external input:
- **External pitch**: Averaged into a long-term pitch accumulator over minutes
- **Internal func_gens**: Dominate all modulation (90% weight)
- **Logic_seq**: Emits fibonacci triggers that other organisms can pick up
- **Cross-org effect**: SPGL's slow harmonic drift pulls other organisms toward its pitch center via learned affinity edges

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/dsp/cell/func_gen_cell.rs` | Create |
| `src/dsp/cell/saw_bank_cell.rs` | Create |
| `src/dsp/cell/logic_seq_cell.rs` | Create |
| `src/dsp/cell/mod.rs` | Modify — register 3 new cells |
| `src/dsp/cell_registry.rs` | Modify — factory functions + param ranges |
| `assets/dna/spgl-kepler.json` | Create |

---

## Test Plan (~20 tests)

### func_gen_cell
- `func_gen_sine_period`: output completes one cycle in `period` seconds
- `func_gen_depth_scales`: depth=0 → output is 0, depth=1 → full swing
- `func_gen_cosine_sum_complex`: cosine_sum output is different from plain sine
- `func_gen_ramp_monotonic`: ramp shape increases monotonically within period
- `func_gen_long_period`: 600s period accumulates correctly without drift

### saw_bank_cell
- `saw_bank_produces_audio`: non-zero stereo output
- `saw_bank_voices_scale`: more voices → denser sound (detectable spectral spread)
- `saw_bank_spread_widens`: larger spread → wider pitch range between voices
- `saw_bank_gain_normalized`: 8 voices not louder than 2 voices at same gain
- `saw_bank_freq_tracks`: changing freq shifts all voices

### logic_seq_cell
- `logic_euclidean_distribution`: euclidean pattern has correct number of hits
- `logic_prime_steps`: prime algorithm triggers on prime-numbered steps
- `logic_fibonacci_spacing`: fibonacci pattern has increasing inter-trigger gaps
- `logic_density_scales`: higher density → more triggers per cycle
- `logic_rate_changes_speed`: higher rate → faster clock

### SPGL integration
- `spgl_loads_dna`: spgl-kepler.json loads without error
- `spgl_produces_audio`: organism generates non-zero audio
- `spgl_evolves_slowly`: audio character measurably changes over 30+ seconds
- `spgl_ignores_external_pitch`: external pitch signal barely affects output
- `spgl_func_gen_modulates_filter`: filter cutoff changes over time

---

## Verification Criteria

- [ ] func_gen_cell produces multi-minute curves in all 6 shapes
- [ ] saw_bank_cell produces properly detuned N-voice saw stack
- [ ] logic_seq_cell generates correct algorithmic trigger patterns
- [ ] SPGL plays dense, slowly evolving drone from two saw banks + function generators
- [ ] SPGL barely responds to external sequencer/keyboard (fidelity=0.1)
- [ ] Modulation landscape never exactly repeats (incommensurate periods)
- [ ] Heavy reverb send (0.6) creates expansive spatial character
- [ ] `cargo test` — all tests pass
