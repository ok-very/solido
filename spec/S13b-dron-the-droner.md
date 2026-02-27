# S13b — DRON (The Droner)

**Layer**: L5 Organism
**Depends on**: S11 (atoms), S12 (cells + DNA), S09 (visual sim), S13 (scaffold)
**Status**: Ready (prerequisites complete)
**FunDSP**: 0.23.0

> A vast, warm presence. Gets along with everyone but others eventually get
> bored. Emits continuous harmonic fields that slowly evolve. Infinite stamina
> but low excitement — other organisms' valence toward DRON decays over time
> unless DRON introduces variation.

## Coverage Role

| Dimension | Value |
|-----------|-------|
| Temporal | Continuous — always producing signal |
| Pitch | Slowly drifting (brownian pitch wander via control thread) |
| Social | Passive, warm — others get bored unless DRON shifts harmonics |
| Energy | Infinite stamina |
| DSP | Harmonic: detuned saws, allpass diffusion, slow filter LFO |
| Visual | Large diffuse blob, cool blue/cyan thermal palette |

## Personality (Emotion Profile)

```
base_valence:   +0.3    (contentedly warm)
base_arousal:   0.1     (deeply calm)
stamina:        infinite (always producing signal)
boredom_effect: edges FROM other organisms TO DRON weaken over time
social_pull:    mildly attractive (+0.1)
social_repel:   none — DRON never pushes anyone away
variation_need: must introduce pitch/timbre drift or edges to it decay faster
```

## Composition (Implemented)

```
ORGANISM: DRON
├── CELL: HarmonicBed (src/dsp/cell/harmonic_bed.rs)
│   ├── MOLECULE: detuned_stack (Fused)
│   │   └── 3 detuned saws + sub sine → Shared freq handles
│   ├── MOLECULE: slow_filter (Fused)
│   │   └── lowpass() with Shared cutoff + q
│   ├── MOLECULE: stereo_spread (Wired)
│   │   └── LfoAtom → PanAtom for stereo movement
│   └── Output: stereo (2ch)
│
├── CELL: ShimmerLayer (src/dsp/cell/shimmer_layer.rs)
│   ├── SineAtom (octave-up of root)
│   ├── 3x AllpassAtom chain (diffusion at 1200, 800, 500 Hz)
│   ├── DelayAtom + LowpassAtom in feedback path
│   └── Output: pseudo-stereo (2ch, inverted R channel)
│
└── WIRING: none (both cells produce audio independently, summed by OrganismDsp)
```

### DNA (assets/dna/dron-alpha.json)

```json
{
  "cells": [
    { "cell_type": "harmonic_bed", "params": { "root_hz": 110, "detune_cents": 5, "cutoff": 800, "resonance": 0.707, "pan_rate": 0.05 } },
    { "cell_type": "shimmer_layer", "params": { "shimmer_amount": 0.3, "diffusion": 0.5, "feedback": 0.3 } }
  ],
  "cell_wiring": []
}
```

### SharedHandles

| Handle | Cell | Controls |
|--------|------|----------|
| `cell0.f1` | HarmonicBed | Saw oscillator 1 frequency |
| `cell0.f2` | HarmonicBed | Saw oscillator 2 (detuned up) |
| `cell0.f3` | HarmonicBed | Saw oscillator 3 (detuned down) |
| `cell0.f_sub` | HarmonicBed | Sub sine frequency |
| `cell0.cutoff` | HarmonicBed | Filter cutoff |
| `cell0.q` | HarmonicBed | Filter resonance |
| `cell0.lfo.rate` | HarmonicBed | Stereo pan LFO rate |
| `cell0.lfo.depth` | HarmonicBed | Stereo pan LFO depth |
| `cell1.shimmer_freq` | ShimmerLayer | Shimmer oscillator frequency |

### Control-Thread Drift

DRON's pitch wander and harmonic shifts happen on the control thread (60Hz),
not the audio thread. OrganismModule.tick() implements:

