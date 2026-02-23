# Solido 0.6 — Module-First Audiovisual Synthesis Engine

> From MAKE A BABY to the Hosono Test: raga-based generative
> synthesis through a Hebbian affinity graph, where everything
> is a Module and every Module is a blob.

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

## Architecture: Module-First Design

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 5 — UX SHELL                                             │
│  egui panels, per-module inspectors, presets, ledger, panic     │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 4 — OUTPUT MODULES                                       │
│  Synthesis voices, blob SDF renderer, data diagrams,            │
│  ASCII texture mapper, tool glyph overlays, ISF shader units    │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 3 — PROCESSING MODULES                                   │
│  Pitch gravity, rhythm gravity, raga quantizer,                 │
│  spectral analysis, pattern generators                          │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 2 — INPUT MODULES                                        │
│  Camera frames, cursor/pixel, keyboard, LLaVA,                  │
│  audio analysis, OSC, MIDI, video file                          │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 1 — ROUTING BACKBONE                                     │
│  AffinityGraph, SeedReactor, typed ports, Hebbian learning,     │
│  emotion, homeostasis, ledger, PortRegistry                     │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 0 — MODULE CONTRACT + SUBSTRATE                          │
│  ModuleCore trait, Signal types, PortId + registry, ISF parser, │
│  cpal audio, ringbuf channels, feature-gated ModuleUi           │
└─────────────────────────────────────────────────────────────────┘
```

## Visual Model: Blobs Replace Organisms

The L-shaped organisms from 0.4/0.5 are retired. Each Module becomes a
round blob node in the SDF renderer:

- **Position** = spatial arrangement on canvas
- **Size** = activity level (EWMA throughput)
- **Edge sharpness** = gravity state of connected processing modules
- **Color** = thermal palette driven by emotion (valence→hue, arousal→temperature)
- **Connections** = smin-merged soft bridges between modules with strong affinity edges
- **Pulse** = beat phase from connected TalaModule

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

**PortId** is `Copy` (`u32`), assigned by global counter when ports are created.
`PortRegistry` maps IDs to human-readable names for UI/debug/ledger.

**Tiered UI**: Every module automatically gets a minimal inspector showing
port list, emotion gauges, edge weights, and ledger events. Modules that
need domain-specific controls implement `ModuleUi` for a custom panel below.

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

## Session Map

```
S01  ✅  Module contract + substrate    ModuleCore, Signal, PortId, ISF parser, cpal      [L0]
S02  ✅  Routing backbone               AffinityGraph, SeedReactor, Hebbian tick           [L1]
S02b ··  Routing refinement             Range-aware edge discovery, stop Float crosstalk   [L1]
S03  ✅  First input modules            Keyboard, cursor/pixel, audio analysis stub        [L2]
S04  ✅  Tuning + gravity core          Scala, TuningSystem, PitchGravity as Module        [L3]
S05  ··  First output: audio voice      VoicePool as Module, wired through affinity        [L4]
S06  ··  Rhythm + raga                  TalaGrid, RagaMode, gamaka as Modules              [L3]
S07  ··  Camera + video modules         Frame capture, cursor-over pixel stream            [L2]
S08  ··  LLaVA vision module            Multimodal LLM as Module, frame→signals            [L2]
S09  ··  Visual output modules          Tool glyphs, data diagrams, ASCII textures, ISF    [L4]
S10  ··  UX shell + integration         Inspectors, presets, ledger view, Hosono test      [L5]
```

The affinity graph exists from S02. Every module added after S02 routes
through it immediately. No "direct parameter passing" to refactor later.

## Dependency Graph

```
S01 ✅ → S02 ✅ → S03 ✅ → S04 ✅ → S05 ·· → S02b ·· → S06 ··
                    │
                    ├──→ S07 ·· → S08 ··
                    │
                    └──→ S09 ·· → S10 ··
```

S07/S08 (camera/LLaVA) can run in parallel with S04-S06 (tuning/gravity).
S09 (visual outputs) depends on S07 for FrameRef but can start with
audio signals only.

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
│   ├── schema.rs             (ModuleSchema, ModuleCategory)
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
│   ├── graph.rs              (AffinityGraph)
│   └── ledger.rs             (LedgerRingBuffer)
├── reactor/
│   ├── mod.rs                (SeedReactor)
│   └── routing.rs            (affinity weights → delivery)
├── modules/
│   ├── keyboard_input.rs
│   ├── cursor_input.rs
│   ├── audio_analysis.rs
│   ├── quantizer.rs          (QuantizerModule)
│   ├── voice_module.rs       (VoiceModule)
│   ├── tala_module.rs        (TalaModule)
│   ├── raga_module.rs        (RagaModule)
│   ├── camera_module.rs
│   ├── pixel_probe.rs
│   ├── video_file_module.rs
│   ├── llava_module.rs
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
│   ├── blob_renderer.rs      (refactored from organism_renderer.rs)
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
