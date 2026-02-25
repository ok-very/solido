# S13 — First Organisms: Three Creatures for the Petri Dish

**Layer**: L2–L5 (Atoms through Organisms)
**Depends on**: S05c (two-tier architecture), S11 (atom primitives), S12 (cell composition + DNA)
**Status**: Prospect
**FunDSP**: 0.23.0 (checksum `6969e5c9d5c5c704723f547237494fde9b1addf16f718f65cd786b3b6e320a4f`)

## Sub-Specs

| Spec | Organism | Personality |
|------|----------|-------------|
| [S13a](S13a-tblk-tabla-machine.md) | **TBLK** (Tabla Machine) | Aggressive loner, low stamina, liked by others |
| [S13b](S13b-dron-the-droner.md) | **DRON** (The Droner) | Warm presence, infinite stamina, others get bored |
| [S13c](S13c-melo-the-melodizer.md) | **MELO** (The Melodizer) | Novelty-hungry arpeggiator, chases filter/envelope organisms |

## Why These Three

These organisms are chosen to stress-test the full composition hierarchy and give
maximum coverage across different infrastructure consumption patterns, temporal
behaviors, social dynamics, and DSP requirements.

| Dimension | TBLK | DRON | MELO |
|-----------|------|------|------|
| Temporal | Burst / silence | Continuous | Rhythmic arpeggiation |
| Pitch | Pitched percussion (fixed) | Slowly drifting | Fast discrete steps |
| Social | Aggressive loner, liked by others | Passive, boring long-term | Filter/envelope chaser |
| Energy | Low stamina, regenerates near others | Infinite stamina | Medium, feeds on movement |
| Infra needs | Keyboard triggers, cursor intensity | Audio analysis (rms), cursor position | Quantizer pitch, keyboard triggers |
| Visual | Sharp transient blob, red/orange | Large diffuse blob, cool blue/cyan | Darting small blob, green/yellow |
| FunDSP | noise >> resonator, impulse + comb | detuned saws, allpass diffusion | pulse/square, filter envelopes |

Together they exercise:
- Trigger vs continuous vs sequenced signal consumption
- Short vs infinite vs medium voice lifetimes
- Noise-based vs harmonic vs melodic DSP
- All current infrastructure ports
- Inter-organism social dynamics (affinity graph learning)
- Three distinct emotional profiles

---

## Social Dynamics Matrix

How these three organisms interact through the AffinityGraph:

```
         TBLK          DRON          MELO
TBLK   [self:-0.3]    weak→DRON     weak→MELO
DRON   strong→TBLK    [self:+0.3]   medium→MELO
MELO   strong→TBLK    strong→DRON   [self:0.0]

Legend: "strong→X" = that organism's edges to X strengthen quickly
```

**TBLK → others**: TBLK's aggressive output (transient-rich, high arousal) pushes
its own edges to weaken (it doesn't want to connect), BUT other organisms' edges
TO TBLK strengthen because transients are high-novelty, high-impact signals that
boost receiving organisms' valence.

**DRON → others**: DRON's continuous, slowly-varying output initially strengthens
edges TO DRON (warmth feels good) but because DRON's signal has low novelty, other
organisms' valence toward it gradually decays. DRON must periodically shift its
harmonic series to recapture attention.

**MELO → others**: MELO actively chases novelty. Its edges to TBLK and DRON
strengthen when those organisms produce varied output. MELO's own output (fast
arpeggio) is high-novelty, so edges FROM MELO tend to be valued by listeners.

### Emergent Behaviors

1. **TBLK isolation cycles**: TBLK burns through stamina, goes quiet, other
   organisms' edges to it weaken. Then DRON or MELO signals regen TBLK, it
   explodes back, recapturing everyone's attention. (~10-30s macro rhythm)

2. **DRON background fade**: DRON is always present but edges to it slowly weaken
   unless it shifts harmonics. Becomes a warm substrate that other organisms orbit.

3. **MELO synchronization**: MELO's arp_gate output might sync with TBLK's
   hit_trigger, creating accidental polyrhythms. If the combined pattern pleases
   both, the sync edge strengthens.

4. **Timbral sympathy**: MELO's spectral_track atom follows DRON's harmonic_field.
   When DRON shifts harmonics, MELO's filter sweeps to match — a learned harmonic
   agreement that periodically re-strengthens their edge.

---

## FunDSP 0.23 Verified API Surface

All DSP sketches in S13a/b/c use only functions verified compilable against
`fundsp 0.23.0`. The following test lives in `src/audio/master_bus.rs`:

```rust
#[test] fn fundsp_api_surface_check()  // verifies all referenced functions
```

### Confirmed Functions

| Category | Function | Signature |
|----------|----------|-----------|
| Oscillators | `noise()` | White noise generator |
| | `pink()` | Pink noise generator |
| | `sine_hz(f)` | Fixed-frequency sine |
| | `saw_hz(f)` | Fixed-frequency sawtooth |
| | `square_hz(f)` | Fixed-frequency square (50% duty) |
| | `pulse()` | Pulse oscillator (signal-input: freq, width) |
| Filters | `lowpass_hz(f, q)` | 2nd order lowpass |
| | `highpass_hz(f, q)` | 2nd order highpass |
| | `butterpass_hz(f)` | Butterworth **lowpass** (name misleads — type is `ButterLowpass`) |
| | `resonator_hz(f, bw)` | Bandpass resonator |
| | `bell_hz(f, q, gain)` | Bell/peak EQ |
| | `allpass_hz(f, q)` | Allpass (diffusion) |
| | `lowpole_hz(f)` | One-pole lowpass |
| Effects | `delay(t)` | Delay line |
| | `feedback(node)` | Single-node internal feedback loop |
| | `limiter(attack, release)` | Mono limiter |
| | `limiter_stereo(a, r)` | Linked stereo limiter |
| | `dcblock()` | DC blocker (default freq) |
| | `dcblock_hz(f)` | DC blocker (custom freq) |
| | `declick_s(t)` | Startup fade-in |
| | `pan(p)` | Stereo panner |
| Generators | `dc(v)` | Constant value |
| Analysis | `follow(t)` | Envelope follower |
| Envelopes | `envelope2(\|t, x\| ...)` | Time-varying envelope |
| Mixing | `join::<UN>()` | Mix N inputs to mono |

