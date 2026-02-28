# S21 — Sequencer Cells + Envelopes + HOSO (Cochin Moon)

**Layer**: L4 + L5
**Depends on**: S20 (granular cells)
**Status**: Spec
**Aesthetic**: Haruomi Hosono — *Cochin Moon* (1978)

## Goal

Four new cells (seq, env, slew, accent_env) + pulse/PWM mode for osc_cell. Build **HOSO** — a rigid, clinical organism that faithfully follows the sequencer through its own nasal PWM filter character.

## Aesthetic Reference: Cochin Moon

Hosono's *Cochin Moon* is rigid, precise, synthetic. Sequenced patterns run like clockwork. The character comes from timbral choices — nasal pulse-width buzz, microtonal slides — not from rhythmic variation. HOSO follows the sequencer's pattern exactly (fidelity=0.9) but sounds unmistakably itself.

---

## New Cells

### seq_cell

**File**: `src/dsp/cell/seq_cell.rs`
**I/O**: none → mono (trigger/pitch output)
**Purpose**: The organism's **innate pattern** — its internal musical tendency.

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| bpm | 20–300 | 120.0 | Internal clock speed |
| steps | 1–16 | 16 | Active step count |
| gate_length | 0.01–1.0 | 0.5 | Gate duration as fraction of step |
| swing | 0–1 | 0.0 | Swing amount |

**String params** (per-step data, stored as comma-separated):
| Param | Format | Example |
|-------|--------|---------|
| pitches | "hz,hz,hz,..." | "110,130.8,146.8,110,..." |
| accents | "0/1,0/1,..." | "1,0,0,1,0,0,1,0,..." |
| gates | "0/1,0/1,..." | "1,1,0,1,1,1,0,1,..." |
| slides | "0/1,0/1,..." | "0,0,0,1,0,0,0,0,..." |

**Output behavior**: On each step, outputs trigger pulse (1.0 for gate duration, then 0.0). Pitch output available via `analysis()` for OrganismDsp to route.

### seq_cell vs SequencerModule

| | seq_cell (internal) | SequencerModule (external) |
|---|---|---|
| **Scope** | Inside organism's DSP | Infrastructure module |
| **Thread** | Audio thread (44.1kHz) | Control thread (60Hz) |
| **Purpose** | Organism's innate pattern | Human's prompt |
| **Editability** | DNA only | Step grid UI |
| **Blending** | Combined with external via `fidelity` | Source of truth for human intent |

When no external sequencer is connected (weak/no affinity edge), organism follows its internal seq_cell. When connected (strong edge), the blend is:
```
actual = lerp(internal_seq, external_seq, affinity_weight * fidelity)
```

### env_cell

**File**: `src/dsp/cell/env_cell.rs`
**I/O**: trigger → mono (ADSR envelope)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| attack | 0.001–2.0 | 0.01 | Attack time (seconds) |
| decay | 0.001–2.0 | 0.1 | Decay time (seconds) |
| sustain | 0–1 | 0.7 | Sustain level |
| release | 0.001–5.0 | 0.3 | Release time (seconds) |

**Trigger input**: Receives trigger from seq_cell or SequencerModule gate via trigger wire. On trigger: attack→decay→sustain. On gate off: release.

**Output**: 0.0–1.0 envelope value. Used as modulation source for gain (VCA) or filter (VCF).

### slew_cell

**File**: `src/dsp/cell/slew_cell.rs`
**I/O**: mono → mono (portamento/glide)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| rise | 0.001–2.0 | 0.05 | Rise time (seconds) — slew rate for increasing values |
| fall | 0.001–2.0 | 0.05 | Fall time (seconds) — slew rate for decreasing values |

**Purpose**: Smooths pitch or any control signal. Essential for HOSO's microtonal slides between notes. Also used by DRON for slow pitch drift (with rise/fall = 2.0s).

### accent_env_cell

