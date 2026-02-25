# S13c — MELO (The Melodizer)

**Layer**: L5 Organism
**Depends on**: S11 (atom primitives), S12 (cell composition + DNA), S13 (organism scaffold)
**Status**: Prospect
**FunDSP**: 0.23.0 (`6969e5c9...`) — all DSP calls verified against `fundsp::prelude32`

> A darting, arpeggiating creature obsessed with filter sweeps and envelope
> shapes. Chases interesting timbral territory. Drawn to other organisms that
> produce rich harmonic content (loves DRON's harmonics, fascinated by TBLK's
> transients). Gets bored of static sounds — needs constant melodic motion.

## Coverage Role

| Dimension | Value |
|-----------|-------|
| Temporal | Rhythmic arpeggiation (fast discrete steps) |
| Pitch | Fast discrete steps across scale, transposed by input |
| Social | Filter/envelope chaser — seeks spectral novelty, repels from static |
| Energy | Medium stamina — needs input to sustain, can self-generate for a while |
| DSP | Melodic: pulse oscillator, filter envelope sweeps, comb/allpass color |
| Visual | Darting small blob, green/yellow thermal palette |

## Personality (Emotion Profile)

```
base_valence:   0.0     (neutral — valence driven entirely by input novelty)
base_arousal:   0.4     (moderately energetic, always moving)
stamina:        medium  (needs input to sustain; can self-generate for a while)
novelty_hunger: high    (valence tanks if receiving the same pitch repeatedly)
social_pull:    chases filter-rich organisms (+0.4 toward high spectral centroid)
social_repel:   avoids static organisms (-0.2 toward low-novelty sources)
envelope_love:  edges to sources with high transient content strengthen fast
```

MELO's valence is driven by pitch variety: receiving the same note twice in a
row → valence drops. Receiving a new scale degree → valence spikes. This makes
its Hebbian learning actively seek diverse pitch sources.

## Composition

```
ORGANISM: MELO
├── CELL: arpeggiator (pattern-driven pitch sequencer)
│   ├── MOLECULE: step_sequencer
│   │   ├── ATOM: clock_source    (internal clock or sync to TBLK hits)
│   │   ├── ATOM: pattern_gen     (up, down, up-down, random, converge)
│   │   └── ATOM: step_counter    (tracks position in pattern)
│   ├── MOLECULE: pitch_mapper
│   │   ├── ATOM: scale_lock      (quantize arp steps to nearest infra pitch)
│   │   ├── ATOM: octave_spread   (spread across 1–3 octaves)
│   │   └── ATOM: transpose       (shift pattern root by received pitch)
│   └── MOLECULE: gate_shaper
│       ├── ATOM: gate_length     (staccato ↔ legato control)
│       └── ATOM: swing           (timing offset on even steps)
├── CELL: timbre_voice (the sound each arp step makes)
│   ├── MOLECULE: osc_pair (two oscillators for harmonic richness)
│   │   ├── ATOM: pulse_osc      (pulse() — PWM modulated via input)
│   │   └── ATOM: sine_sub       (sine_hz one octave below for body)
│   ├── MOLECULE: filter_envelope (the signature sweep)
│   │   ├── ATOM: svf_multi      (lowpass_hz, sweepable cutoff)
│   │   ├── ATOM: env_to_cutoff  (ADSR → cutoff with adjustable depth)
│   │   └── ATOM: key_follow     (higher notes → higher base cutoff)
│   └── MOLECULE: amp_envelope
│       ├── ATOM: fast_adsr      (snappy: A=2ms, D=80ms, S=0.4, R=50ms)
│       └── ATOM: velocity_scale (accent pattern → amplitude)
├── CELL: modulation_matrix
│   ├── MOLECULE: lfo_bank
│   │   ├── ATOM: lfo_pwm        (triangle LFO → pulse width, 2-5Hz)
│   │   ├── ATOM: lfo_filter     (sine LFO → filter cutoff wobble, 0.5-2Hz)
│   │   └── ATOM: lfo_pitch      (subtle vibrato, 5-7Hz, ±10 cents)
│   └── MOLECULE: env_followers
│       ├── ATOM: ext_env_follow  (follow() — follows TBLK hit_energy for rhythmic ducking)
│       └── ATOM: spectral_track  (follows DRON harmonic_field for timbral sympathy)
└── CELL: curiosity_engine
    ├── ATOM: pitch_history      (ring buffer of last 16 received pitches)
    ├── ATOM: novelty_score      (unique pitches / total in buffer)
    ├── ATOM: valence_from_novelty (high novelty → positive valence)
    └── ATOM: pattern_mutator    (low novelty → mutate arp pattern)
```

