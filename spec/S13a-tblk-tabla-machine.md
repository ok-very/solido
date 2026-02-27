# S13a — TBLK (Tabla Machine)

**Layer**: L5 Organism
**Depends on**: S11 (atoms), S12 (cells + DNA), S09 (visual sim), S13 (scaffold)
**Status**: Ready (prerequisites complete)
**FunDSP**: 0.23.0

> An aggressive, percussive organism. Hits hard, burns out fast. Ironically
> prefers solitude but other organisms are drawn to its rhythmic energy.
> Must regenerate by proximity to other particles — alone too long, it dies.

## Coverage Role

| Dimension | Value |
|-----------|-------|
| Temporal | Burst / silence cycles |
| Pitch | Pitched percussion (fixed tuning, not melodic) |
| Social | Aggressive loner — repels from others, but others attracted to it |
| Energy | Low stamina, regenerates from social signal input |
| DSP | Noise-based: noise >> resonator, impulse + comb membrane |
| Visual | Sharp transient blob, red/orange thermal palette |

## Personality (Emotion Profile)

```
base_valence:   -0.2    (slightly irritable at rest)
base_arousal:   0.7     (high-strung, ready to strike)
stamina:        low     (arousal decays fast without external stimulation)
regen_trigger:  proximity to other organisms (signal count from non-self)
social_pull:    repulsive (-0.3 toward other organisms)
social_attract: +0.5 from others toward TBLK (others like it)
```

## Composition (Implemented)

```
ORGANISM: TBLK
├── CELL: PatternGen (src/dsp/cell/pattern_gen.rs)
│   ├── ClockAtom (bpm-driven sample counter)
│   ├── Bjorklund euclidean pattern generator
│   ├── Accent map (first beat + every 4th)
│   └── Output: mono gate (velocity on hit, 0.0 on rest)
│
├── CELL: StrikeVoice (src/dsp/cell/strike_voice.rs)
│   ├── MOLECULE: membrane_sim (Fused)
│   │   └── noise() >> resonator() — pitched membrane body
│   ├── MOLECULE: snap_transient (Fused)
│   │   └── dc(1.0) >> lowpole_hz(8000) >> highpass_hz(3000, 1.5)
│   ├── MOLECULE: body_resonance (Fused)
│   │   └── feedback(delay(t) >> lowpass_hz(2000, 0.5))
│   └── Output: mono (membrane + click → body → soft-clip)
│
└── WIRING: PatternGen →[Trigger]→ StrikeVoice
    (scratch[0][0] > 0.5 → DspCommand::NoteOn to StrikeVoice)
```

### DNA (assets/dna/tblk-alpha.json)

```json
{
  "cells": [
    { "cell_type": "pattern_gen", "params": { "bpm": 120, "steps": 7, "hits": 5, "accent_depth": 0.6 } },
    { "cell_type": "strike_voice", "params": { "membrane_freq": 180, "bandwidth": 60, "click_mix": 0.3, "body_feedback": 0.4 } }
  ],
  "cell_wiring": [{ "src_cell": 0, "dst_cell": 1, "wire_type": "Trigger" }]
}
```

### SharedHandles

| Handle | Cell | Controls |
|--------|------|----------|
| `cell0.bpm` | PatternGen | Clock tempo |
| `cell0.steps` | PatternGen | Euclidean step count |
| `cell0.hits` | PatternGen | Euclidean hit count |
| `cell0.accent_depth` | PatternGen | Ghost note quietness |
| `cell0.swing` | PatternGen | Even-step timing offset |
| `cell1.membrane_freq` | StrikeVoice | Membrane resonance frequency |
| `cell1.bandwidth` | StrikeVoice | Resonator bandwidth |
| `cell1.click_mix` | StrikeVoice | Transient click blend |
| `cell1.body_feedback` | StrikeVoice | Comb resonance amount |

## Infrastructure Consumption

| Infra Port | OrganismModule.receive_signal() | Action |
|------------|-------------------------------|--------|
| `keyboard.note_on` | `cmd_tx.try_send(DspCommand::NoteOn)` | Hit trigger |
| `keyboard.trigger` | `cmd_tx.try_send(DspCommand::NoteOn)` | Any-key bang |
| `cursor.y` [0,1] | `shared_handles["cell1.membrane_freq"].set(mapped)` | Modulate membrane pitch |
| `cursor.x` [0,1] | `shared_handles["cell0.bpm"].set(mapped)` | Modulate tempo |
| `audio_analysis.rms` | Update emotion arousal regen | Stamina regen from loudness |

## Organism Outputs

| Output | Type | Source |
|--------|------|--------|
| `hit_trigger` | Trigger Event | PatternGen output > 0.5 |
| `hit_energy` [0,1] | Block | DspAnalysis.rms from OrganismDsp |
| `membrane_pitch` [60,400] | Block | Current membrane_freq Shared value |

## Social Dynamics

```
TBLK → DRON:  weak (TBLK doesn't want connections, repels)
TBLK → MELO:  weak (same — TBLK is antisocial)
DRON → TBLK:  strong (DRON is warm, drawn to rhythmic energy)
MELO → TBLK:  strong (MELO chases transient-rich novelty)
```

### Emergent behavior: isolation cycles

TBLK burns through stamina → goes quiet → other organisms' edges to it weaken →
DRON or MELO signals regen TBLK → it explodes back → recaptures everyone's attention.
Creates natural rhythmic macro-structure at ~10-30s timescale.

## Verification Criteria

- [ ] TBLK produces percussive hits when keyboard triggers arrive
- [ ] Membrane resonance is tunable (cursor.y modulates pitch)
- [ ] Pattern generator produces euclidean rhythms (odd meters)
- [ ] Ghost notes appear between accented hits at lower velocity
- [ ] Stamina drains during silence, regenerates from social signals
- [ ] TBLK's own edges to others weaken (antisocial behavior)
- [ ] Other organisms' edges to TBLK strengthen (rhythmic attraction)
- [ ] Blob renders as sharp, red/orange with transient flashes
- [ ] `hit_trigger` output allows other organisms to sync
- [ ] SharedHandles respond to infrastructure signals in real-time
