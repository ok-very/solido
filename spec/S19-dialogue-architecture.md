# S19 — Dialogue Architecture + SequencerModule

**Layer**: L0 + L1
**Depends on**: S13 (first organisms), S18 (param bridge)
**Status**: Spec

## Goal

Establish the **dialogue pattern** — keyboard and sequencer are prompts; organisms interpret through personality. This session builds the SequencerModule (infrastructure), expands OrganismModule with gate/accent/response ports, and adds per-organism `fidelity` DNA so each species controls how closely it follows external input.

## The Dialogue Model

Human input is a **prompt**. Organism response is an **interpretation**. The affinity graph mediates fidelity.

```
PROMPT LAYER (infrastructure):
  KeyboardModule ──→ pitch_hz, gate, trigger     ──→ AffinityGraph
  SequencerModule ──→ step_pitch, step_gate, accent ──→ AffinityGraph

INTERPRETATION LAYER (organism):
  OrganismModule receives signals with affinity-weighted strength
  │
  ├─ Internal intent: organism's own pattern/rhythm tendency
  │   └─ seq_cell, logic_seq_cell, func_gen_cell generate internal patterns
  │
  ├─ Personality blend: emotion state + fidelity determine response
  │   └─ High valence + strong edge → faithful follower
  │   └─ Low valence or weak edge → follows own pattern, ignores prompt
  │   └─ High arousal → exaggerates/transforms the prompt
  │
  └─ Response: organism emits what it actually played
      └─ actual_pitch, rhythm_density → back into graph
      └─ Other organisms can learn from these emissions
      └─ UI shows divergence: prompted vs actual

CROSS-ORGANISM DIALOGUE:
  ACID emits its rhythm → TBLK picks it up → TBLK syncs or counterpunches
  DRON emits its harmonic field → HOSO's filter tracks it
  SPGL ignores everyone → slowly pulls others toward its pitch center
```

### Personality Blend Formula

```
actual_behavior = lerp(internal_pattern, external_prompt, affinity_weight * fidelity)
```

Where:
- `internal_pattern` = output of organism's own seq_cell / logic_seq_cell / func_gen_cell
- `external_prompt` = signal received from KeyboardModule or SequencerModule
- `affinity_weight` = learned edge weight from AffinityGraph (0..1)
- `fidelity` = DNA param per organism (ACID=0.8, SPGL=0.1, DRON=0.3, etc.)

---

## SequencerModule (New Infrastructure Module)

**File**: `src/modules/sequencer.rs`
**Tier**: Infrastructure
**Purpose**: Human writes a 16-step pattern; module emits pitch/gate/accent/beat_phase per step.

### Schema

```rust
ModuleSchema {
    name: "Sequencer",
    category: ModuleCategory::Input,
    tier: ModuleTier::Infrastructure,
    inputs: [],
    outputs: [
        ("step_pitch", SignalType::Float),    // Hz of current step
        ("step_gate", SignalType::Trigger),    // fires on gate-on steps
        ("step_accent", SignalType::Float),    // 0.0 or accent level
        ("beat_phase", SignalType::Float),     // 0.0–1.0 ramp per beat
        ("step_index", SignalType::Float),     // 0–15 current step
    ],
}
```

### Internal State

```rust
pub struct SequencerModule {
    // Pattern data (16 steps)
    steps: [StepData; 16],

    // Transport
    bpm: Shared,                // writeable from UI
    playing: Shared,            // 1.0 = running, 0.0 = stopped
    step_count: u8,             // 1–16 active steps
    swing: Shared,              // 0.0–1.0 swing amount

    // Runtime
    phase: f64,                 // accumulates at sample rate
    current_step: usize,
    gate_remaining: f32,        // samples remaining in current gate

    // Ports
    ports: Vec<PortId>,
}

pub struct StepData {
    pitch_hz: Shared,           // frequency for this step
    gate: Shared,               // 1.0 = on, 0.0 = off (rest)
    accent: Shared,             // 0.0 = normal, 1.0 = accent
    slide: Shared,              // 1.0 = slide to next step's pitch
}
```

