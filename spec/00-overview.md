# Solido 0.6 — Module-First Audiovisual Synthesis Engine

> From MAKE A BABY to the Hosono Test: raga-based generative
> synthesis through a Hebbian affinity graph, where infrastructure
> serves organisms and organisms are the blobs.

## Lineage

```
MAKE A BABY (Max/MSP, ~2006)
  cycle~ voices + ragaraga coll + delay feedback + FFT spectral filters
  mods_simple_bow gamaka engine + 6-channel spatial output
  └─ Design DNA: scale-driven pitch, bow modulation, feedback networks

roadmap.md + addendum
  Perception → LLM → Seed Reactor → Audio Engine → Visual Renderer → Feedback
  └─ Three timescales: audio-rate (44.1kHz), block-rate (~689Hz), LLM-rate (~2-10Hz)
      └─ cpal, candle Phi-2, wgpu

microtonal_gravity_implementation_plan.md + addendum
  Scala parser → TuningSystem → PitchGravity → TalaGrid → QuantizerModule
  └─ "Not a scale. A gravitational field."
      └─ Emotion drives gravity strength → texture ↔ music continuum

blob_affinity_implementation_plan.md
  Typed Contract Core → Affinity Graph → Blob Renderer → UX Shell
  └─ Hebbian learning, homeostatic emotion, ledger, thermal shader

can we add an emotive color system...
  SDF depth → thermal palette (black→indigo→blue→cyan→green→yellow→orange→white)
  └─ Domain warp, fresnel glow, arousal-driven temperature

solido 0.5 (Rust + eframe + wgpu)
  SDF organism renderer + egui sliders + MSDF text + recorder
  └─ The visual foundation we refactor into blobs
```

## Architecture: Two-Tier Design

```
┌──────────────────────────────────────────────────────────────┐
│  LAYER 6 — UX SHELL                                          │
│  egui panels, inspectors, DNA editor, presets, ledger view   │
├──────────────────────────────────────────────────────────────┤
│  LAYER 5 — ORGANISMS                                         │
│  Compound instruments built from cells, defined by DNA,      │
│  routed through AffinityGraph with Hebbian learning          │
├──────────────────────────────────────────────────────────────┤
│  LAYER 4 — CELLS                                             │
│  Functional units: synthesis voice, pattern generator,        │
│  spectral processor — composed from molecules                │
├──────────────────────────────────────────────────────────────┤
│  LAYER 3 — MOLECULES                                         │
│  Small atom combinations: filtered oscillator,               │
│  envelope follower, pitch tracker                            │
├──────────────────────────────────────────────────────────────┤
│  LAYER 2 — ATOMS                                             │
│  Primitive behaviors: oscillate, filter, gate, envelope,     │
│  delay, sample-and-hold                                      │
├──────────────────────────────────────────────────────────────┤
│  LAYER 1 — INFRASTRUCTURE                                    │
│  Input modules (keyboard, cursor, camera, audio analysis),   │
│  processing (quantizer, pitch/rhythm gravity, raga/tala),    │
│  master bus (mix + dynamics), InfrastructureRouter            │
├──────────────────────────────────────────────────────────────┤
│  LAYER 0 — MODULE CONTRACT + SUBSTRATE                       │
│  ModuleCore trait, Signal types, PortId, cpal audio,         │
│  ringbuf channels, AffinityGraph, InfrastructureRouter       │
└──────────────────────────────────────────────────────────────┘
```

### Infrastructure Tier (L1) — Deterministic Substrate

Infrastructure modules are the studio hardware that organisms play on:

- **Modules**: keyboard input, cursor input, audio analysis, quantizer, voice DSP, camera, etc.
- **Routing**: Fixed via `InfrastructureRouter` — edges auto-discovered by type/range/rate compatibility, then frozen
- **No emotions**, no Hebbian learning, no exploration, no pruning
- **Schema tier**: `ModuleTier::Infrastructure`
- Think: pickups, strings, frets, amplifier, mixer

### Organism Tier (L2–L5) — Creative Entities That Learn

Organisms are compound instruments that learn and evolve:

