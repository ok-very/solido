# S12 — Cell Composition + Unified DNA

**Layer**: L4 (Cells) + DNA
**Depends on**: S11 (atom + molecule primitives) ✅
**Status**: Partial — infrastructure implemented, cell inventory superseded

## Implementation Status

### What was implemented
- DspCell trait (`src/dsp/cell/mod.rs`) — `tick()`, `handle_command()`, `analysis()`, `output_channels()`, `reset()`, `name()`
- CellRegistry with factory pattern + param ranges (`src/dsp/cell/mod.rs`)
- OrganismDna unified struct with audio + visual + physics + social (`src/organism/dna.rs`)
- DNA I/O: save/load JSON (`src/organism/dna_io.rs`)
- DNA mutation operators (`src/organism/mutation.rs`)
- OrganismDsp: `from_dna()`, `tick()`, `handle_command()` with wire dispatch (`src/dsp/organism_dsp.rs`)
- 1 cell: `drone_bed` — monolithic FunDSP-based drone

### What was skipped
- S11 atom/molecule layer (17 atoms, 9 molecules) — **skipped entirely**. Cells talk directly to FunDSP `AudioUnit` graphs. The atom/molecule abstraction added complexity without benefit when FunDSP's operator chaining already composes primitives.
- GraphDna structs in `dna.rs` — vestigial HexoDSP code, always `None`, candidates for removal.

### Cell inventory: superseded by S20–S25

The 7 cells below were designed for TBLK/DRON/MELO organisms that were later replaced by 6 organisms (DRON/HOSO/SPGL/ACID/TBLK/KKIT) with a granular composable cell architecture. **None of the 7 cells below were built.**

| S12 Cell | Superseded by | Session |
|----------|---------------|---------|
| HarmonicBed | osc_cell + filter_cell + lfo_cell | S20 |
| ShimmerLayer | osc_cell + filter_cell + func_gen_cell | S20 + S22 |
| StrikeVoice | strike_voice_cell | S24 |
| PatternGen | seq_cell + logic_seq_cell | S21 + S22 |
| Arpeggiator | seq_cell | S21 |
| TimbreVoice | osc_cell + env_cell + filter_cell | S20 + S21 |
| ModMatrix | lfo_cell + func_gen_cell | S20 + S22 |

See [S20](S20-granular-cells.md) through [S25](S25-kkit-909.md) for the replacement cell inventory (17 granular cells).

---

## Goal (original)

Build the cell layer that composes S11 molecules into functional units, and the
unified DNA system that defines organism blueprints for both audio AND visual identity.
Cells are the bridge between raw DSP (S11) and full organisms (S13).

## What a Cell Is

A cell is a **self-contained DSP unit with a parameter interface**. It owns one or
more molecules, wires them together, and exposes named parameters that the organism
layer (S13) can control.

Cells run on the audio thread (like their constituent molecules). The cell's parameter
interface maps high-level musical concepts to low-level atom parameters:

```
Organism (control thread)          Cell (audio thread)
  "play note at 440Hz"      →       NoteOn { freq: 440, vel: 0.8 }
  "set brightness to 0.7"   →       Shared cutoff handle .set(0.7 * range + base)
  "trigger hit"              →       retrigger envelopes, reset phase
```

### Parameter Control Model

Cells use two communication channels, matching S11's implementation:

- **Continuous params**: `Shared` handles (lock-free atomics). Both the control
  thread (OrganismModule) and audio thread (Cell) hold clones of the same `Shared`.
  Control thread calls `.set(v)`, audio graph reads via `var(&shared)`. Zero latency.
- **Discrete events**: `DspCommand` via ring buffer. NoteOn, NoteOff, Reset, Panic.

No `SetParam` command is needed — all continuous parameters use Shared directly.

## DspCell Trait

