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

## Dialogue Model: Prompt → Interpretation → Response

Human input to organisms is a **prompt**, not a command. Organisms **interpret** prompts
through their species personality. The affinity graph mediates delivery strength.

### Signal Flow

```
PROMPT LAYER (infrastructure):
  KeyboardModule ──→ pitch_hz, gate, trigger     ──→ AffinityGraph
  SequencerModule ──→ step_pitch, step_gate, accent ──→ AffinityGraph

INTERPRETATION LAYER (organism):
  OrganismModule receives signals with affinity-weighted strength
  ├─ Internal intent: organism's own cells (seq_cell, logic_seq_cell, func_gen_cell)
  ├─ Personality blend: fidelity * affinity_weight determines external vs internal
  └─ Response: organism emits actual_pitch, rhythm_density → back into graph

CROSS-ORGANISM DIALOGUE:
  ACID emits rhythm → TBLK syncs or counterpunches
  DRON emits harmonic field → HOSO's filter tracks it
  SPGL ignores everyone → slowly pulls others toward its pitch center
```

### Personality Blend

```
actual_behavior = lerp(internal_pattern, external_prompt, affinity_weight * fidelity)
```

Where `fidelity` is a DNA param per organism (0.0 = ignores input, 1.0 = faithful follower).

### The Six Organisms

| Species | Aesthetic | Dialogue Personality | Fidelity |
|---------|-----------|---------------------|----------|
| **DRON** | Ambient drone | Slowly absorbs pitch, never jumps | 0.3 |
| **HOSO** | Cochin Moon | Rigid follower through nasal PWM character | 0.9 |
| **SPGL** | Expanding Universe | Mostly ignores prompts, func_gens dominate | 0.1 |
| **ACID** | Acid Mt. Fuji | High fidelity + own accent/slide interpretation | 0.8 |
| **TBLK** | Indian percussion | Follows rhythm, quantizes pitch to membrane modes | 0.5 |
| **KKIT** | TR-909 drum kit | Mechanical gate/accent follower, ignores pitch | 0.95 |

### SequencerModule (Infrastructure)

16-step pattern with per-step pitch, gate, accent, slide. Human writes via step grid UI.
Emits `step_pitch`, `step_gate`, `step_accent`, `beat_phase` into the affinity graph.
Bidirectional step grid shows human pattern + per-organism response overlay.

See [S19](S19-dialogue-architecture.md) for full specification.

## Granular Cell Inventory

Cells are single-function units wired by DNA. One function per cell.