- **Composed from** atoms → molecules → cells → organisms (see hierarchy below)
- **Routing**: Through `AffinityGraph` with full Hebbian learning
- **Emotions**: valence (homeostatic satisfaction) + arousal (surprise/deviation)
- **Explore** new connections, **strengthen** productive ones, **prune** weak ones
- **Visualized as blobs** (SDF renderer) with thermal-palette coloring
- **Defined by DNA** — can be saved, cloned, evolved
- **Consume infrastructure outputs** as signal sources

### Cross-Tier Signal Flow

```
Infrastructure (fixed routing):
  keyboard → quantizer ──→ signal graph (pitch_hz, gravity_weights, etc.)
  audio_analysis ←── master bus (rms/peak feedback)

Audio output path (single):
  organisms ──AudioBlock──→ master bus (mix + crossover + limiters + DC block) → speakers

Organism (AffinityGraph):
  organism_A ←── infrastructure outputs (pitch_hz, rms, beat_phase, etc.)
  organism_A ←→ organism_B (learned edges, Hebbian)
  organism_A ──→ infrastructure inputs (if organism produces control signals)
  organism_A ──→ master bus (AudioBlock submission via ring buffer)
```

Infrastructure outputs are available as signal sources for organisms. Edges from
infrastructure→organism live in the AffinityGraph (managed by the organism side).
Infrastructure modules have no emotion state — only the organism's valence drives
Hebbian updates on those edges.

### Audio Output: Master Bus as Single Path

All audio reaches speakers through the **master bus** — there is no separate voice
rendering path. The current VoicePool (infrastructure scaffolding for S05) will be
absorbed into the master bus architecture when organisms arrive (S11+):

- **Now (S05–S06)**: VoiceModule sends commands → VoicePool renders → MasterBus limits
- **Future (S11+)**: Organisms own their DSP (FunDSP atoms/cells). Each organism
  submits stereo AudioBlocks via the ring buffer. The master bus mixes all submissions
  and applies dynamics processing. VoiceModule/VoicePool are retired.
- **Why**: One audio output path eliminates redundant mixing, reduces latency, and
  removes a class of bugs (double-limiting, gain staging mismatches, orphaned voices).

## Composition Hierarchy

### Atoms (L2) — Primitive Signal Behaviors

Single-function units: oscillate, threshold, filter, gate, envelope, delay, sample-and-hold.
Stateless or minimal state. One input → one output (or few).

Examples: a sine oscillator, a lowpass filter, an ADSR envelope.

### Molecules (L3) — Small Atom Combinations

2–5 atoms wired together for a coherent function. Internal wiring is fixed (not learned).

Examples: filtered oscillator (sine + SVF), amplitude envelope (oscillator + ADSR),
spectral follower (FFT + peak tracker).

### Cells (L4) — Functional Units with Identity

Molecules combined into a self-contained voice or behavior. Have timbral/rhythmic identity.
Cell-level parameters define character (filter brightness, envelope shape, modulation depth).

Examples: a synthesis voice (oscillator molecule + filter molecule + envelope + modulation
routing), a rhythmic pattern generator (clock + sequencer + gate logic).

### Organisms (L5) — Full Instruments

Multiple cells orchestrated together. Route through AffinityGraph — learn which
infrastructure signals to consume, which connections between cells are productive.
Have emotions driving their learning. Rendered as blobs with thermal-palette coloring.

Example: a synth organism with 4 voice cells, scale affinity toward Bhairav,
rhythmic gravity toward 7-beat patterns.

## DNA

DNA is the blueprint that defines an organism's structure:

- Which cells compose it, how they're wired
- Parameter ranges and defaults for each cell
- Affinity biases (what infrastructure signals it prefers)
- Saved as serializable data (JSON or binary)
- Operations: save, load, clone, mutate, crossover

DNA does NOT encode learned weights — those emerge from the AffinityGraph.
DNA encodes structure; learning encodes behavior.

## Module Contract

Two traits, split for headless support:

```rust
// Always compiled — pure data/signal contract
pub trait ModuleCore: Send {
    fn schema(&self) -> &ModuleSchema;
    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>);
    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError>;
    fn tick(&mut self, dt: f32);
}

// Only with `ui-egui` feature — custom inspector panels
#[cfg(feature = "ui-egui")]
pub trait ModuleUi {
    fn ui(&mut self, ui: &mut egui::Ui);
}
```

