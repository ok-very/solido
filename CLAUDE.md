# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Solido 0.6** is a Rust + wgpu audiovisual synthesis engine built on a module-first architecture. Modules (inputs, processors, outputs) communicate through typed signals routed via a Hebbian affinity graph. The system learns which connections are productive through emotional valence and homeostatic regulation. Visual output renders modules as SDF blobs with thermal-palette coloring driven by module emotion state.

Lineage: MAKE A BABY (Max/MSP) → Godot 0.3 → TypeScript/Canvas 0.4 → Rust/eframe 0.5 → **Rust/wgpu module-first 0.6**

## Build & Run

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run                      # launch native window (eframe + wgpu)
cargo test                     # all unit tests
cargo test affinity            # tests in affinity module only
cargo test -- --test-threads=1 # serialize tests (useful for audio device contention)
RUST_LOG=debug cargo run       # verbose logging via env_logger
```

Feature flags:
- `ui-egui` (default): Enables `ModuleUi` trait and egui inspector panels. Disable with `--no-default-features` for headless/test builds.

## Architecture

### Five-Layer Stack

```
L5  UX Shell         egui panels, inspectors, presets, ledger view
L4  Output Modules   Voice DSP (cpal), blob SDF renderer, ISF shaders
L3  Processing       Pitch gravity, rhythm gravity, raga/gamaka quantizers
L2  Input Modules    Keyboard, cursor, camera, audio analysis, LLaVA
L1  Routing Backbone AffinityGraph + SeedReactor + RoutingTable
L0  Module Contract  ModuleCore trait, Signal types, PortId, ISF parser, cpal substrate
```

L0-L4 are implemented. L5 is partial. Development continues incrementally per the session map.

### Core Abstractions

**ModuleCore trait** (`src/module/mod.rs`) — The universal contract. Every module implements `schema()`, `emit_signals()`, `receive_signal()`, and `tick()`. The trait is `Send`, headless-safe. GUI is split into a separate feature-gated `ModuleUi` trait (`src/module/ui.rs`).

**Signal** (`src/module/signal.rs`) — 9 typed variants (Float, Bool, Trigger, Text, AudioBlock, FrameRef, Embedding, PixelSample, Pattern). Heap-heavy variants are `Arc`-wrapped for O(1) clone during multi-cast routing.

**PortId** (`src/module/port.rs`) — `Copy` `u32` assigned by a global atomic counter. Cheap to hash, no allocation. `PortRegistry` maps IDs to human-readable names for UI/debug.

**SeedReactor** (`src/reactor/mod.rs`) — Central orchestrator. Owns all modules, drives the tick cycle: Emit → Route → Deliver → Learn. Uses a pre-allocated `emit_buffer` to avoid per-tick allocation.

**AffinityGraph** (`src/affinity/graph.rs`) — Hebbian learning on edges between module ports. Edges have weight, eligibility trace, goodput, and impact. Graph evolves: strong edges strengthen, weak edges prune, high arousal triggers stochastic exploration of new connections.

**RoutingTable** (`src/reactor/routing.rs`) — Cached topology with softmax-weighted multi-cast delivery. Rebuilt only on topology changes (edge add/remove), not on continuous weight updates.

**ModuleEmotion** (`src/affinity/emotion.rs`) — Per-module valence [-1,1] and arousal [0,1]. Drives Hebbian reward signals and exploration behavior. Valence = homeostatic satisfaction + navigation reward + harmonic consonance; arousal = surprise + trap stress + harmonic tension.

### Rendering Pipeline

eframe provides the window and event loop. A fullscreen-triangle SDF pass renders all organisms in a single draw call via `organism.wgsl`. The shader samples an MSDF font atlas for text and computes signed distance fields for blob geometry. Frame capture uses async GPU readback for video export (`src/recorder.rs`).

### Tuning & Ecology

**Gravity Wells** (`src/tuning/gravity_well.rs`) — Spatial harmonic attractors with Lennard-Jones force profiles. Organisms orbit wells based on consonance between their root pitch class and the well's tonal center. Well energy drains under occupancy (sub-linear with sqrt(N)), regenerates when empty. Three-state machine: Healthy → Wavering → Dormant.

**Harmonic Interaction** (`src/tuning/harmony.rs`) — Organism-to-organism consonance via Tenney height (log2(p×q) for JI ratio p/q). 12-entry table maps semitone intervals to [0.1, 1.0] consonance. Live Hz consonance uses nearest-JI lookup with 30-cent detuning penalty. Blended: static root (30%) + live pitch (70%). Effects: well quality bonus, niche penalty reduction, emergent affinity term, valence/arousal modulation.

**Navigation Reward** (`src/tuning/gravity_well.rs:WellTracker`) — Per-organism trajectory tracking with 6 events (arrival, departure, slingshot, trapping, transition, passive exit). Events modulate valence/arousal to reward exploration and penalize stasis.

**Pitch Gravity** (`src/tuning/pitch_gravity.rs`) — Quantizes continuous pitch toward scale degrees with weighted pull. Gamaka ornaments add microtonal slides and vibrato.

**Raga/Scale** (`src/tuning/raga.rs`, `src/tuning/scale.rs`) — 5 ragas (Bhairav, Bhairavi, Yaman, Jog, Kafi) with per-degree gravity weights, aroha/avaroha paths, vadi/samvadi. Scala .scl file support for custom tuning systems.

### Lock-Free Audio Path

`src/substrate/audio.rs` uses cpal for platform audio I/O. Communication between audio callback and control thread uses `ringbuf` SPSC channels (`src/substrate/channel.rs`) — no mutexes on the audio thread.

## Session Map & Specs

Development follows a layered session plan. Each session has a spec in `spec/`:

```
L0-S01 → L1-S02 → L2-S03 → L3-S04 → L4-S05
                │                │
                │      L3-S06 ←──┘
                ├──→ L2-S07 → L2-S08
                └──→ L4-S09 → L5-S10
