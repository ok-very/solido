# S13a — TBLK (Tabla Machine)

**Layer**: L5 Organism
**Depends on**: S11 (atom primitives), S12 (cell composition + DNA), S13 (organism scaffold)
**Status**: Prospect
**FunDSP**: 0.23.0 (`6969e5c9...`) — all DSP calls verified against `fundsp::prelude32`

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

When starved of input: arousal crashes, valence goes deeply negative, begins
pruning its own edges. When another organism sends it ANY signal, arousal spikes
and it regenerates — bursting with new hits.

## Composition

```
ORGANISM: TBLK
├── CELL: strike_voice (percussive hit)
│   ├── MOLECULE: membrane_sim (pitched body)
│   │   ├── ATOM: noise_burst     (white noise × short envelope)
│   │   ├── ATOM: bandpass_res    (resonator_hz — pitched membrane body)
│   │   └── ATOM: pitch_env       (pitch sweep down: 400Hz → 120Hz in 30ms)
│   ├── MOLECULE: snap_transient (click attack)
│   │   ├── ATOM: impulse         (dc(1.0) >> envelope)
│   │   └── ATOM: highpass_click  (highpass_hz ~3kHz, sharp Q)
│   └── MOLECULE: body_resonance
│       ├── ATOM: comb_body       (feedback delay ~5ms = pitched comb)
│       └── ATOM: decay_env       (fast exp decay, 50-200ms)
├── CELL: pattern_gen (rhythmic sequencer)
│   ├── MOLECULE: euclidean_clock
│   │   ├── ATOM: clock_div       (subdivides incoming triggers)
│   │   └── ATOM: euclidean_gate  (distributes hits across steps)
│   └── MOLECULE: accent_map
│       ├── ATOM: velocity_curve  (maps step position → amplitude)
│       └── ATOM: ghost_note      (quiet fills between main hits)
└── CELL: aggression_modulator
    ├── ATOM: arousal_to_density  (high arousal → more hits per bar)
    ├── ATOM: valence_to_pitch    (negative valence → lower membrane tuning)
    └── ATOM: stamina_decay       (energy drains each hit, regens from social signals)
```

## Infrastructure Consumption

| Infra Port | TBLK Input | Use |
|------------|-----------|-----|
| `keyboard.note_on` [10,16] Event | `trigger_in` | Primary hit trigger (note number → drum voice selection) |
| `keyboard.trigger` Trigger Event | `raw_trigger` | Any-key bang for improv hits |
| `cursor.y` [0,1] Block | `intensity` | Y position modulates hit velocity |
| `cursor.x` [0,1] Block | `membrane_pitch_mod` | X position detunes membrane resonance |
| `audio_analysis.rms` [0,1] Block | `feedback_rms` | Hears its own output — drives stamina regen when loud |
| `audio_analysis.is_active` Bool Block | `silence_detector` | Extended silence triggers stamina drain |

## Organism Outputs (back-pressure)

| TBLK Output | Type | Consumers |
|-------------|------|-----------|
| `hit_trigger` Trigger Event | Trigger burst on each drum hit | Other organisms can sync to TBLK's rhythm |
| `hit_energy` [0,1] Block | Envelope follower of recent hit density | Organisms near TBLK feel rhythmic energy |
| `membrane_pitch` [60,400] Block | Current drum body pitch | DRON might detune toward this, MELO might avoid it |

## Future Infrastructure Preferences

These infrastructure modules don't exist yet but TBLK's DNA should express
affinity biases toward them when they appear:

- **CV input module** — external trigger/gate for drum machine sync
- **Camera motion module** — sudden visual movement → surprise hit
- **3D scene depth module** — Z-depth of hand gesture → strike velocity
- **Shader parameter handle** — TBLK wants to drive visual transient flashes
- **Cursor-when-in-view** — mouse hovering over TBLK's blob → arousal spike, burst of hits

## FunDSP DSP Sketch

All function calls verified compilable against `fundsp 0.23.0`.

```rust
use fundsp::prelude32::*;

// Per-hit voice (instantiated on trigger, released after decay)
//
// Membrane: white noise through a resonator with pitch envelope
let membrane = noise()
    >> resonator_hz(membrane_freq, bandwidth)
    >> envelope2(|t, _| (-t * decay_rate).exp());

// Transient click: DC impulse through a highpass
let click = dc(1.0)
    >> lowpole_hz(8000.0)           // lowpole_hz, NOT onepole_hz
    >> highpass_hz(3000.0, 1.5)
    >> envelope2(|t, _| (-t * 200.0).exp());

// Body resonance: comb filter via feedback delay
// feedback() takes a single node (internal feedback loop)
let body = feedback(delay(0.005) >> lowpass_hz(2000.0, 0.5));

// Final hit = membrane + click → body → limiter
let hit = (membrane + click * dc(0.3)) >> body >> limiter(0.002, 0.05);
```

### FunDSP API Notes

| Spec reference | Actual FunDSP 0.23 | Notes |
|----------------|---------------------|-------|
| `onepole_hz(f)` | `lowpole_hz(f)` | FunDSP names it `lowpole_hz` |
| `feedback(node, gain)` | `feedback(node)` | Single-arg; gain is inside the node chain |
| `noise()` | `noise()` | White noise, confirmed |
| `resonator_hz(f, bw)` | `resonator_hz(f, bw)` | Bandpass resonator, confirmed |
| `envelope2(\|t, x\| ...)` | `envelope2(\|t, x\| ...)` | Time-varying envelope, confirmed |

## DNA Parameters

```
membrane_base_freq:     180.0       // Hz — tabla "na" tuning
membrane_freq_range:    [60, 400]
pitch_sweep_ratio:      3.3         // start freq / end freq
decay_ms:               [50, 300]   // range per DNA
click_mix:              0.3         // transient click blend
body_feedback:          0.4         // comb resonance
euclidean_steps:        [5, 7, 9, 11, 13]   // preferred odd meters
accent_depth:           0.6         // ghost note quietness
stamina_drain_rate:     0.02/tick   // how fast energy depletes
stamina_regen_rate:     0.05/signal // how much each received signal heals
```

## Social Dynamics

```
TBLK → DRON:  weak (TBLK doesn't want connections, repels)
TBLK → MELO:  weak (same — TBLK is antisocial)
DRON → TBLK:  strong (DRON is warm, drawn to rhythmic energy)
MELO → TBLK:  strong (MELO chases transient-rich novelty)
```

TBLK's aggressive output (transient-rich, high arousal) pushes its OWN edges to
weaken (it doesn't want to connect), BUT other organisms' edges TO TBLK strengthen
because transients are high-novelty, high-impact signals that boost receiving
organisms' valence.

### Emergent behavior: isolation cycles

TBLK burns through stamina → goes quiet → other organisms' edges to it weaken →
DRON or MELO signals regen TBLK → it explodes back → recaptures everyone's attention.
This creates a natural rhythmic macro-structure at the ~10-30s timescale.

## Verification Criteria

- [ ] TBLK produces percussive hits when keyboard triggers arrive
- [ ] Membrane resonance is tunable (cursor.x modulates pitch)
- [ ] Hit velocity responds to cursor.y
- [ ] Pattern generator produces euclidean rhythms (odd meters)
- [ ] Ghost notes appear between accented hits
- [ ] Stamina drains during silence, regenerates from social signals
- [ ] TBLK's own edges to others weaken (antisocial behavior)
- [ ] Other organisms' edges to TBLK strengthen (rhythmic attraction)
- [ ] Blob renders as sharp, red/orange with transient flashes
- [ ] `hit_trigger` output allows other organisms to sync