### Tick Behavior

Each `tick(dt)`:
1. Accumulate `phase += bpm / 60.0 * dt`
2. On step boundary: read `steps[current_step]`, emit `step_pitch` + `step_gate` + `step_accent`
3. Between steps: emit `beat_phase` as 0.0–1.0 ramp
4. Swing offsets even steps by `swing * half_step_duration`

### UI: Bidirectional Step Grid

The step grid is the primary dialogue display:

```
┌─────────────────────────────────────────────────────────────┐
│  SEQUENCER  ▶ 120 BPM  [16 steps]                          │
├─────────────────────────────────────────────────────────────┤
│  Human:  [C4][D4][ ][E4][C4][D4][ ][G3][C4][D4][ ][E4]... │  ← editable
│  ACID:   [C4][D4][ ][E4][C4][D4][ ][G3][C4][D4][ ][E4]... │  ← read-only, green
│  DRON:   [C#][D ][ ][Eb][C#][D ][ ][G ][C#][D ][ ][Eb]... │  ← read-only, blue (drifted)
│  SPGL:   [B3][B3][ ][B3][B3][B3][ ][B3][B3][B3][ ][B3]... │  ← read-only, violet (ignoring)
├─────────────────────────────────────────────────────────────┤
│  ▲ step 5                                                    │
└─────────────────────────────────────────────────────────────┘
```

Each organism row shows what it **actually played** at each step (from `actual_pitch` emissions). Color-coded by species hue. Human pattern on top is editable via click.

**File**: `src/ui/sequencer_grid.rs` (new egui panel)

---

## OrganismModule Expanded Ports

### New Input Ports

| Port | Type | Source | Purpose |
|------|------|--------|---------|
| `gate` | Trigger | Keyboard or Sequencer | Triggers NoteOn/NoteOff in organism's DSP |
| `accent` | Float | Sequencer | Modulates velocity / filter intensity |

### New Output Ports

| Port | Type | Purpose |
|------|------|---------|
| `actual_pitch` | Float | What the organism actually played (Hz) |
| `rhythm_density` | Float | Activity level (triggers per second) |

### Expanded `receive_signal()`

```rust
fn receive_signal(&mut self, port: PortId, signal: &Signal) {
    match self.port_name(port) {
        "pitch_hz" => {
            let hz = signal.as_float();
            // Apply species personality transform
            let actual = self.personality_transform_pitch(hz);
            self.shared_handles["cell0.root_hz"].set(actual);
            self.last_actual_pitch = actual;
        }
        "gate" => {
            if signal.is_trigger() {
                self.cmd_tx.try_send(DspCommand::NoteOn {
                    freq: self.last_actual_pitch,
                    velocity: self.accent_level,
                });
            }
        }
        "accent" => {
            self.accent_level = signal.as_float();
        }
        _ => { /* existing handling */ }
    }
}
```

### Species-Specific Pitch Personality

```rust
fn personality_transform_pitch(&mut self, prompted_hz: f32) -> f32 {
    let blend = self.dna.fidelity * self.affinity_weight_for_source();
    let internal_hz = self.internal_pitch_intent(); // from internal cells

    match self.dna.species.as_str() {
        "dron" => {
            // Slew toward prompted pitch, never jump
            self.pitch_slew.target = lerp(internal_hz, prompted_hz, blend);
            self.pitch_slew.tick() // returns smoothed value
        }
        "acid" => {
            // Follow tightly but can transpose octaves based on arousal
            let base = lerp(internal_hz, prompted_hz, blend);
            if self.emotion.arousal > 0.7 { base * 2.0 } else { base }
        }
        "hoso" => {
            // Rigid follower — direct passthrough
            lerp(internal_hz, prompted_hz, blend)
        }
        "spgl" => {
            // Barely acknowledges — long averaging
            self.pitch_accumulator.push(prompted_hz);
            self.pitch_accumulator.average() // drift over minutes
        }
        "tblk" => {
            // Quantize to nearest resonant membrane mode
            self.quantize_to_membrane_mode(lerp(internal_hz, prompted_hz, blend))
        }
        "kkit" => {
            // Ignore pitch entirely
            internal_hz
        }
        _ => prompted_hz,
    }
}
```