```

Specs are the source of truth for session scope. Audits in `spec/audits/` capture review findings and action items. Archived pre-rewrite specs live in `spec/archive/`.

## Workflow

- **Branch management**: Use Stackit (`st`) for stacked branches. CI enforces lock status and stack ordering via `.github/workflows/stackit.yml`.
- **Main branch**: `main`. Working branch: `v0.6`.
- **New modules** implement `ModuleCore`, register with `SeedReactor`, and route through the affinity graph immediately — no direct parameter passing.

## Key Design Invariants

- **Everything is a Module.** No ad-hoc signal plumbing. All data flows through typed ports and the affinity graph.
- **Emit buffer reuse.** `SeedReactor::emit_buffer` is pre-allocated and cleared each tick. Modules push to it via `emit_signals(&mut buffer)`.
- **Arc-wrapped heap signals.** AudioBlock, FrameRef, Pattern, Text are `Arc`-wrapped. Cloning is O(1).
- **Separated tick phases.** The borrow checker demands strict phase separation: collect all emissions first, then route and deliver. Never mutably borrow modules during both emit and receive.
- **Topology-triggered rebuild.** `RoutingTable` rebuilds on edge add/remove, not on every weight change.
- **Feature-gated UI.** `ModuleCore` compiles without egui. `ModuleUi` only exists under `ui-egui`.

## Working With Me (User Preferences)

- **Ask, don't guess.** When architecture, DSP behavior, or musical intent is ambiguous, serve a questionnaire with concrete options rather than assuming. I like answering questions. Low-confidence decisions should always surface as explicit choice points.
- **Decision hooks.** Before implementing any of these, ask:
  - **DSP behavior**: "Should this be linear ramp or exponential convergence?" / "Should Replace mode fall back to base when source is zero?"
  - **Signal routing**: "Should this go through the AffinityGraph or be direct-wired?" / "Is this a continuous param (Shared) or a discrete event (DspCommand)?"
  - **Musical intent**: "Should organisms sync to the tala grid or drift freely?" / "Is this effect per-organism or global bus?"
  - **Physics/interaction**: "Should this force scale with attachment or be binary threshold?"
  - **Architecture coherence**: When two specs or code paths seem to contradict each other, present both interpretations and ask which is intended.
- **Domain context.** This is an audiovisual synthesis engine inspired by Indian classical music (raga/tala), biological metaphors (organisms, DNA), and emergent systems. The "correct" behavior is often aesthetic, not algorithmic — ask when unsure.

## Conventions

- Rust naming: `snake_case` for functions/variables, `PascalCase` for types/traits, `SCREAMING_SNAKE` for constants.
- `PortId` is always `Copy` — pass by value, never reference.
- `ModuleId` is a `Copy` `u32` — same philosophy.
- Signal type matching uses `SignalType` enum for compatibility checks without inspecting payload.
- Ledger ring buffer (1000 capacity) records weight-change events for explainability.
- SDF primitives live in `src/sdf.rs` — used for both CPU-side distance queries and as reference for shader logic.

## Tools

- `tools/msdf-atlas-gen/` and `tools/msdfgen/` — MSDF font atlas generation for the renderer. Pre-built Windows binaries included.
- Font atlas output: `assets/fonts/Okuda-A5PL-msdf/` (PNG + JSON metrics).