## Infrastructure Consumption

| Infra Port | MELO Input | Use |
|------------|-----------|-----|
| `quantizer.pitch_hz` [20,20000] Block | `root_pitch` | Base pitch for transposing arp patterns |
| `quantizer.nearest_degree` [0,127] Block | `scale_degree` | Which scale degree to start arp from |
| `keyboard.note_on` [10,16] Event | `trigger_in` | Note-on restarts arp from step 0 |
| `keyboard.note_off` [10,16] Event | `release_in` | Note-off begins arp fadeout |
| `cursor.x` [0,1] Block | `arp_rate` | Cursor X controls arpeggio speed |
| `cursor.y` [0,1] Block | `filter_depth` | Cursor Y controls envelope → filter mod depth |
| `audio_analysis.rms` [0,1] Block | `env_follow` | Side-chains to environment loudness |

## Organism Outputs

| MELO Output | Type | Consumers |
|-------------|------|-----------|
| `arp_pitch` [20,20000] Block | Current arp step frequency | Other organisms can harmonize |
| `arp_gate` Trigger Event | Gate trigger on each arp step | TBLK might sync to MELO's rhythm |
| `filter_position` [0,1] Block | Normalized filter sweep position | Visual modules track the sweep |
| `pattern_entropy` [0,1] Block | How unpredictable the current pattern is | Social signal — high entropy attracts curious organisms |

## Future Infrastructure Preferences

- **Visualizer feedback module** — cursor-when-in-view of MELO's blob → excites arp speed
- **Shader swizzle handle** — filter position drives visual color cycling on MELO's blob
- **CV input module** — external sequencer sync for live performance
- **MIDI input module** — external keyboard overrides arp root
- **3D generative module** — 3D mesh vertex data → modulation matrix routing targets
- **Video stream color module** — dominant hue from video → filter cutoff target
- **Camera motion module** — hand movement speed → arp rate multiplier

## FunDSP DSP Sketch

All function calls verified compilable against `fundsp 0.23.0`.

```rust
use fundsp::prelude32::*;

// Per-arp-step voice (fast retrigger, short envelope)
//
// pulse() takes freq + width as signal inputs (no pulse_hz variant)
// For fixed-freq use: dc(freq) | dc(width) >> pulse()
// Or use square_hz(freq) for 50% duty cycle without PWM
let osc = square_hz(freq) + sine_hz(freq * 0.5) * dc(0.4);

// For PWM modulation, use the signal-input form:
// let pwm_osc = (dc(freq) | lfo_pulse_width) >> pulse();

// Filter with envelope-driven cutoff
// In practice, cutoff is computed per-sample in the atom:
//   env_cutoff = base_cutoff + env_depth * env_level + key_follow * freq
let filtered = osc >> lowpass_hz(env_cutoff, 0.6);

// Envelope follower for external signal tracking
let ext_follower = follow(0.01);  // 10ms smoothing

// Final voice chain
let voice = filtered >> pan(0.0) >> limiter(0.001, 0.02);
```

### Pulse Oscillator Note

FunDSP 0.23 provides `pulse()` (signal-input form) but NOT `pulse_hz()`.
For fixed-frequency pulse, use `(dc(freq) | dc(width)) >> pulse()`.
For the simplest case (50% duty = square), use `square_hz(freq)`.

### FunDSP API Notes

