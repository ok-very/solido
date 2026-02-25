# S13b — DRON (The Droner)

**Layer**: L5 Organism
**Depends on**: S11 (atom primitives), S12 (cell composition + DNA), S13 (organism scaffold)
**Status**: Prospect
**FunDSP**: 0.23.0 (`6969e5c9...`) — all DSP calls verified against `fundsp::prelude32`

> A vast, warm presence. Gets along with everyone but others eventually get
> bored. Emits continuous harmonic fields that slowly evolve. Infinite stamina
> but low excitement — other organisms' valence toward DRON decays over time
> unless DRON introduces variation.

## Coverage Role

| Dimension | Value |
|-----------|-------|
| Temporal | Continuous — always producing signal |
| Pitch | Slowly drifting (brownian pitch wander ±50 cents) |
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
                (others' Hebbian decay outpaces DRON's slow signal novelty)
social_pull:    mildly attractive (+0.1)
social_repel:   none — DRON never pushes anyone away
variation_need: must introduce pitch/timbre drift or edges to it decay faster
```

When DRON's arousal spikes (rare — maybe from TBLK hits or MELO activity), it
shifts its harmonic series, briefly recapturing attention from other organisms.

## Composition

```
ORGANISM: DRON
├── CELL: harmonic_bed (continuous pitched drone)
│   ├── MOLECULE: detuned_stack (rich harmonic core)
│   │   ├── ATOM: saw_osc_1      (saw_hz, base pitch)
│   │   ├── ATOM: saw_osc_2      (saw_hz, +7 cents)
│   │   ├── ATOM: saw_osc_3      (saw_hz, -5 cents)
│   │   └── ATOM: sub_sine       (sine_hz one octave below, adds weight)
│   ├── MOLECULE: slow_filter (timbral evolution)
│   │   ├── ATOM: svf_lowpass    (lowpass_hz, cutoff LFO'd)
│   │   ├── ATOM: lfo_cutoff     (sine LFO 0.03–0.1 Hz → cutoff)
│   │   └── ATOM: resonance_mod  (arousal → resonance amount)
│   └── MOLECULE: stereo_spread
│       ├── ATOM: pan_lfo_l      (slow pan left, slightly out of phase)
│       └── ATOM: pan_lfo_r      (slow pan right)
├── CELL: shimmer_layer (upper harmonics that come and go)
│   ├── MOLECULE: octave_up
│   │   ├── ATOM: pitch_shift    (+12 semitones, or 2x frequency sine)
│   │   └── ATOM: soft_gate      (amplitude fades in/out on LFO)
│   └── MOLECULE: reverb_wash
│       ├── ATOM: allpass_chain  (allpass_hz series for diffusion)
│       └── ATOM: feedback_delay (feedback(delay >> lowpass) for wash)
├── CELL: drift_controller (slow parameter evolution)
│   ├── ATOM: pitch_wander      (brownian motion on base pitch, ±50 cents)
│   ├── ATOM: detune_breathe    (detune amount oscillates 2–15 cents)
│   └── ATOM: harmonic_shift    (every ~30s, shift harmonic series by a 5th or 4th)
└── CELL: social_warmth
    ├── ATOM: signal_absorber   (receives signals from other organisms passively)
    ├── ATOM: novelty_tracker   (tracks how much its output has changed recently)
    └── ATOM: boredom_counter   (if novelty < threshold for too long, trigger harmonic_shift)
```

## Infrastructure Consumption

| Infra Port | DRON Input | Use |
|------------|-----------|-----|
| `quantizer.pitch_hz` [20,20000] Block | `root_pitch` | Quantized root note — DRON tunes its base to this |
| `cursor.x` [0,1] Block | `cutoff_bias` | Cursor X gently biases filter cutoff |
| `cursor.y` [0,1] Block | `detune_amount` | Cursor Y controls how wide the detuning spread is |
| `audio_analysis.rms` [0,1] Block | `environment_energy` | When environment is loud, DRON gets slightly louder |
| `audio_analysis.peak` [0,1] Block | `transient_detector` | Peaks from TBLK hits cause momentary shimmer brightening |

## Organism Outputs

| DRON Output | Type | Consumers |
|-------------|------|-----------|
| `drone_pitch` [20,2000] Block | Current root frequency (drifting) | MELO might tune arpeggios relative to this |
| `harmonic_field` [0,1] Block | Spectral centroid (how bright/dark the drone is) | Visual organisms track timbral state |
| `warmth` [0,1] Block | Smoothed output RMS (steady presence indicator) | Other organisms feel DRON's steady presence |

## Future Infrastructure Preferences

- **Video stream / webcam module** — frame brightness → filter cutoff evolution
- **3D depth module** — depth field → reverb size parameter
- **Shader swizzle handle** — DRON wants to slowly shift the hue of its blob
- **Microphone input module** — acoustic room resonance → pitch alignment
- **OSC control surface** — slider for manual harmonic series selection
- **Visualizer feedback** — blob size feeds back into filter resonance (bigger blob → more resonance)

## FunDSP DSP Sketch

All function calls verified compilable against `fundsp 0.23.0`.

```rust
use fundsp::prelude32::*;

// Continuous voice (always active, parameters modulated over time)
//
// Core: three detuned saws summed to mono through lowpass
let core = (saw_hz(root) | saw_hz(root * detune_up) | saw_hz(root * detune_down))
    >> join::<U3>()             // mix three saws to mono (average)
    >> lowpass_hz(cutoff, resonance)
    >> pan(pan_position);

// Shimmer: octave-up sine through allpass diffusion chain
let shimmer = sine_hz(root * 2.0)
    >> allpass_hz(1200.0, 0.5)
    >> allpass_hz(800.0, 0.5)
    >> feedback(delay(0.08) >> lowpass_hz(3000.0, 0.3));

// Final drone = core + shimmer → limiter → DC removal
let drone = (core + shimmer * dc(shimmer_amount))
    >> limiter(0.01, 0.2)
    >> dcblock();
```

### FunDSP API Notes

| Spec reference | Actual FunDSP 0.23 | Notes |
|----------------|---------------------|-------|
| `saw_hz(f)` | `saw_hz(f)` | Fixed-frequency sawtooth, confirmed |
| `join::<U3>()` | `join::<U3>()` | Mix N inputs to mono (average), confirmed |
| `lowpass_hz(f, q)` | `lowpass_hz(f, q)` | 2nd order lowpass, confirmed |
| `allpass_hz(f, q)` | `allpass_hz(f, q)` | Allpass for diffusion, confirmed |
| `feedback(node)` | `feedback(node)` | Single-arg internal feedback, confirmed |
| `dcblock()` | `dcblock()` | DC blocker (default frequency), confirmed |
| `shimmer * 0.3` | `shimmer * dc(0.3)` | Multiply requires `An<_>` on both sides |

## DNA Parameters

```
root_hz:                130.0       // ~C3 — warm bass register
detune_cents:           [2, 15]     // range of detuning spread
cutoff_base:            800.0       // Hz — warm default
cutoff_lfo_rate:        0.05        // Hz — very slow
cutoff_lfo_depth:       400.0       // Hz
shimmer_amount:         0.15        // blend of upper octave
reverb_feedback:        0.6
drift_rate:             0.001       // brownian step size per tick (cents)
drift_range:            [-50, 50]   // max drift from root (cents)
harmonic_shift_interval: 1800       // ticks (~30s at 60fps) between shifts
boredom_threshold:      0.02        // novelty below this → harmonic shift
```

## Social Dynamics

```
DRON → TBLK:  weak→medium (DRON is warm, drawn to rhythmic energy)
DRON → MELO:  medium (appreciates melodic company)
TBLK → DRON:  weak (TBLK doesn't want connections)
MELO → DRON:  strong initially, decays (MELO seeks novelty, DRON is steady)
```

DRON's continuous, slowly-varying output initially strengthens edges TO DRON
(warmth feels good) but because DRON's signal has low novelty, other organisms'
valence toward it gradually decays. DRON must periodically shift its harmonic
series to recapture attention.

### Emergent behavior: background fade

DRON is always present but edges to it slowly weaken unless it shifts harmonics.
Becomes a warm substrate that other organisms orbit — occasionally recapturing
attention through harmonic shifts triggered by its own boredom counter.

When TBLK's arousal-spike hits arrive via `audio_analysis.peak`, DRON's shimmer
layer brightens momentarily — a sympathetic transient response that keeps its
output just novel enough to delay edge decay.

## Verification Criteria

- [ ] DRON produces continuous sound at all times (never silent)
- [ ] Three detuned saws create a rich chorusing harmonic bed
- [ ] Filter cutoff modulates slowly via internal LFO (~0.05 Hz)
- [ ] Cursor X biases filter cutoff, cursor Y controls detune spread
- [ ] Shimmer layer fades in and out on slow LFO
- [ ] Pitch wander drifts root ±50 cents via brownian motion
- [ ] Harmonic shift occurs every ~30s (or when boredom threshold hit)
- [ ] Edges FROM other organisms TO DRON weaken over time without novelty
- [ ] Edges strengthen briefly after harmonic shift
- [ ] `audio_analysis.peak` triggers momentary shimmer brightening
- [ ] Blob renders as large, diffuse, cool blue/cyan
- [ ] `drone_pitch` output tracks actual drifting root frequency
- [ ] `warmth` output provides steady RMS presence signal