```rust
/// A functional DSP unit with parameter interface. Runs on audio thread.
/// Owns molecules, exposes musical parameters via Shared handles.
pub trait DspCell: Send {
    /// Process one sample. Cells call their molecules' tick() internally.
    /// Output is interleaved stereo (2 channels) or mono (1 channel).
    fn tick(&mut self, output: &mut [f32]);

    /// Handle a discrete command from the control thread.
    fn handle_command(&mut self, cmd: &DspCommand);

    /// Return current analysis (RMS, peak) for the control thread.
    fn analysis(&self) -> DspAnalysis;

    /// Number of output channels (1 for mono cells, 2 for stereo).
    fn output_channels(&self) -> usize;

    /// Reset all internal state (molecules, envelopes, accumulators).
    fn reset(&mut self);

    /// Cell type name (matches CellDna.cell_type).
    fn name(&self) -> &str;
}
```

Note: `tick()` is per-sample (not block-based). This matches S11's Molecule::tick()
directly. If block processing is needed for cache efficiency, the OrganismDsp
container can batch tick calls.

## Cell Inventory (7 cells)

### TBLK Cells

**StrikeVoice** — Single percussive hit. Owns membrane_sim + snap_transient +
body_resonance molecules from S11.

```
Shared params:
  membrane_freq:  [60, 400] Hz    — drum body pitch
  bandwidth:      [20, 100] Hz    — membrane resonance width
  click_mix:      [0, 1]          — transient click vs body blend
  body_feedback:  [0, 0.9]        — comb resonance amount

Commands: NoteOn (trigger hit + set velocity), NoteOff (unused — percussion self-decays)
Output: mono (1ch) — panned to stereo at organism level
```

**PatternGen** — Rhythmic sequencer. Owns ClockAtom + euclidean logic.
Not audio DSP — produces internal trigger events that fire StrikeVoice.

```
Shared params:
  bpm:            [40, 240]       — tempo
  steps:          [3, 16]         — pattern length
  hits:           [1, steps]      — filled steps (euclidean)
  accent_depth:   [0, 1]          — velocity difference accented/ghost
  swing:          [0, 0.5]        — even-step timing offset

Output: internal trigger events (not audio) — dispatched to StrikeVoice
```

### DRON Cells

**HarmonicBed** — Continuous drone voice. Owns detuned_stack + slow_filter +
stereo_spread molecules.

```
Shared params:
  root_hz:         [30, 500]       — base pitch
  detune_cents:    [2, 15]         — detuning spread (recalculates f1/f2/f3)
  cutoff:          [100, 8000] Hz  — filter cutoff
  resonance:       [0.1, 2.0]     — filter Q
  pan_rate:        [0.01, 0.1] Hz — stereo movement speed

Commands: none (always playing)
Output: stereo (2ch)
```

**ShimmerLayer** — Upper harmonic wash. Owns SineAtom (octave up) + AllpassAtom
chain + DelayAtom (feedback). New molecule needed: `shimmer_wash`.

```
Shared params:
  shimmer_amount: [0, 1]          — blend into output
  diffusion:      [0, 1]          — allpass chain feedback
  feedback:       [0, 0.8]        — reverb wash decay

Commands: none (always playing)
Output: stereo (2ch)
```

### MELO Cells

**Arpeggiator** — Pattern-driven pitch sequencer. Owns ClockAtom + pattern logic.
Produces pitch/gate events that drive TimbreVoice.

```
Shared params:
  rate_hz:        [1, 16]         — arp steps per second
  pattern:        enum(Up, Down, UpDown, Random, Converge)
  octaves:        [1, 3]          — octave spread
  gate_length:    [0.1, 1.0]      — staccato ↔ legato
  swing:          [0, 0.5]        — timing offset

Output: internal pitch + gate events (dispatched to TimbreVoice)
```

**TimbreVoice** — Synth voice per arp step. Owns osc_pair + filter_envelope +
amp_envelope molecules.

```
Shared params:
  freq:           [20, 20000] Hz  — set per step from arpeggiator
  pulse_width:    [0.1, 0.9]      — PWM (via osc_pair freq_sub ratio)
  filter_base:    [100, 8000] Hz  — filter envelope base cutoff
  filter_depth:   [0, 8000] Hz    — envelope modulation range
  filter_q:       [0.1, 2.0]      — resonance
  attack_ms:      [1, 50]
  decay_ms:       [10, 500]
  sustain:        [0, 1]
  release_ms:     [10, 500]

Commands: NoteOn { freq, velocity }, NoteOff
Output: mono (1ch) — panned to stereo at organism level
```

