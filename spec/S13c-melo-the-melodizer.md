# S13c — MELO (The Melodizer)

**Layer**: L5 Organism
**Depends on**: S11 (atoms), S12 (cells + DNA), S09 (visual sim), S13 (scaffold)
**Status**: Ready (prerequisites complete)
**FunDSP**: 0.23.0

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
```

## Composition (Implemented)

```
ORGANISM: MELO
├── CELL: Arpeggiator (src/dsp/cell/arpeggiator.rs)
│   ├── ClockAtom (rate_hz-driven, converted to BPM internally)
│   ├── 5 pattern modes: Up, Down, UpDown, Random, Converge
│   ├── Octave spread (1-3 octaves)
│   ├── Gate timer (staccato ↔ legato via gate_length param)
│   └── Output: mono gate (1.0 during note, 0.0 during rest)
│
├── CELL: TimbreVoice (src/dsp/cell/timbre_voice.rs)
│   ├── MOLECULE: osc_pair (Fused)
│   │   └── square() + sine() — two oscillators for harmonic richness
│   ├── MOLECULE: filter_envelope (Wired)
│   │   └── AdsrAtom → LowpassAtom cutoff modulation
│   ├── MOLECULE: amp_envelope (Wired)
│   │   └── AdsrAtom × audio — amplitude shaping
│   └── Output: mono (oscillator → filter sweep → amplitude envelope)
│
├── CELL: ModMatrix (src/dsp/cell/mod_matrix.rs)
│   ├── LFO: pwm (triangle, 2-5 Hz → pulse width modulation)
│   ├── LFO: filter (sine, 0.5-2 Hz → filter cutoff wobble)
│   ├── LFO: vibrato (sine, 5-7 Hz → pitch deviation ±10 cents)
│   ├── EnvFollowAtom (follows external input for ducking)
│   └── Output: mono (LFO sum — not directly used for audio)
│
└── WIRING: Arpeggiator →[Trigger]→ TimbreVoice
    (scratch[0][0] > 0.5 → DspCommand::NoteOn to TimbreVoice)
```

### DNA (assets/dna/melo-alpha.json)

```json
{
  "cells": [
    { "cell_type": "arpeggiator", "params": { "rate_hz": 4, "pattern": 0, "octaves": 1, "gate_length": 0.5 } },
    { "cell_type": "timbre_voice", "params": { "freq": 440, "pulse_width": 0.5, "filter_base": 200, "filter_depth": 5000, "filter_q": 0.707, "attack_ms": 5, "decay_ms": 100, "sustain": 0.7, "release_ms": 200 } },
    { "cell_type": "mod_matrix", "params": { "pwm_rate": 2, "pwm_depth": 0.2, "filter_lfo_rate": 0.5, "vibrato_rate": 5, "vibrato_depth": 10 } }
  ],
  "cell_wiring": [{ "src_cell": 0, "dst_cell": 1, "wire_type": "Trigger" }]
}
```

### SharedHandles

| Handle | Cell | Controls |
|--------|------|----------|
| `cell0.rate_hz` | Arpeggiator | Steps per second |
| `cell0.pattern` | Arpeggiator | Pattern mode (0-4) |
| `cell0.octaves` | Arpeggiator | Octave spread |
| `cell0.gate_length` | Arpeggiator | Staccato/legato |
| `cell0.swing` | Arpeggiator | Even-step timing offset |
| `cell1.freq` | TimbreVoice | Oscillator frequency |
| `cell1.freq_sub` | TimbreVoice | Sub oscillator frequency |
| `cell1.cutoff` | TimbreVoice | Filter base cutoff |
| `cell1.q` | TimbreVoice | Filter resonance |
| `cell1.adsr.a` | TimbreVoice | Attack time |
| `cell1.adsr.d` | TimbreVoice | Decay time |
| `cell1.adsr.s` | TimbreVoice | Sustain level |
| `cell1.adsr.r` | TimbreVoice | Release time |
| `cell2.pwm_rate` | ModMatrix | PWM LFO rate |
| `cell2.pwm_depth` | ModMatrix | PWM LFO depth |
| `cell2.filter_lfo_rate` | ModMatrix | Filter wobble rate |
| `cell2.vibrato_rate` | ModMatrix | Vibrato rate |
| `cell2.vibrato_depth` | ModMatrix | Vibrato depth (cents) |

### Control-Thread Novelty Tracking

MELO's novelty engine runs on the control thread (60Hz):

```
// Ring buffer of last 16 received pitches
pitch_history.push(current_pitch);