**File**: `src/dsp/cell/accent_env_cell.rs`
**I/O**: trigger → mono (accent decay envelope)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| accent_amount | 0–1 | 0.7 | Peak level on accent trigger |
| decay | 0.01–1.0 | 0.15 | Decay time (seconds) |

**Purpose**: Short decay envelope that fires on accented steps. Adds punch to filter cutoff or amplitude. Simpler than full ADSR — just peak→decay→zero.

---

## osc_cell Upgrade: Pulse/PWM Mode

Add `pulse` to osc_cell's wtype options:

| wtype | FunDSP | Notes |
|-------|--------|-------|
| soft_saw | `soft_saw()` | Existing |
| saw | `saw()` | Existing |
| sine | `sine()` | Existing |
| square | `square()` | Existing |
| triangle | `triangle()` | Existing |
| **pulse** | `pulse()` | **New** — requires PWM input |

New param for osc_cell:
| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| pw | 0.1–0.9 | 0.5 | Pulse width (only used when wtype="pulse") |

FunDSP `pulse()` takes a pulse width input. In pulse mode:
```
var(&pw_shared) >> pulse() → audio output
```

Modulating `pw` via LFO creates classic PWM buzz — the core of HOSO's nasal character.

---

## HOSO Organism: "Malabar Ground Floor"

### Character

Rigid, clinical, microtonal slides, nasal PWM buzz. Follows the external sequencer's pattern exactly but sounds unmistakably itself through timbral processing.

### DNA: `hoso-malabar.json`

```json
{
  "species": "hoso",
  "fidelity": 0.9,
  "cells": [
    { "type": "seq_cell", "params": { "bpm": 130, "steps": 16, "gate_length": 0.4, "swing": 0.0 },
      "string_params": {
        "pitches": "130.8,146.8,164.8,130.8,146.8,164.8,196.0,130.8,146.8,164.8,130.8,196.0,146.8,164.8,130.8,220.0",
        "accents": "1,0,0,1,0,0,1,0,1,0,0,1,0,0,1,0",
        "gates":   "1,1,0,1,1,1,0,1,1,1,0,1,1,1,0,1",
        "slides":  "0,0,0,1,0,0,0,0,0,0,0,1,0,0,0,0"
      }
    },
    { "type": "osc_cell", "params": { "freq": 130.8, "det": 0, "gain": 0.6, "wtype": "pulse", "pw": 0.3 } },
    { "type": "env_cell", "params": { "attack": 0.005, "decay": 0.15, "sustain": 0.6, "release": 0.2 } },
    { "type": "slew_cell", "params": { "rise": 0.03, "fall": 0.03 } },
    { "type": "accent_env_cell", "params": { "accent_amount": 0.6, "decay": 0.12 } },
    { "type": "filter_cell", "params": { "cutoff": 800, "res": 0.6, "ftype": "moog" } },
    { "type": "lfo_cell", "params": { "rate": 2.5, "depth": 0.15, "shape": "tri" } },
    { "type": "mixer_cell", "params": { "gain": 0.7, "pan": 0.0 } }
  ],
  "wires": [
    { "type": "Trigger", "src": 0, "dst": 2 },
    { "type": "Trigger", "src": 0, "dst": 4 },
    { "type": "Audio", "src": 1, "dst": 5 },
    { "type": "Audio", "src": 5, "dst": 7 },
    { "type": "Modulation", "src": 2, "dst": 1, "target_param": "gain", "gain": 1.0 },
    { "type": "Modulation", "src": 3, "dst": 1, "target_param": "freq", "gain": 1.0 },
    { "type": "Modulation", "src": 4, "dst": 5, "target_param": "cutoff", "gain": 2000.0 },
    { "type": "Modulation", "src": 6, "dst": 1, "target_param": "pw", "gain": 0.2 }
  ],
  "sends": {
    "reverb": { "type": "reverb_stereo", "send": 0.2, "params": { "size": 0.4, "dcy": 0.4, "damp": 0.6 } }
  }
}
```

### Wire Graph