| Cell | Session | I/O | Purpose |
|------|---------|-----|---------|
| `drone_bed` | S13 (deprecated S26) | none→stereo | Monolithic drone (legacy) |
| `osc_cell` | S20 | none→stereo | Dual detuned osc, 6 waveforms incl. pulse/PWM |
| `filter_cell` | S20 | stereo→stereo | Moog/lowpass/highpass/bandpass |
| `lfo_cell` | S20 | none→mono | Bipolar control signal LFO |
| `mixer_cell` | S20 | stereo→stereo | Gain + pan terminal |
| `seq_cell` | S21 | none→mono (trigger) | 16-step sequencer (organism's innate pattern) |
| `env_cell` | S21 | trigger→mono | ADSR envelope |
| `slew_cell` | S21 | mono→mono | Portamento/glide |
| `accent_env_cell` | S21 | trigger→mono | Accent decay envelope |
| `func_gen_cell` | S22 | none→mono | Multi-minute math curves (cosine_sum, etc.) |
| `saw_bank_cell` | S22 | none→stereo | N detuned saws (1–8 voices) |
| `logic_seq_cell` | S22 | none→mono (trigger) | Algorithmic triggers (euclidean/fibonacci/prime) |
| `diode_filter_cell` | S23 | stereo→stereo | 18dB/oct 3-pole diode ladder |
| `tape_delay_cell` | S23 | stereo→stereo | Tape echo with HF loss |
| `strike_voice_cell` | S24 | trigger→stereo | Resonant membrane percussion |
| `noise_burst_cell` | S24 | trigger→mono | Short noise transient |
| `drum_voice_cell` | S25 | trigger→stereo | Parameterized 909 drum synth (7 presets) |
| `sample_cell` | S25 | trigger→stereo | PCM sample playback |

Total: 1 legacy + 17 new = 18 cells.

## DNA Presets

| File | Species | Preset Name | Aesthetic |
|------|---------|-------------|-----------|
| `dron-alpha.json` | dron | — | Legacy monolithic drone |
| `dron-composable.json` | dron | — | Composable granular drone |
| `hoso-malabar.json` | hoso | "Malabar Ground Floor" | Cochin Moon |
| `spgl-kepler.json` | spgl | "Kepler's Harmony" | Expanding Universe |
| `acid-kinoko.json` | acid | "Kinoko Shrine Acid" | Acid Mt. Fuji |
| `tblk-dha.json` | tblk | "Dha" | Indian percussion |
| `kkit-909.json` | kkit | "909" | TR-909 drum kit |

## Session Map

```
S01  ✅  Module contract + substrate     [L0]
S01b ··  Lifecycle hooks + event channel [L0] — on_register, graceful shutdown, receive_event
S02  ✅  Routing backbone (AffinityGraph) [L0] — organism-tier only
S02b ✅  Routing refinement              [L0] — range/rate-aware edge discovery
S02c ··  Edge pinning + exploration cache [L0] — pinned edges, cached candidates, two-tier ledger
S03  ✅  First input modules             [L1 infrastructure]
S04  ✅  Tuning + gravity core           [L1 infrastructure]
S05  ✅  Audio voice output              [L1 infrastructure]
S05b ✅  Audio dynamics: FunDSP bus      [L1 infrastructure]
S05c ✅  Infrastructure routing split    [L0+L1] — two-tier architecture
S06  ✅  Rhythm + raga                   [L1 infrastructure]
S11  ✅  Atom + molecule primitives      [L2+L3] — 17 atoms, 9 molecules, Shared/var
S07  ··  Camera + video                  [L1 infrastructure]
S08  ··  LLaVA vision                    [L1 infrastructure]
S12  ⚠️  Cell composition + unified DNA  [L4] — infrastructure done, 7 cells superseded by S20-S25
S09  ··  Visual outputs + organism sim   [L4+L5] — blob renderer, lobes, interactions, fusion
S09b ✅  Animation pipeline              [L4+L5] — additive potential fields, per-org emotion, sonar
S13  ✅  First organisms                 [L5] — three creatures, organism panel, DSP fixes
S14  ··  Cell-level audio wiring         [L4] — audio/mod wire dispatch, topological tick, wire params
S14b ··  Cell-to-module signal bridge    [L4→L5] — cell events to affinity graph, cross-org learning
S15  ··  Port semantic tags              [L0] — Frequency/Level/Phase/Gate port semantics
S16  ··  Normalized parameter space      [L0+L4] — ParamScale, ParamDescriptor, mutation operators
S17  ··  Inverse synthesis pipeline      [L5+L6] — sound-target search, headless renderer, dataset gen
S10  ✅  UX shell + DNA editor           [L6] — status bar, controls, keyboard, ledger, presets
S18  ✅  Parameter bridge architecture   [L4] — Shared handle bridge, control↔audio, CellRegistry ranges
S19  ✅  Dialogue architecture           [L0+L1] — SequencerModule, fidelity, personality blend (UI deferred to S26)
S20  ✅  Granular cell kit + DRON rebuild [L4] — osc/filter/lfo/mixer cells, composable architecture
S21  ✅  HOSO (Cochin Moon)              [L4+L5] — seq/env/slew/accent cells, pulse osc, modulation fixes (known issues documented)
S22  ··  SPGL (Expanding Universe)       [L4+L5] — func_gen/saw_bank/logic_seq cells, SPGL organism
S23  ··  ACID (Acid Mt. Fuji)            [L4+L5] — diode_filter/tape_delay cells, ACID organism
S24  ✅  TBLK Tabla                      [L4+L5] — strike_voice/noise_burst cells, TBLK rebuild
S25  ✅  KKIT TR-909                     [L4+L5] — drum_voice/sample cells, KKIT organism
S26  ··  Six-organism integration        [L5+L6] — gain staging, visual identity, acidBros UI, sequencer grid
```

## Dependency Graph

```
S01 ✅ → S02 ✅ → S03 ✅ → S04 ✅ → S05 ✅ → S05b ✅ → S02b ✅ → S05c ✅ → S06 ✅
  │        │        │                                │
  │        │        │                      S11 ✅ → S12 ·· ──┐
  │        │        │                                         ├──→ S13 ✅ → S10 ✅
  │        │        ├──→ S09 ·· → S09b ✅ ────────────────────┘       │
  │        │        │                                                  ↓
  │        │        └──→ S07 ·· → S08 ··                    S14 ·· → S14b ··
  │        │                                                  │
  │        └──→ S02c ·· (edge pinning — independent of S13)   │
  │                                                           ↓
  │                                               S16 ·· → S17 ··
  │                                          (normalized params → inverse synthesis)
  │
  └──→ S01b ·· (lifecycle hooks — independent, can land anytime)
       S15 ·· (port semantics — depends on S01, benefits S02c + S16)

S13 ✅ → S18 ✅ → S19 ·· → S20 ·· → S21 ·· → S23 ·· ────→ S26 ··
                                 │         ↘ S24 ·· ───────↗
                                 ↘ S22 ·· → S25 ·· ───────↗
```

### Spec dependencies (S01–S17)

- **S01b** (lifecycle hooks): depends only on S01. Can land anytime. Improves organism shutdown and eliminates as_any.
- **S02c** (edge pinning): depends on S02. Independent of S13. Improves exploration efficiency and user control.
- **S14** (cell wiring): depends on S12 + S13. Makes audio/modulation wires functional in OrganismDsp.
- **S14b** (cell signal bridge): depends on S14. Bridges cell events into the affinity graph for cross-organism learning.
- **S15** (port semantics): depends on S01. Benefits S02c (smarter exploration) and S16 (semantic-aware normalization). Reduces spurious edge creation.
- **S16** (normalized params): depends on S01 + S12. `ParamScale`/`ParamDescriptor` give every cell param a `[0,1]` normalized representation with log/linear/int scaling. Enables uniform mutation, crossover, and ML export. Benefits from S15 (semantic tags on params).
- **S17** (inverse synthesis): depends on S13 + S16 + S14. Evolutionary sound-target search, headless organism renderer, dataset generation for future neural estimators. "You hum, the creature learns to sing it back."

### Spec dependencies (S18–S26)

- **S18** (param bridge): Complete. Shared handle architecture for control↔audio bridging.
- **S19** (dialogue architecture): Complete (UI deferred to S26). SequencerModule, expanded OrganismModule ports, fidelity DNA, personality blend. Foundational for all subsequent organisms. Sequencer grid UI moved to S26.
- **S20** (granular cells): ✅ Complete. osc/filter/lfo/mixer cells, composable architecture, DRON rebuild from parts.
- **S21** (HOSO): ✅ Complete. seq/env/slew/accent cells + pulse osc mode. Critical bugfixes applied (modulation timing, control signal isolation). Known refinement issues documented.
- **S22** (SPGL): ✅ Complete. func_gen/saw_bank/logic_seq cells. SPGL organism.
- **S23** (ACID): ✅ Complete. diode_filter + tape_delay_bus. ACID organism + gain staging.
- **S24** (TBLK): ✅ Complete. strike_voice (3-resonator membrane) + noise_burst cells. TBLK organism, 4:3 polyrhythm.
- **S25** (KKIT): ✅ Complete. drum_voice (7 presets: kick/snare/hat_closed/hat_open/clap/tom/rim) + sample_cell (PCM WAV playback). KKIT organism at 130 BPM four-on-the-floor.
- **S26** (integration): Depends on S23 + S24 + S25 (all six organisms). Gain staging, visual identity, acidBros UI.

S01b, S02c, and S15 are **foundation improvements** — they can be built in parallel with S13 work.
S14 and S14b are **post-S13** — they deepen organisms once the basic system is working.
S16 is **post-S12** — it adds a normalization layer over CellDna params. Can proceed once cell DNA is stable.
S17 is **post-S13 + S16 + S14** — it needs live organisms, normalized params, and working cell wiring to render candidates.
S21 and S22 are **parallel** — HOSO's seq/env cells and SPGL's func_gen/saw_bank cells are independent.
S23, S24, S25 converge into S26 — all six organisms must exist before integration.

S07/S08 (camera/LLaVA) can run in parallel with everything else.
S11 (atoms/molecules) is complete. S12 (cells/DNA) builds on S11.
S09 (blob renderer + organism sim) is independent of S11/S12 until S13.
**S09 and S12 can run in parallel** — visual and audio paths converge at S13.
S13 (first organisms) depends on S11 + S12 + S09 (audio + visual + DNA).
DNA schema is unified in S12 — S09 references BodyDna, RenderDna, PhysicsDna sections.

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

## Known Issues (S21 Aftermath)

**Critical fixes committed (5303064), but refinement needed:**

### 1. Pitch Modulation Architecture
**Issue**: Additive modulation (`freq = base + slew`) causes octave-shift artifacts. When slew outputs 146.8 Hz and base is 130.8 Hz, result is 277.6 Hz (wrong octave).

**Root cause**: Pitch CV needs **replacement** semantics (1V/Oct standard), not additive. The slew_cell output should directly set the frequency, not add to a base.

**Workaround**: Set base freq to 0 in DNA, but this causes startup transients when slew ramps from 0→target.

**Proper fix**: Add `Replace` modulation mode where `param = mod_signal * gain` (no base involved). Or use exponential pitch modulation: `freq = base * 2^(mod_signal / 1200)` for cent-based CV.

**Impact**: HOSO plays wrong pitches (too high). SPGL/ACID will have same issue.

### 2. Cell Parameter Range Constraints
**Issue**: Some cell parameters lack proper min/max constraints, allowing values that cause audio artifacts or instability.

**Examples**:
- Filter resonance can exceed stable range, causing self-oscillation
- Envelope attack/release can be set to 0, causing discontinuities
- LFO depth not clamped, allowing extreme modulation

**Proper fix**:
- Review all `CellRegistry::register_ranges()` entries in `src/dsp/cell/mod.rs`
- Add conservative min/max for each param based on audio stability
- Document safe ranges in cell implementation files

**Impact**: Users can create unstable/broken sounds via UI sliders.

### 3. UI Slider Sensitivity and Scaling
**Issue**: Linear sliders for frequency/time parameters have poor usability. Moving a linear freq slider from 20→20000 Hz gives very coarse control at low end, very fine at high end.

**Examples**:
- Frequency params need logarithmic scaling (20 Hz steps at low end, 1000 Hz steps at high end)
- Envelope times feel wrong with linear sliders (1ms, 2ms, 3ms... vs 10ms, 100ms, 1s)
- Resonance/depth params bunched near 0, hard to fine-tune

**Proper fix**:
- Use `.logarithmic(true)` for freq/time sliders in `organism_panel.rs`
- Add `ParamScale` enum (Linear/Log/Exp) to `CellRegistry`
- UI reads scale hint from registry, applies correct transform

**Impact**: Cell controls feel clumsy, hard to dial in musical values.

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
│   ├── sequencer.rs          (Infrastructure — SequencerModule, S19)
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
├── dsp/
│   ├── mod.rs
│   ├── shared.rs             (Shared — lock-free Arc<AtomicU32>)
│   ├── organism_dsp.rs       (OrganismDsp — cell wiring + tick)
│   ├── cell_registry.rs      (CellRegistry — factory + param ranges)
│   └── cell/
│       ├── mod.rs             (DspCell trait)
│       ├── drone_bed.rs       (deprecated S26)
│       ├── osc_cell.rs        (S20 — dual detuned osc, 6 waveforms)
│       ├── filter_cell.rs     (S20 — moog/LP/HP/BP)
│       ├── lfo_cell.rs        (S20 — bipolar control LFO)
│       ├── mixer_cell.rs      (S20 — gain + pan terminal)
│       ├── seq_cell.rs        (S21 — 16-step sequencer)
│       ├── env_cell.rs        (S21 — ADSR envelope)
│       ├── slew_cell.rs       (S21 — portamento/glide)
│       ├── accent_env_cell.rs (S21 — accent decay envelope)
│       ├── func_gen_cell.rs   (S22 — multi-minute math curves)
│       ├── saw_bank_cell.rs   (S22 — N detuned saws)
│       ├── logic_seq_cell.rs  (S22 — euclidean/fibonacci/prime triggers)
│       ├── diode_filter_cell.rs (S23 — 18dB/oct diode ladder)
│       ├── tape_delay_cell.rs (S23 — tape echo + HF loss)
│       ├── strike_voice_cell.rs (S24 — resonant membrane percussion)
│       ├── noise_burst_cell.rs (S24 — short noise transient)
│       ├── drum_voice_cell.rs (S25 — parametric 909 drum synth)
│       └── sample_cell.rs    (S25 — PCM sample playback)
├── audio/
│   ├── mod.rs
│   ├── reverb_bus.rs          (ReverbBus — send bus)
│   └── master_bus.rs          (MasterBus — mix + dynamics)
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
│   ├── module_palette.rs
│   ├── organism_panel.rs     (per-cell param sliders + bypass)
│   ├── sequencer_grid.rs     (S19 — bidirectional step grid)
│   ├── transport.rs          (S26 — play/stop/BPM)
│   └── oscilloscope.rs       (S26 — CRT waveform, optional)
├── recorder.rs
├── sdf.rs
└── automation.rs             (Hosono test)
assets/
├── scales/                   (.scl files)
├── ragas/                    (.yaml raga definitions)
├── tala/                     (.yaml tala definitions)
├── shaders/                  (.isf visual shaders)
├── presets/                  (.json presets)
├── dna/                      (organism DNA presets)
│   ├── dron-alpha.json       (legacy monolithic drone)
│   ├── dron-composable.json  (S20 — granular drone)
│   ├── hoso-malabar.json     (S21 — Cochin Moon)
│   ├── spgl-kepler.json      (S22 — Expanding Universe)
│   ├── acid-kinoko.json      (S23 — Acid Mt. Fuji)
│   ├── tblk-dha.json         (S24 — Indian percussion)
│   └── kkit-909.json         (S25 — TR-909 drum kit)
├── fonts/
└── elements/
```