// Novelty = unique pitches / total in buffer
let unique = pitch_history.iter().collect::<HashSet<_>>().len();
let novelty = unique as f32 / pitch_history.len() as f32;

// Valence from novelty
emotion.valence = (novelty - 0.5) * 2.0;  // high novelty → positive valence

// Pattern mutation when bored
if novelty < mutate_threshold {
    shared_handles["cell0.pattern"].set(rng.gen_range(0.0..5.0));
}
```

## Infrastructure Consumption

| Infra Port | OrganismModule.receive_signal() | Action |
|------------|-------------------------------|--------|
| `quantizer.pitch_hz` | `shared_handles["cell1.freq"].set(v)` | Base pitch |
| `quantizer.nearest_degree` | Update arp chord root | Scale-locked arpeggiation |
| `keyboard.note_on` | `cmd_tx.try_send(NoteOn)` | Restart arp from step 0 |
| `keyboard.note_off` | `cmd_tx.try_send(NoteOff)` | Begin arp fadeout |
| `cursor.x` [0,1] | `shared_handles["cell0.rate_hz"].set(mapped)` | Arp speed |
| `cursor.y` [0,1] | `shared_handles["cell1.cutoff"].set(mapped)` | Filter depth |
| `audio_analysis.rms` | Update env_follow input | Side-chain ducking |

## Organism Outputs

| Output | Type | Source |
|--------|------|--------|
| `arp_pitch` [20,20000] | Block | Current arp step frequency |
| `arp_gate` | Trigger Event | Gate trigger on each arp step |
| `filter_position` [0,1] | Block | Normalized filter sweep position |
| `pattern_entropy` [0,1] | Block | Current pattern unpredictability |

## Social Dynamics

```
MELO → TBLK:  strong (MELO chases transient-rich novelty from hits)
MELO → DRON:  strong initially, decays (harmonic field is novel at first, then static)
TBLK → MELO:  weak (TBLK is antisocial)
DRON → MELO:  medium (DRON appreciates melodic company)
```

### Emergent behavior: timbral sympathy

MELO's filter tracking follows DRON's harmonic_field. When DRON shifts harmonics,
MELO's filter sweeps to match — creating a cycle: decay → DRON shifts → MELO follows
→ edge strengthens → decay again.

### Emergent behavior: polyrhythmic sync

MELO's arp_gate output might sync with TBLK's hit_trigger, creating accidental
polyrhythms. If the combined pattern pleases both (positive valence), the sync
edge strengthens — organisms accidentally lock into groove.

## DNA Parameters

```
arp_rate_hz:            4.0         // steps per second
arp_pattern:            0           // 0=up, 1=down, 2=up-down, 3=random, 4=converge
arp_octaves:            2           // octave spread
gate_length:            0.6         // proportion of step (0=staccato, 1=legato)
swing:                  0.0         // even step timing offset [0, 0.5]
pulse_width:            0.5         // PWM base (0.5 = square)
pwm_depth:              0.3         // LFO modulation of pulse width
filter_base_cutoff:     1200.0      // Hz
filter_env_depth:       3000.0      // Hz added by envelope
attack_ms:              2.0
decay_ms:               80.0
sustain:                0.4
release_ms:             50.0
vibrato_rate:           5.5         // Hz
vibrato_depth:          10.0        // cents
novelty_window:         16          // ring buffer size for pitch history
pattern_mutate_threshold: 0.3       // novelty score below this → mutate
```

## Verification Criteria

- [ ] MELO produces arpeggiated patterns synced to quantizer pitch
- [ ] Arp patterns play in correct modes (up, down, up-down, random, converge)
- [ ] Cursor X controls arp rate, cursor Y controls filter envelope depth
- [ ] Note-on restarts arp from step 0, note-off triggers fadeout
- [ ] Filter sweeps audibly on each arp step (envelope → cutoff)
- [ ] PWM modulation audible via LFO from ModMatrix
- [ ] Novelty tracking: valence responds to pitch variety
- [ ] Pattern mutation occurs when novelty drops below threshold
- [ ] SharedHandles respond to infrastructure signals in real-time
- [ ] Blob renders as darting, small, green/yellow
- [ ] `arp_gate` output provides trigger for other organisms to sync