```
seq_cell[0] ──trigger──→ env_cell[2] ──mod(gain)──→ osc_cell[1] (VCA)
seq_cell[0] ──trigger──→ accent_env[4] ──mod(cutoff, +2kHz)──→ filter_cell[5]
seq_cell[0] ──pitch──→ slew_cell[3] ──mod(freq)──→ osc_cell[1] (pitch with glide)
osc_cell[1] ──audio──→ filter_cell[5] ──audio──→ mixer_cell[7]
lfo_cell[6] ──mod(pw, +/-0.2)──→ osc_cell[1] (PWM)
```

### Dialogue Personality

HOSO with fidelity=0.9 closely follows the SequencerModule when connected:
- **External pattern**: Almost directly used (90% weight)
- **Internal seq_cell**: Backup when no external sequencer (10% weight when connected)
- **Pitch slides**: slew_cell adds microtonal transitions between steps
- **Character**: All in the PWM buzz + moog filter + accent emphasis

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/dsp/cell/seq_cell.rs` | Create |
| `src/dsp/cell/env_cell.rs` | Create |
| `src/dsp/cell/slew_cell.rs` | Create |
| `src/dsp/cell/accent_env_cell.rs` | Create |
| `src/dsp/cell/osc_cell.rs` | Modify — add pulse/PWM mode + pw param |
| `src/dsp/cell/mod.rs` | Modify — register 4 new cells |
| `src/dsp/cell_registry.rs` | Modify — factory functions + param ranges |
| `assets/dna/hoso-malabar.json` | Create |

---

## Test Plan (~25 tests)

### seq_cell
- `seq_fires_triggers`: trigger output pulses at correct BPM intervals
- `seq_respects_gates`: rest steps produce no trigger
- `seq_accents_flag`: accent steps output higher velocity
- `seq_slide_flag`: slide steps accessible via analysis
- `seq_pitch_per_step`: each step outputs correct pitch
- `seq_swing_shifts_even`: even steps delayed by swing amount
- `seq_wraps_at_step_count`: cycles back to step 0 after last active step

### env_cell
- `env_attack_rises`: output ramps up during attack phase
- `env_sustain_holds`: output steady at sustain level during gate
- `env_release_falls`: output ramps down after gate off
- `env_retrigger`: new trigger during sustain restarts attack
- `env_zero_attack_instant`: attack=0.001 reaches peak in ~1ms

### slew_cell
- `slew_smooths_step`: step input → smooth output
- `slew_rise_fall_asymmetric`: different rise/fall rates produce different curves
- `slew_tracks_constant`: constant input → output converges

### accent_env_cell
- `accent_fires_on_trigger`: trigger → peak → decay → zero
- `accent_amount_scales_peak`: higher amount → higher peak
- `accent_decay_time`: longer decay → slower fall

### osc_cell pulse mode
- `osc_pulse_produces_audio`: pulse waveform generates non-zero output
- `osc_pw_changes_timbre`: different pw values produce different spectra
- `osc_pw_modulation`: LFO modulating pw creates PWM effect

### HOSO integration
- `hoso_loads_dna`: hoso-malabar.json loads without error
- `hoso_produces_audio`: organism generates non-zero audio
- `hoso_seq_triggers_env`: seq_cell triggers fire envelope
- `hoso_accent_boosts_filter`: accented steps open filter more

---

## Verification Criteria

- [ ] seq_cell fires triggers at correct BPM with per-step pitch/gate/accent/slide
- [ ] env_cell produces correct ADSR shape
- [ ] slew_cell smooths pitch transitions
- [ ] accent_env_cell produces short decay on accent triggers
- [ ] osc_cell pulse mode produces PWM-modulated output
- [ ] HOSO plays sequenced pattern with clinical precision + nasal PWM character
- [ ] When connected to SequencerModule, HOSO follows at fidelity=0.9
- [ ] When disconnected, HOSO falls back to internal seq_cell pattern
- [ ] `cargo test` — all tests pass
