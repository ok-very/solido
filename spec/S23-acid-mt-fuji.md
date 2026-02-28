# S23 — Diode Filter + Tape Echo + ACID (Acid Mt. Fuji)

**Layer**: L4 + L5
**Depends on**: S21 (reuses seq_cell, env_cell, slew_cell, accent_env_cell)
**Status**: Spec
**Aesthetic**: Various artists — *Acid Mt. Fuji* compilation, TB-303 acid bass

## Goal

Two new cells (diode_filter, tape_delay) + slide behavior for seq_cell. Build **ACID** — a squelchy 303 bass organism that follows the sequencer closely but adds its own accent/slide interpretation.

## Aesthetic Reference: Acid Mt. Fuji

The TB-303's character comes from three elements: (1) a resonant diode ladder filter that squelches and screams, (2) accent that simultaneously boosts filter and amplitude, (3) slides that glide between notes with the filter opening. ACID faithfully follows the pattern (fidelity=0.8) but "fights back" — adding its own accent emphasis and octave jumps based on arousal.

---

## New Cells

### diode_filter_cell

**File**: `src/dsp/cell/diode_filter_cell.rs`
**I/O**: stereo → stereo (3-pole diode ladder filter)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| cutoff | 20–20000 | 400.0 | Filter cutoff frequency (Hz) |
| res | 0–2.0 | 0.8 | Resonance (>1.0 for squelch/self-oscillation) |
| drive | 0–1 | 0.3 | Pre-filter saturation |
| env_mod | 0–1 | 0.7 | Envelope modulation depth on cutoff |

**Implementation**: The 303's distinctive sound comes from its 3-pole (18dB/oct) diode ladder filter, not a standard 4-pole Moog ladder.

FunDSP approximation:
```
input * (1.0 + drive * 3.0)  // pre-drive saturation
  → lowpole(cutoff)          // 1st pole (6dB/oct)
  → lowpole(cutoff)          // 2nd pole (12dB/oct)
  → lowpole(cutoff)          // 3rd pole (18dB/oct)
  → tanh() soft clip          // post-saturation
```

Three cascaded `lowpole()` filters approximate the 18dB/oct slope. Resonance is implemented as negative feedback from output to input, controlled by `res`. When res > 1.0, the filter begins to self-oscillate — the characteristic 303 squelch.

**env_mod** controls how much the accent_env_cell opens the cutoff:
```
effective_cutoff = cutoff + env_signal * env_mod * 8000.0
```

### tape_delay_cell

**File**: `src/dsp/cell/tape_delay_cell.rs`
**I/O**: stereo → stereo (tape echo with HF loss)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| time | 0.01–2.0 | 0.375 | Delay time (seconds) — 3/16 at 120 BPM |
| feedback | 0–0.95 | 0.4 | Feedback amount (capped to prevent runaway) |
| hf_damp | 0–1 | 0.5 | High-frequency damping in feedback path |
| mix | 0–1 | 0.3 | Dry/wet mix |

**Implementation**: Circular buffer delay with a lowpass filter in the feedback path to simulate tape degradation.

```
delay_line[write_pos] = input + delay_line[read_pos] * feedback
output_wet = lowpole(delay_line[read_pos], hf_damp_freq)
output = input * (1 - mix) + output_wet * mix
```

`hf_damp` maps to lowpass cutoff: `hf_damp_freq = lerp(20000, 2000, hf_damp)`. At hf_damp=1.0, each echo loses everything above 2kHz — warm, murky tape character.

---

## seq_cell Slide Behavior

seq_cell (from S21) already has per-step `slides` string_param. In S23, slide behavior is implemented:

When `slide=1` on a step:
1. Gate does **not** retrigger — previous note's envelope stays in sustain
2. Pitch glides to next step's frequency via slew_cell
3. Filter cutoff gets a slide boost (+30% of env_mod depth)

This is the classic 303 slide: notes blur into each other with the filter screaming.

---

## ACID Organism: "Kinoko Shrine Acid"

### Character

Squelchy 303 bass. Accent bounces open the filter. Slides blur note transitions. Tape echo adds space. ACID follows the sequencer pattern closely (fidelity=0.8) but interprets accent and slide with its own personality — "the 303 that fights back."

### DNA: `acid-kinoko.json`