```
// Brownian pitch wander (±50 cents from root)
drift += rng.sample_normal() * drift_rate;
drift = drift.clamp(-50.0, 50.0);
let freq = root_hz * 2.0_f32.powf(drift / 1200.0);
shared_handles["cell0.f1"].set(freq);
shared_handles["cell0.f2"].set(freq * detune_up);
shared_handles["cell0.f3"].set(freq * detune_down);
shared_handles["cell0.f_sub"].set(freq * 0.5);

// Slow filter LFO (0.03-0.1 Hz) driven by control thread
filter_phase += dt * cutoff_lfo_rate;
let cutoff = cutoff_base + cutoff_lfo_depth * filter_phase.sin();
shared_handles["cell0.cutoff"].set(cutoff);

// Harmonic shift (every ~30s or when boredom threshold hit)
if novelty < boredom_threshold || ticks_since_shift > shift_interval {
    root_hz *= [1.5, 0.75, 1.333, 0.667].choose(&mut rng);  // 5th/4th up/down
}
```

## Infrastructure Consumption

| Infra Port | OrganismModule.receive_signal() | Action |
|------------|-------------------------------|--------|
| `quantizer.pitch_hz` | `root_hz = v` (next tick updates Shared handles) | Quantized root |
| `cursor.x` [0,1] | `shared_handles["cell0.cutoff"].set(mapped)` | Bias filter cutoff |
| `cursor.y` [0,1] | Adjust detune_cents spread | Control detuning width |
| `audio_analysis.rms` | Scale output gain slightly | Match environment energy |
| `audio_analysis.peak` | Brighten shimmer temporarily | Sympathetic transient response |

## Organism Outputs

| Output | Type | Source |
|--------|------|--------|
| `drone_pitch` [20,2000] | Block | Current drifting root frequency |
| `harmonic_field` [0,1] | Block | Normalized cutoff position (brightness) |
| `warmth` [0,1] | Block | DspAnalysis.rms — steady presence |

## Social Dynamics

```
DRON → TBLK:  weak→medium (DRON is warm, drawn to rhythmic energy)
DRON → MELO:  medium (appreciates melodic company)
TBLK → DRON:  weak (TBLK doesn't want connections)
MELO → DRON:  strong initially, decays (MELO seeks novelty, DRON is steady)
```

### Emergent behavior: background fade

DRON is always present but edges to it slowly weaken unless it shifts harmonics.
Becomes a warm substrate that other organisms orbit — occasionally recapturing
attention through harmonic shifts triggered by its own boredom counter.

## DNA Parameters

```
root_hz:                130.0       // ~C3
detune_cents:           [2, 15]     // range of detuning spread
cutoff_base:            800.0       // Hz
cutoff_lfo_rate:        0.05        // Hz — very slow
cutoff_lfo_depth:       400.0       // Hz
shimmer_amount:         0.15        // blend of upper octave
drift_rate:             0.001       // brownian step size per tick (cents)
drift_range:            [-50, 50]   // max drift from root (cents)
harmonic_shift_interval: 1800       // ticks (~30s at 60fps) between shifts
boredom_threshold:      0.02        // novelty below this → harmonic shift
```

## Verification Criteria

- [ ] DRON produces continuous sound at all times (never silent)
- [ ] Three detuned saws create rich chorusing via HarmonicBed
- [ ] Filter cutoff modulates slowly via control-thread LFO (~0.05 Hz)
- [ ] Cursor X biases filter cutoff, cursor Y controls detune spread
- [ ] Shimmer layer provides upper-octave diffusion via ShimmerLayer
- [ ] Pitch wander drifts root ±50 cents via brownian motion
- [ ] Harmonic shift occurs every ~30s (or when boredom threshold hit)
- [ ] SharedHandles respond to infrastructure signals in real-time
- [ ] Blob renders as large, diffuse, cool blue/cyan
- [ ] `warmth` output provides steady RMS presence signal