**ModuleSchema** declares ports, category, and **tier** (`Infrastructure` or `Organism`).
The tier determines which router handles the module's edges.

**PortId** is `Copy` (`u32`), assigned by global counter when ports are created.
`PortRegistry` maps IDs to human-readable names for UI/debug/ledger.

**Tiered UI**: Every module automatically gets a minimal inspector showing
port list, edge weights, and ledger events. Modules that need domain-specific
controls implement `ModuleUi` for a custom panel below. Infrastructure modules
show as utility labels; organism modules show emotion gauges.

## Signal Types

All heap-heavy variants are `Arc`-wrapped for O(1) clone during routing:

```rust
pub enum Signal {
    Float(f32),
    Bool(bool),
    Trigger,
    Text(Arc<str>),
    AudioBlock(Arc<Vec<f32>>),
    FrameRef(Arc<FrameBuffer>),
    Embedding([f32; 4]),
    PixelSample([f32; 4]),
    Pattern(Arc<Vec<f32>>),
}
```

## Visual Model: Organisms Are the Blobs

Only **organisms** (L5) are blobs. Infrastructure modules are not visualized as
blobs — they're the substrate. The blob renderer shows:

- Each organism = one blob
- **Size** = organism activity level
- **Color** = thermal palette from organism emotion (valence→hue, arousal→temperature)
- **Connections** = smin-merged bridges between organisms with strong affinity edges
- **Pulse** = beat phase from connected TalaModule

Infrastructure is shown as fixed utility labels in a sidebar or status bar,
not as learning blobs.

## Session Map

```
S01  ✅  Module contract + substrate     [L0]
S02  ✅  Routing backbone (AffinityGraph) [L0] — organism-tier only
S02b ✅  Routing refinement              [L0] — range/rate-aware edge discovery
S03  ✅  First input modules             [L1 infrastructure]
S04  ✅  Tuning + gravity core           [L1 infrastructure]
S05  ✅  Audio voice output              [L1 infrastructure]
S05b ✅  Audio dynamics: FunDSP bus      [L1 infrastructure]
S05c ✅  Infrastructure routing split    [L0+L1] — two-tier architecture
S06  ✅  Rhythm + raga                   [L1 infrastructure]
S07  ··  Camera + video                  [L1 infrastructure]
S08  ··  LLaVA vision                    [L1 infrastructure]
S11  ··  Atom + molecule primitives      [L2+L3]
S12  ··  Cell composition + DNA          [L4]
S13  ··  First organism                  [L5]
S09  ··  Visual output (blob renderer)   [L5] — organisms only
S10  ··  UX shell + DNA editor           [L6]
```

## Dependency Graph

```
S01 ✅ → S02 ✅ → S03 ✅ → S04 ✅ → S05 ✅ → S05b ✅ → S02b ✅ → S05c ✅ → S06 ··
                    │                                │
                    │                      S11 ·· → S12 ·· → S13 ··
                    │
                    ├──→ S07 ·· → S08 ··
                    │
                    └──→ S09 ·· → S10 ··
```

S07/S08 (camera/LLaVA) can run in parallel with S06 (rhythm/raga).
S11–S13 (composition hierarchy) depend on S05c (two-tier architecture).
S09 (visual output) depends on S13 for organisms to render.

## Nannou Strategy

| Component | Action | Destination |
|-----------|--------|-------------|
| nannou_wgpu | Fork | `src/substrate/wgpu_helpers/` — TextureBuilder, pipeline constructors |
| nannou_isf | Fork + extend | `src/substrate/isf/` — ISF parser, param→port mapping |
| nannou_osc | Use as dependency | OSC sender/receiver for live control surfaces |
| nannou_egui | Study patterns | egui+winit+wgpu integration reference |
| nannou_audio | Skip | Thin cpal wrapper — use cpal directly |
| nannou app loop | Skip | Single-threaded model doesn't fit our architecture |

## New Crate Dependencies (added incrementally)