---

## New DNA Field: `fidelity`

Added to `OrganismDna`:

```rust
pub struct OrganismDna {
    // ... existing fields ...
    pub fidelity: f32,  // 0.0–1.0: how closely organism follows external prompts
}
```

| Species | Fidelity | Meaning |
|---------|----------|---------|
| DRON | 0.3 | Slowly absorbs, mostly self-driven |
| HOSO | 0.9 | Rigid follower |
| SPGL | 0.1 | Mostly ignores external input |
| ACID | 0.8 | Follows sequencer closely, adds interpretation |
| TBLK | 0.5 | Follows rhythm, quantizes pitch |
| KKIT | 0.95 | Mechanical gate/accent follower |

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/modules/sequencer.rs` | Create — SequencerModule |
| `src/organism/module.rs` | Modify — add gate/accent inputs, actual_pitch/rhythm_density outputs |
| `src/organism/module.rs` | Modify — personality_transform_pitch() |
| `src/dsp/dna.rs` | Modify — add `fidelity: f32` to OrganismDna |
| `src/ui/sequencer_grid.rs` | Create — bidirectional step grid panel |
| `src/ui/mod.rs` | Modify — register sequencer grid |

---

## Test Plan (~15 tests)

### SequencerModule
- `seq_emits_step_pitch`: advancing 1 step emits correct pitch_hz
- `seq_emits_gate_on_active_steps`: gate fires only on non-rest steps
- `seq_emits_accent`: accent value matches step data
- `seq_beat_phase_ramps`: beat_phase goes 0.0→1.0 between steps
- `seq_swing_offsets_even_steps`: even steps delayed by swing amount
- `seq_respects_step_count`: only cycles through active step count
- `seq_stop_silences`: playing=0.0 stops all emissions

### OrganismModule Dialogue
- `gate_triggers_noteon`: receiving gate Trigger sends DspCommand::NoteOn
- `accent_modulates_velocity`: accent Float affects NoteOn velocity
- `emits_actual_pitch`: organism reports what it actually played
- `emits_rhythm_density`: organism reports activity level

### Personality Transforms
- `dron_slews_pitch`: DRON pitch changes slowly toward target
- `acid_follows_tightly`: ACID pitch tracks sequencer closely
- `spgl_ignores_prompt`: SPGL pitch barely changes from external input
- `kkit_ignores_pitch`: KKIT pitch unchanged regardless of input

---

## Verification Criteria

- [x] SequencerModule registered as Infrastructure, emits all 5 signal types
- [x] OrganismModule receives gate/accent, sends DspCommand to audio thread
- [x] OrganismModule emits actual_pitch reflecting personality transform
- [x] Fidelity DNA param serialized/deserialized in JSON
- [ ] **DEFERRED**: Step grid UI shows human pattern + per-organism response overlay (moved to S26 UX integration)
- [x] DRON slews, ACID follows, SPGL ignores — personality transform implemented (audio testing pending cells)
- [x] `cargo test` — 27 tests pass (12 sequencer + 15 organism)

## Implementation Status

**Core functionality: COMPLETE** ✅
- SequencerModule: 16-step pattern sequencer with 12 passing tests
- OrganismModule dialogue ports: gate/accent inputs, actual_pitch/rhythm_density outputs
- Personality transforms: Species-specific fidelity-based pitch interpretation
- DNA fidelity field: Serialization working, dron-alpha.json updated

**Deferred to S26 (Six-organism integration):**
- `src/ui/sequencer_grid.rs` — Bidirectional step grid UI panel
- Visual feedback of organism responses in step grid

The step grid UI is purely visual and doesn't block downstream sessions. Organisms can receive sequencer signals and respond with personality now. The UI will be built once all six organisms exist (S26).