### Corrections from Original Draft

| Original (wrong) | Corrected | Reason |
|-------------------|-----------|--------|
| `onepole_hz(f)` | `lowpole_hz(f)` | FunDSP names it `lowpole_hz` |
| `feedback(node, gain)` | `feedback(node)` | Single arg; gain goes inside the node chain |
| `pulse_hz(f, w)` | `(dc(f) \| dc(w)) >> pulse()` | No `pulse_hz`; signal-input form only |
| `node * 0.3` | `node * dc(0.3)` | FunDSP multiply requires `An<_>` on both sides |
| `feedback2(node, 0.4)` | `feedback2(node, dc(0.4))` | Second arg must be `An<_>`, not raw float |

---

## Prerequisite Sessions

These organisms depend on layers that don't exist yet:

| Prereq | Session | What |
|--------|---------|------|
| Atom primitives | S11 | noise, oscillators, filters, envelopes, LFOs, delays as `ModuleCore` atoms with `Organism` tier |
| Molecule wiring | S11 | Fixed internal wiring between atoms (not learned) |
| Cell composition | S12 | Combining molecules into cells with identity, parameter interfaces |
| DNA serialization | S12 | Save/load/clone/mutate organism blueprints |
| Organism scaffold | S13 | `OrganismModule` wrapper that contains cells, has emotions, routes through AffinityGraph |
| Additional infra | S06+ | Rhythm/raga infrastructure (for TBLK euclidean patterns), camera (for future infra preferences) |

### S11 Atom Inventory (minimum for these three organisms)

| Atom | FunDSP basis | Used by |
|------|-------------|---------|
| `NoiseAtom` | `noise()` | TBLK membrane |
| `SineAtom` | `sine_hz(f)` | DRON sub, MELO sub |
| `SawAtom` | `saw_hz(f)` | DRON core |
| `PulseAtom` | `pulse()` (signal-input) | MELO osc |
| `SquareAtom` | `square_hz(f)` | MELO osc (simple PWM-free variant) |
| `LowpassAtom` | `lowpass_hz(f, q)` | DRON filter, MELO filter |
| `HighpassAtom` | `highpass_hz(f, q)` | TBLK click |
| `BandpassAtom` | `resonator_hz(f, bw)` | TBLK membrane |
| `AllpassAtom` | `allpass_hz(f, q)` | DRON shimmer diffusion |
| `AdsrAtom` | custom (existing `AdsrState`) | all three |
| `LfoAtom` | `sine_hz(f)` at sub-audio rate | DRON cutoff, MELO PWM/vibrato |
| `DelayAtom` | `delay(t)` | TBLK comb body, DRON reverb |
| `GateAtom` | threshold comparator | MELO gate shaper |
| `EnvFollowAtom` | `follow(t)` | MELO ext follow |
| `ClockAtom` | internal tick counter + division | TBLK euclidean, MELO arp |
| `PanAtom` | `pan(p)` | DRON stereo, MELO stereo |
| `LowpoleAtom` | `lowpole_hz(f)` | TBLK transient shaping |

### S12 Cell Inventory

| Cell | Contains | Organism |
|------|----------|----------|
| `StrikeVoice` | membrane_sim + snap_transient + body_resonance | TBLK |
| `PatternGen` | euclidean_clock + accent_map | TBLK |
| `HarmonicBed` | detuned_stack + slow_filter + stereo_spread | DRON |
| `ShimmerLayer` | octave_up + reverb_wash | DRON |
| `Arpeggiator` | step_sequencer + pitch_mapper + gate_shaper | MELO |
| `TimbreVoice` | osc_pair + filter_envelope + amp_envelope | MELO |
| `ModMatrix` | lfo_bank + env_followers | MELO |

---

## Verification Criteria (integration)

When all three organisms are running simultaneously:

- [ ] TBLK produces percussive hits when keyboard triggers arrive
- [ ] TBLK goes quiet when isolated, regenerates when other organisms signal it
- [ ] DRON produces continuous sound, slowly evolving timbre
- [ ] DRON's edges from other organisms weaken over time unless DRON shifts harmonics
- [ ] MELO produces arpeggiated patterns synced to quantizer pitch
- [ ] MELO's valence responds to pitch variety (tanks on repetition, spikes on novelty)
- [ ] AffinityGraph shows learned edge weight evolution between all three
- [ ] Ledger records Hebbian updates, exploration events, and pruning
- [ ] No infrastructure modules have emotions or learned weights
- [ ] Blob renderer shows three blobs with distinct thermal colors and sizes
- [ ] Organisms can be saved to DNA files and reloaded
- [ ] `cargo test` — all organism atoms, molecules, cells, and organisms have unit tests
- [ ] `fundsp_api_surface_check` test passes (all DSP functions compilable)