| Session | Crate | Purpose |
|---------|-------|---------|
| S01 | `cpal` | Audio I/O |
| S01 | `ringbuf` | Lock-free audio↔control comms |
| S01 | `nannou_osc` | OSC sender/receiver |
| S06 | `serde_yml` or Rust constants | Tala/raga definitions (serde_yaml archived) |
| S02 | `rand` + `rand_xoshiro` | Fast RNG for exploration/stochastic routing |
| S07 | `nokhwa` | Camera capture |
| S08 | `candle` (optional) | LLM inference |

## Cargo Features

```toml
[features]
default = ["ui-egui"]
ui-egui = []          # enables ModuleUi trait + egui inspector panels
```

## File Structure After All Sessions

```
src/
├── main.rs
├── app.rs                    (SeedReactor integration, module palette)
├── module/
│   ├── mod.rs                (ModuleCore trait, ModuleId, SignalError)
│   ├── signal.rs             (Signal enum, SignalType, FrameBuffer)
│   ├── port.rs               (PortId(u32), Port, PortRegistry, PortRate)
│   ├── schema.rs             (ModuleSchema, ModuleCategory, ModuleTier)
│   └── ui.rs                 (ModuleUi trait — feature-gated)
├── substrate/
│   ├── mod.rs
│   ├── audio.rs              (cpal stream, audio callback, ringbuf)
│   ├── channel.rs            (lock-free SPSC channels)
│   ├── camera.rs             (camera capture thread)
│   ├── llm.rs                (LLM inference thread)
│   ├── isf/                  (ISF parser + param→port mapping)
│   └── osc.rs                (nannou_osc wrapper)
├── affinity/
│   ├── mod.rs
│   ├── edge.rs               (EdgeAffinity)
│   ├── emotion.rs            (ModuleEmotion)
│   ├── graph.rs              (AffinityGraph — organism-tier routing)
│   └── ledger.rs             (LedgerRingBuffer)
├── reactor/
│   ├── mod.rs                (SeedReactor — two-tier routing)
│   ├── routing.rs            (affinity weights → delivery)
│   └── infrastructure.rs     (InfrastructureRouter — fixed routing)
├── modules/
│   ├── keyboard_input.rs     (Infrastructure)
│   ├── cursor_input.rs       (Infrastructure)
│   ├── audio_analysis.rs     (Infrastructure)
│   ├── quantizer.rs          (Infrastructure — QuantizerModule)
│   ├── voice_module.rs       (Infrastructure — VoiceModule)
│   ├── tala_module.rs        (Infrastructure — TalaModule)
│   ├── raga_module.rs        (Infrastructure — RagaModule)
│   ├── camera_module.rs      (Infrastructure)
│   ├── pixel_probe.rs        (Infrastructure)
│   ├── video_file_module.rs  (Infrastructure)
│   ├── llava_module.rs       (Infrastructure)
│   ├── tool_glyph_module.rs
│   ├── data_diagram_module.rs
│   ├── ascii_texture_module.rs
│   └── isf_visual_module.rs
├── audio/
│   ├── mod.rs
│   ├── voice.rs              (Voice DSP)
│   └── voice_pool.rs         (VoicePool)
├── tuning/
│   ├── mod.rs
│   ├── scala.rs              (.scl parser, TuningSystem)
│   ├── pitch_gravity.rs      (PitchGravity algorithm)
│   ├── rhythm_gravity.rs     (TalaGrid, euclidean rhythms)
│   ├── raga.rs               (RagaMode, gamaka)
│   ├── gamaka.rs             (ornament state machine)
│   ├── gravity_control.rs    (emotion → gravity mapping)
│   └── scale_morph.rs        (raga transitions)
├── renderer/
│   ├── mod.rs
│   ├── blob_renderer.rs      (organism blobs — SDF renderer)
│   ├── font_atlas.rs
│   └── shape_atlas.rs
├── ui/
│   ├── mod.rs
│   ├── inspector.rs          (auto-gen module inspector)
│   ├── controls.rs           (global controls)
│   ├── ledger_view.rs
│   ├── presets.rs
│   └── module_palette.rs
├── recorder.rs
├── sdf.rs
└── automation.rs             (Hosono test)
assets/
├── scales/                   (.scl files)
├── ragas/                    (.yaml raga definitions)
├── tala/                     (.yaml tala definitions)
├── shaders/                  (.isf visual shaders)
├── presets/                  (.json presets)
├── fonts/
└── elements/
```