| Spec reference | Actual FunDSP 0.23 | Notes |
|----------------|---------------------|-------|
| `pulse_hz(f, w)` | `(dc(f) \| dc(w)) >> pulse()` | No `pulse_hz` — signal-input form only |
| `square_hz(f)` | `square_hz(f)` | 50% duty cycle square wave, confirmed |
| `lowpass_hz(f, q)` | `lowpass_hz(f, q)` | Sweepable lowpass, confirmed |
| `follow(t)` | `follow(t)` | Envelope follower with smoothing time, confirmed |
| `pan(p)` | `pan(p)` | Stereo panner [-1,1], confirmed |
| `limiter(a, r)` | `limiter(a, r)` | Attack/release limiter, confirmed |
| `osc * 0.4` | `osc * dc(0.4)` | Multiply requires `An<_>` both sides |

## DNA Parameters

```
arp_rate_hz:            4.0         // steps per second (modulatable via cursor.x)
arp_pattern:            "up-down"   // up, down, up-down, random, converge
arp_octaves:            2           // octave spread
gate_length:            0.6         // proportion of step (0=staccato, 1=legato)
swing:                  0.0         // even step timing offset [0, 0.5]
pulse_width:            0.5         // PWM base (0.5 = square)
pwm_depth:              0.3         // LFO modulation of pulse width
filter_base_cutoff:     1200.0      // Hz
filter_env_depth:       3000.0      // Hz added by envelope
filter_key_follow:      0.5         // proportion of note freq added to cutoff
attack_ms:              2.0
decay_ms:               80.0
sustain:                0.4
release_ms:             50.0
vibrato_rate:           5.5         // Hz
vibrato_depth:          10.0        // cents
novelty_window:         16          // ring buffer size for pitch history
pattern_mutate_threshold: 0.3       // novelty score below this → mutate
```

## Social Dynamics

```
MELO → TBLK:  strong (MELO chases transient-rich novelty from hits)
MELO → DRON:  strong initially, decays (harmonic field is novel at first, then static)
TBLK → MELO:  weak (TBLK is antisocial)
DRON → MELO:  medium (DRON appreciates melodic company)
```

MELO actively chases novelty. Its edges to TBLK and DRON strengthen when those
organisms produce varied output. MELO's own output (fast arpeggio) is high-novelty,
so edges FROM MELO tend to be valued by listeners.

### Emergent behavior: timbral sympathy

MELO's `spectral_track` atom follows DRON's `harmonic_field`. When DRON shifts
harmonics, MELO's filter sweeps to match — a kind of learned harmonic agreement.
This makes MELO's edge to DRON periodically re-strengthen, creating a cycle:
decay → DRON shifts → MELO's filter follows → edge strengthens → decay again.

### Emergent behavior: polyrhythmic sync

MELO's `arp_gate` output might sync with TBLK's `hit_trigger`, creating accidental
polyrhythms. If the combined pattern pleases both (positive valence from productive
signal flow), the sync edge strengthens — organisms accidentally lock into groove.

## Verification Criteria

- [ ] MELO produces arpeggiated patterns synced to quantizer pitch
- [ ] Arp patterns play in correct modes (up, down, up-down, random, converge)
- [ ] Cursor X controls arp rate, cursor Y controls filter envelope depth
- [ ] Note-on restarts arp from step 0, note-off triggers fadeout
- [ ] Filter sweeps audibly on each arp step (envelope → cutoff)
- [ ] Key-follow makes higher notes brighter
- [ ] PWM modulation audible via LFO
- [ ] Novelty tracking: valence responds to pitch variety
- [ ] Pattern mutation occurs when novelty drops below threshold
- [ ] `ext_env_follow` ducks MELO when TBLK hits hard
- [ ] `spectral_track` shifts MELO's filter toward DRON's harmonic field
- [ ] Edges to static sources weaken over time
- [ ] Blob renders as darting, small, green/yellow
- [ ] `arp_gate` output provides trigger for other organisms to sync
- [ ] `pattern_entropy` output reflects current pattern unpredictability