**ModMatrix** — Modulation routing. Owns LfoAtom bank + EnvFollowAtom.
Routes modulation signals to TimbreVoice parameters via Shared handles.

```
Shared params:
  pwm_rate:        [0.5, 8] Hz     — pulse width LFO
  pwm_depth:       [0, 0.4]        — modulation amount
  filter_lfo_rate: [0.1, 4] Hz     — filter cutoff wobble
  vibrato_rate:    [4, 8] Hz       — pitch vibrato
  vibrato_depth:   [0, 30] cents   — vibrato range

Output: modulation values applied to TimbreVoice Shared handles each tick
```

## Unified Organism DNA

DNA defines an organism's complete identity — audio, visual, physical, and social.
It merges the audio/cell structure (this spec) with the visual/physics identity
(S09 organism sim). One struct, serialized as one JSON document.

### Rust Types

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganismDna {
    // --- Identity ---
    pub name: String,                    // "tblk-alpha"
    pub species: String,                 // "tblk", "dron", "melo"
    pub seed: u64,                       // deterministic RNG seed
    pub version: u32,                    // schema version for forward compat

    // --- Audio / DSP (S12) ---
    pub cells: Vec<CellDna>,
    pub cell_wiring: Vec<CellWire>,

    // --- Visual Body (S09) ---
    pub body: BodyDna,

    // --- Rendering (S09) ---
    pub render: RenderDna,

    // --- Physics (S09) ---
    pub physics: PhysicsDna,

    // --- Emotion / Social ---
    pub emotion: EmotionDna,
    pub affinity_tags: Vec<String>,      // ["aggressive", "percussive"]
    pub affinity_biases: Vec<AffinityBias>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellDna {
    pub cell_type: String,               // "strike_voice", "harmonic_bed", etc.
    pub params: HashMap<String, f32>,    // named params with initial values
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellWire {
    pub src_cell: usize,
    pub dst_cell: usize,
    pub wire_type: WireType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WireType {
    Audio,                               // audio output of src → dst input
    Trigger,                             // trigger/gate events
    Modulation { target_param: String }, // modulation values → dst param
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyDna {
    pub lobe_count: u8,                  // 1–12, default 6
    pub core_radius: f32,                // base blob size
    pub pseudopod_gain: f32,             // how far pseudopods extend
    pub extension_speed: f32,            // pseudopod extend rate
    pub retraction_speed: f32,           // pseudopod retract rate
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderDna {
    pub smin_k: f32,                     // intra-organism lobe blend softness
    pub edge_softness: f32,              // SDF AA band width
    pub glow: f32,                       // halo intensity
    pub hue: f32,                        // base hue [0,1]
    pub thermal_enabled: bool,
    pub palette_variant: u8,
    pub pulse_response: f32,             // beat-sync breathing amplitude
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsDna {
    pub mass: f32,
    pub drag: f32,
    pub max_speed: f32,
    pub interaction_rules: Vec<InteractionRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionRule {
    pub with_species: String,            // species_tag or "*" wildcard
    pub mode: InteractionMode,
    pub range: f32,
    pub strength: f32,
    pub dwell_secs: Option<f32>,         // for IntegratePropose
    pub rest_length: Option<f32>,        // for Attach
    pub break_force: Option<f32>,
    pub break_distance: Option<f32>,
    pub affinity_threshold: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionMode {
    Repel,
    Bounce,
    Slow,
    Attach,
    Glob,
    IntegratePropose,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmotionDna {
    pub base_valence: f32,               // [-1, 1]
    pub base_arousal: f32,               // [0, 1]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AffinityBias {
    pub port_name: String,               // infrastructure port this organism prefers
    pub bias: f32,                       // initial edge weight bias [0, 1]
}
```

### Example DNA (TBLK)

```json
{
  "name": "tblk-alpha",
  "species": "tblk",
  "seed": 183774,
  "version": 1,

  "cells": [
    {
      "cell_type": "pattern_gen",
      "params": { "bpm": 120.0, "steps": 7.0, "hits": 5.0, "accent_depth": 0.6 }
    },
    {
      "cell_type": "strike_voice",
      "params": { "membrane_freq": 180.0, "bandwidth": 60.0, "click_mix": 0.3, "body_feedback": 0.4 }
    }
  ],
  "cell_wiring": [
    { "src_cell": 0, "dst_cell": 1, "wire_type": "Trigger" }
  ],

  "body": {
    "lobe_count": 6,
    "core_radius": 18.0,
    "pseudopod_gain": 0.9,
    "extension_speed": 6.0,
    "retraction_speed": 8.0
  },

  "render": {
    "smin_k": 0.25,
    "edge_softness": 2.0,
    "glow": 0.4,
    "hue": 0.05,
    "thermal_enabled": true,
    "palette_variant": 0,
    "pulse_response": 0.6
  },

  "physics": {
    "mass": 0.8,
    "drag": 0.9,
    "max_speed": 150.0,
    "interaction_rules": [
      { "with_species": "*", "mode": "Repel", "range": 25.0, "strength": 1.0 }
    ]
  },

  "emotion": {
    "base_valence": -0.2,
    "base_arousal": 0.7
  },

  "affinity_tags": ["aggressive", "percussive"],
  "affinity_biases": [
    { "port_name": "note_on", "bias": 0.8 },
    { "port_name": "rms", "bias": 0.3 }
  ]
}
```

### DNA Operations

| Operation | Description |
|-----------|-------------|
| `save(path)` | Serialize to JSON |
| `load(path)` | Deserialize from JSON |
| `clone()` | Exact copy (Rust derive) |
| `mutate(rng, rate)` | Perturb numeric params within ranges |
| `crossover(other, rng)` | Swap cells between two organisms (future) |

### Mutation

`mutate(rng, rate)` iterates all numeric params in all CellDna entries and with
probability `rate` perturbs each by ±10% of its range. BodyDna, RenderDna, and
PhysicsDna params are also mutated. Non-numeric params (enums like arp pattern)
mutate by random selection from valid values.

## OrganismDsp — Audio Thread Container

The audio-thread counterpart to a control-thread OrganismModule. Owns all cells,
processes their audio, mixes to stereo output.

```rust
pub struct OrganismDsp {
    cells: Vec<Box<dyn DspCell>>,
    cell_wiring: Vec<CellWire>,
    /// Per-cell scratch output buffers for inter-cell routing.
    scratch: Vec<Vec<f32>>,
    /// Mixed stereo output.
    output: [f32; 2],
    sample_rate: f32,
}

impl OrganismDsp {
    /// Build from DNA blueprint. Returns (OrganismDsp, SharedHandles).
    /// SharedHandles are cloned Shared references for the control thread.
    pub fn from_dna(dna: &OrganismDna, sr: f32) -> (Self, SharedHandles);

    /// Process one sample. Cells tick in wiring order.
    pub fn tick(&mut self, output: &mut [f32]);

    /// Dispatch a command to the appropriate cell(s).
    pub fn handle_command(&mut self, cmd: DspCommand);

    /// Collect analysis from all cells.
    pub fn analysis(&self) -> DspAnalysis;
}
```

`from_dna()` returns both the DSP container and a set of `Shared` handle clones.
The control thread (OrganismModule) holds these handles and calls `.set(v)` directly
for continuous param updates — no ring buffer needed for float params.

## CellRegistry — Factory Pattern

Maps cell type strings from DNA to constructor functions:

```rust
pub struct CellRegistry {
    factories: HashMap<String, CellFactory>,
}

type CellFactory = Box<dyn Fn(&CellDna, f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)>>;

impl CellRegistry {
    pub fn new() -> Self;      // registers all known cell types
    pub fn build(&self, dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)>;
}
```

Each factory returns the cell AND its Shared handles, so OrganismDsp::from_dna()
can collect all handles for the control thread.

## File Structure

```
src/dsp/                          (from S11 ✅)
├── cell/
│   ├── mod.rs                    — DspCell trait, CellRegistry
│   ├── strike_voice.rs           — TBLK percussion
│   ├── pattern_gen.rs            — TBLK rhythm sequencer
│   ├── harmonic_bed.rs           — DRON drone voice
│   ├── shimmer_layer.rs          — DRON upper harmonics
│   ├── arpeggiator.rs            — MELO sequencer
│   ├── timbre_voice.rs           — MELO synth voice
│   └── mod_matrix.rs             — MELO modulation routing
├── organism_dsp.rs               — OrganismDsp audio-thread container

src/organism/
├── mod.rs                        — pub mod dna (+ future: sim, interaction)
├── dna.rs                        — OrganismDna, CellDna, BodyDna, RenderDna, etc.
├── dna_io.rs                     — save/load JSON
└── mutation.rs                   — mutate, crossover

assets/dna/
├── tblk-alpha.json
├── dron-alpha.json
└── melo-alpha.json
```

## New Dependencies

| Crate | Purpose |
|-------|---------|
| `serde` | Derive Serialize/Deserialize for DNA |
| `serde_json` | JSON serialization |

## Implementation Steps

### Step 1: DNA types + serde (`src/organism/dna.rs`)
OrganismDna, CellDna, BodyDna, RenderDna, PhysicsDna, EmotionDna, InteractionRule,
CellWire, WireType, AffinityBias. Derive Serialize/Deserialize.

### Step 2: DNA I/O (`src/organism/dna_io.rs`)
`save(dna, path)` and `load(path) -> Result<OrganismDna>`. JSON format.

### Step 3: DNA mutation (`src/organism/mutation.rs`)
`mutate(dna, rng, rate)` — perturb numeric params within ranges.

### Step 4: DspCell trait + CellRegistry (`src/dsp/cell/mod.rs`)
Define the trait. CellRegistry with factory functions.

### Step 5: StrikeVoice cell (`src/dsp/cell/strike_voice.rs`)
Compose membrane_sim + snap_transient + body_resonance. NoteOn triggers all three,
mixes output. Returns Shared handles for membrane_freq, bandwidth, click_mix, etc.

### Step 6: PatternGen cell (`src/dsp/cell/pattern_gen.rs`)
Euclidean pattern generation using ClockAtom. Produces internal trigger events.

### Step 7: HarmonicBed + ShimmerLayer cells
HarmonicBed: detuned_stack + slow_filter + stereo_spread. Continuous processing.
ShimmerLayer: new shimmer_wash molecule (SineAtom + AllpassAtom chain + feedback delay).

### Step 8: Arpeggiator + TimbreVoice + ModMatrix cells
Arpeggiator drives TimbreVoice with pitch/gate. ModMatrix provides LFO modulation
via Shared handles.

### Step 9: OrganismDsp (`src/dsp/organism_dsp.rs`)
Build from DNA via CellRegistry. Process cells in wiring order. Mix to stereo.
Return SharedHandles alongside the DSP container.

### Step 10: DNA presets
Write tblk-alpha.json, dron-alpha.json, melo-alpha.json to assets/dna/.

### Step 11: Integration test
Load tblk-alpha DNA → build OrganismDsp → send NoteOn → verify percussive transient.
Same for DRON (continuous) and MELO (arpeggiated).

## Verification

1. `cargo test` — all cell + DNA tests pass
2. DNA round-trip: save → load → save produces identical JSON
3. Mutation changes params within valid ranges, never produces NaN/inf
4. CellRegistry builds all 7 cell types from DNA
5. OrganismDsp from tblk DNA: NoteOn produces percussive transient
6. OrganismDsp from dron DNA: continuous audio output without commands
7. OrganismDsp from melo DNA: arp pattern produces pitched note sequence
8. OrganismDsp.tick() is allocation-free (no heap allocs in hot path)
9. SharedHandles allow control thread to modulate params lock-free
10. DspCommand fits in 16 bytes (cache-friendly ring buffer transfer)