```json
{
  "species": "acid",
  "fidelity": 0.8,
  "cells": [
    { "type": "seq_cell", "params": { "bpm": 138, "steps": 16, "gate_length": 0.6, "swing": 0.1 },
      "string_params": {
        "pitches": "55,55,82.4,55,110,55,82.4,73.4,55,55,82.4,55,110,146.8,82.4,55",
        "accents": "1,0,0,1,0,1,0,0,1,0,0,1,1,0,0,1",
        "gates":   "1,1,1,1,1,1,1,0,1,1,1,1,1,1,1,0",
        "slides":  "0,0,1,0,0,0,1,0,0,0,1,0,0,1,0,0"
      }
    },
    { "type": "osc_cell", "params": { "freq": 55, "det": 0, "gain": 0.7, "wtype": "saw" } },
    { "type": "env_cell", "params": { "attack": 0.003, "decay": 0.2, "sustain": 0.0, "release": 0.05 } },
    { "type": "accent_env_cell", "params": { "accent_amount": 0.8, "decay": 0.2 } },
    { "type": "slew_cell", "params": { "rise": 0.06, "fall": 0.06 } },
    { "type": "diode_filter_cell", "params": { "cutoff": 400, "res": 1.2, "drive": 0.3, "env_mod": 0.7 } },
    { "type": "tape_delay_cell", "params": { "time": 0.375, "feedback": 0.4, "hf_damp": 0.5, "mix": 0.25 } },
    { "type": "mixer_cell", "params": { "gain": 0.7, "pan": 0.0 } }
  ],
  "wires": [
    { "type": "Trigger", "src": 0, "dst": 2 },
    { "type": "Trigger", "src": 0, "dst": 3 },
    { "type": "Audio", "src": 1, "dst": 5 },
    { "type": "Audio", "src": 5, "dst": 6 },
    { "type": "Audio", "src": 6, "dst": 7 },
    { "type": "Modulation", "src": 2, "dst": 1, "target_param": "gain", "gain": 1.0 },
    { "type": "Modulation", "src": 3, "dst": 5, "target_param": "cutoff", "gain": 6000.0 },
    { "type": "Modulation", "src": 4, "dst": 1, "target_param": "freq", "gain": 1.0 }
  ],
  "sends": {
    "reverb": { "type": "reverb_stereo", "send": 0.15, "params": { "size": 0.3, "dcy": 0.3, "damp": 0.7 } }
  }
}
```

### Wire Graph

```
seq_cell[0] ──trigger──→ env_cell[2] ──mod(gain)──→ osc_cell[1] (VCA)
seq_cell[0] ──trigger──→ accent_env[3] ──mod(cutoff, +6kHz)──→ diode_filter[5]
seq_cell[0] ──pitch──→ slew_cell[4] ──mod(freq)──→ osc_cell[1] (pitch with slide)
osc_cell[1] ──audio──→ diode_filter[5] ──audio──→ tape_delay[6] ──audio──→ mixer_cell[7]
```

### 303 Character Breakdown

| Element | Cell | Behavior |
|---------|------|----------|
| Saw oscillator | osc_cell (saw) | Single saw wave, 303's core tone |
| Diode ladder filter | diode_filter_cell | 18dB/oct, res=1.2 for squelch |
| Accent | accent_env_cell → diode_filter | Opens cutoff +6kHz on accented steps |
| Envelope | env_cell | Short decay, zero sustain — percussive "blip" |
| Slide | slew_cell + seq_cell slide flag | Pitch glide + sustained gate on slide steps |
| Tape wash | tape_delay_cell | 3/16 echo with HF loss |

### Dialogue Personality

ACID with fidelity=0.8 follows the SequencerModule closely:
- **Pattern**: 80% external sequencer, 20% internal seq_cell
- **Accent interpretation**: ACID may add extra accent emphasis based on arousal
- **Octave jumps**: At high arousal (>0.7), ACID transposes notes up an octave
- **Slide extension**: ACID may extend slide duration when valence is high
- **"Fights back"**: Unlike HOSO which passively follows, ACID adds its own musical emphasis

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/dsp/cell/diode_filter_cell.rs` | Create |
| `src/dsp/cell/tape_delay_cell.rs` | Create |
| `src/dsp/cell/seq_cell.rs` | Modify — implement slide behavior (gate hold + slew boost) |
| `src/dsp/cell/mod.rs` | Modify — register 2 new cells |
| `src/dsp/cell_registry.rs` | Modify — factory functions + param ranges |
| `assets/dna/acid-kinoko.json` | Create |

---

## Test Plan (~15 tests)

### diode_filter_cell
- `diode_passes_audio`: input → filtered output
- `diode_cutoff_shapes_spectrum`: low cutoff reduces highs
- `diode_res_squelch`: res > 1.0 creates resonant peak / self-oscillation
- `diode_drive_saturates`: higher drive → more harmonic distortion
- `diode_env_mod_opens_filter`: env signal increases effective cutoff

### tape_delay_cell
- `tape_delays_signal`: output has delayed copy of input
- `tape_feedback_repeats`: feedback > 0 creates echoes
- `tape_hf_damp_darkens`: high hf_damp → each echo loses treble
- `tape_mix_blends`: mix=0 → dry only, mix=1 → wet only
- `tape_no_runaway`: feedback capped at 0.95 prevents infinite buildup

### seq_cell slide
- `seq_slide_holds_gate`: slide step does not retrigger envelope
- `seq_slide_glides_pitch`: pitch transitions smoothly on slide steps
- `seq_slide_boosts_filter`: slide steps get extra filter cutoff boost

### ACID integration
- `acid_loads_dna`: acid-kinoko.json loads without error
- `acid_produces_audio`: organism generates non-zero audio with squelchy character

---

## Verification Criteria

- [ ] diode_filter_cell produces 18dB/oct slope with self-oscillation at high resonance
- [ ] tape_delay_cell creates warm echoes with progressive HF loss
- [ ] seq_cell slide behavior: gate hold + pitch glide + filter boost
- [ ] ACID plays squelchy 303 bass line with accent bounces
- [ ] When connected to SequencerModule, ACID follows at fidelity=0.8 with personality
- [ ] Tape delay at 3/16 note creates rhythmic echo wash
- [ ] `cargo test` — all tests pass
