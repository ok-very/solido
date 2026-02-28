# S24 — TBLK Tabla Percussion

**Layer**: L4 + L5
**Depends on**: S22 (reuses logic_seq_cell)
**Status**: Spec
**Aesthetic**: Indian classical tabla — resonant membrane synthesis

## Goal

Two new cells (strike_voice, noise_burst) for physical-modeling percussion. Rebuild **TBLK** — an organic tabla organism that follows rhythm prompts but quantizes pitch to resonant membrane modes, creating polyrhythmic counterpoint.

## Aesthetic Reference: Tabla Synthesis

Tabla sound comes from vibrating membranes with specific harmonic modes. Different strokes excite different mode combinations. The dayan (right drum) has harmonic overtones from the syahi paste; the bayan (left drum) has inharmonic, pitch-bending tones. TBLK models both through parametric physical modeling.

### Tabla Strokes (mapped to DNA presets)

| Stroke | Drum | Character | Model |
|--------|------|-----------|-------|
| Na/Ta | Dayan | Sharp, bright, pitched | High membrane_freq, short decay, low noise |
| Tin | Dayan | Ringing, sustained | High membrane_freq, long decay, very low noise |
| Dha | Both | Full, complex | Mid freq, medium decay, noise burst for bayan |
| Ge/Ghe | Bayan | Deep, pitch-bending | Low freq, long decay, high noise, pitch envelope |
| Te/Re | Dayan | Dry, muted | Mid freq, very short decay, no noise |

---

## New Cells

### strike_voice_cell

**File**: `src/dsp/cell/strike_voice_cell.rs`
**I/O**: trigger → stereo (resonant membrane percussion)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| membrane_freq | 40–800 | 200.0 | Fundamental membrane frequency (Hz) |
| tension | 0–1 | 0.5 | Membrane tension — affects harmonic spacing |
| decay | 0.01–3.0 | 0.4 | Decay time (seconds) |
| noise_mix | 0–1 | 0.2 | Noise component mix (bayan character) |
| tone | 0–1 | 0.5 | Bright/dark balance |

**Physical model**:
```
exciter = impulse + noise * noise_mix
membrane = resonator_hz(membrane_freq, bandwidth) +
           resonator_hz(membrane_freq * harmonic2, bandwidth2) +
           resonator_hz(membrane_freq * harmonic3, bandwidth3)
output = exciter → membrane → envelope(decay) → tone_filter
```

Where `harmonic2` and `harmonic3` depend on `tension`:
- Low tension (bayan): inharmonic ratios (1.0, 1.59, 2.14) — characteristic tabla beating
- High tension (dayan): nearly harmonic (1.0, 2.0, 3.0) — clear pitched tone

`tone` controls a tilt EQ: low = dark/warm, high = bright/sharp.

**FunDSP implementation**: Three parallel `resonator_hz()` filters excited by noise burst. Decay envelope via `envelope2()` or manual exponential decay in tick().

### noise_burst_cell

**File**: `src/dsp/cell/noise_burst_cell.rs`
**I/O**: trigger → mono (short noise transient)

| Param | Range | Default | Description |
|-------|-------|---------|-------------|
| color | 200–8000 | 2000.0 | Lowpass cutoff on noise (Hz) |
| duration | 0.001–0.1 | 0.01 | Burst duration (seconds) |
| level | 0–1 | 0.5 | Output level |

**Purpose**: Short filtered noise burst for attack transients. Adds the "chik" to tabla strokes, the slap to hand drums. Triggered by logic_seq_cell or external gate.

**Implementation**:
```
noise() → lowpole(color) → envelope(duration) → * level
```

---

## TBLK Organism: "Dha"

### Character

Organic tabla patterns with resonant membranes. Two strike voices (dayan + bayan) triggered by logic_seq patterns. TBLK follows rhythm prompts at fidelity=0.5 — it follows the groove but quantizes pitch to resonant membrane modes and creates polyrhythmic counterpoint.

### DNA: `tblk-dha.json`

```json
{
  "species": "tblk",
  "fidelity": 0.5,
  "cells": [
    { "type": "logic_seq_cell", "params": { "rate": 4.0, "density": 0.6, "algorithm": "euclidean" } },
    { "type": "logic_seq_cell", "params": { "rate": 3.0, "density": 0.4, "algorithm": "prime" } },
    { "type": "strike_voice_cell", "params": { "membrane_freq": 320, "tension": 0.7, "decay": 0.3, "noise_mix": 0.15, "tone": 0.6 } },
    { "type": "strike_voice_cell", "params": { "membrane_freq": 100, "tension": 0.2, "decay": 0.6, "noise_mix": 0.5, "tone": 0.3 } },
    { "type": "noise_burst_cell", "params": { "color": 3000, "duration": 0.008, "level": 0.4 } },
    { "type": "mixer_cell", "params": { "gain": 0.7, "pan": 0.0 } }
  ],
  "wires": [
    { "type": "Trigger", "src": 0, "dst": 2 },
    { "type": "Trigger", "src": 0, "dst": 4 },
    { "type": "Trigger", "src": 1, "dst": 3 },
    { "type": "Audio", "src": 2, "dst": 5 },
    { "type": "Audio", "src": 3, "dst": 5 },
    { "type": "Audio", "src": 4, "dst": 5 }
  ],
  "sends": {
    "reverb": { "type": "reverb_stereo", "send": 0.25, "params": { "size": 0.5, "dcy": 0.4, "damp": 0.5 } }
  }
}
```

### Wire Graph

```
logic_seq[0] (euclidean, 4Hz) ──trigger──→ strike_voice[2] (dayan, 320Hz)
logic_seq[0] (euclidean, 4Hz) ──trigger──→ noise_burst[4] (attack transient)
logic_seq[1] (prime, 3Hz) ──trigger──→ strike_voice[3] (bayan, 100Hz)

strike_voice[2] ──audio──┐
strike_voice[3] ──audio──┼──→ mixer_cell[5]
noise_burst[4]  ──audio──┘
```

### Polyrhythmic Structure

Two logic_seq_cells at different rates and algorithms create polyrhythmic counterpoint:
- **Dayan** (euclidean at 4Hz, density=0.6): Regular-ish pattern, ~60% steps filled
- **Bayan** (prime at 3Hz, density=0.4): Sparser, irregular — prime-number accents

The 4:3 rate ratio creates patterns that phase against each other over ~12 beats, never quite repeating the same combination.

### Dialogue Personality

TBLK with fidelity=0.5 creates a balance between following and leading:
- **Rhythm**: 50/50 blend of external gate/accent and internal logic_seq patterns
- **Pitch**: Quantizes any received pitch to nearest resonant membrane mode
  ```rust
  fn quantize_to_membrane_mode(&self, hz: f32) -> f32 {
      let modes = [1.0, 1.59, 2.0, 2.14, 2.65, 3.0]; // tabla membrane modes
      let base = self.membrane_freq;
      modes.iter()
          .map(|&m| base * m)
          .min_by(|a, b| (a - hz).abs().partial_cmp(&(b - hz).abs()).unwrap())
          .unwrap_or(hz)
  }
  ```
- **Cross-organism**: TBLK emits its rhythm_density signal; other organisms can sync to it
- **Counterpoint**: Against KKIT's mechanical precision, TBLK creates organic counterpoint

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/dsp/cell/strike_voice_cell.rs` | Create |
| `src/dsp/cell/noise_burst_cell.rs` | Create |
| `src/dsp/cell/mod.rs` | Modify — register 2 new cells |
| `src/dsp/cell_registry.rs` | Modify — factory functions + param ranges |
| `assets/dna/tblk-dha.json` | Create |

---

## Test Plan (~15 tests)

### strike_voice_cell
- `strike_produces_audio_on_trigger`: trigger → non-zero audio output
- `strike_decays_to_silence`: output falls to near-zero after decay time
- `strike_membrane_freq_sets_pitch`: changing membrane_freq shifts fundamental
- `strike_tension_affects_harmonics`: low tension → inharmonic, high tension → harmonic
- `strike_noise_mix_adds_character`: noise_mix > 0 adds broadband component
- `strike_tone_shapes_spectrum`: low tone → dark, high tone → bright
- `strike_retrigger`: new trigger during decay restarts envelope

### noise_burst_cell
- `noise_burst_on_trigger`: trigger → short noise output
- `noise_burst_duration`: output returns to zero after duration
- `noise_burst_color_filters`: low color → dark noise, high color → bright
- `noise_burst_level_scales`: level=0 → silence

### TBLK integration
- `tblk_loads_dna`: tblk-dha.json loads without error
- `tblk_produces_audio`: organism generates percussive audio
- `tblk_two_voices_distinct`: dayan and bayan produce different timbres
- `tblk_polyrhythm`: two logic_seq_cells produce interleaved patterns

---

## Verification Criteria

- [ ] strike_voice_cell produces resonant percussion with tension-dependent harmonics
- [ ] noise_burst_cell produces short filtered noise transients
- [ ] TBLK plays organic tabla patterns from two logic_seq_cells
- [ ] Dayan (high, harmonic) and bayan (low, inharmonic) are distinct
- [ ] Polyrhythmic structure from 4:3 rate ratio is audible
- [ ] TBLK quantizes external pitch to membrane modes (fidelity=0.5)
- [ ] `cargo test` — all tests pass
