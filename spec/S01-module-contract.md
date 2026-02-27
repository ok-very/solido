# L0-S01 — Module Contract + Substrate

> Define what a Module is. Set up the plumbing everything flows through.

## Status: IMPLEMENTED

Completed with audit fixes applied. See `spec/audits/L0-S01-audit.md`.

## Goal

Establish the foundational types that every subsequent session builds on:
the ModuleCore trait, Signal enum, port schemas, and substrate layers for
audio and ISF shader parsing. After this session, you can write a Module,
declare its ports, and route typed signals — even though no real modules
exist yet.

## Ancestry (MAKE A BABY)

The Max/MSP patch used `cycle~` as its core oscillator and `dac~`
for 6-channel output. The `ragaraga` and `origaraga` coll objects
stored pitch lookup tables. The `mods_simple_bow` abstraction did
LFO-based modulation. All of these become Modules in our system:
same concept, different substrate.

## Implemented

### 1.1 `src/module/mod.rs` — ModuleCore trait

```rust
pub type ModuleId = u64;

pub trait ModuleCore: Send {
    fn schema(&self) -> &ModuleSchema;
    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>);
    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError>;
    fn tick(&mut self, dt: f32);
}

#[derive(Debug)]
pub enum SignalError {
    WrongType { expected: SignalType, got: SignalType },
    UnknownPort(PortId),
    BufferFull,
}
```

**No GUI dependency.** UI is a separate `ModuleUi` trait behind
`#[cfg(feature = "ui-egui")]` — see `src/module/ui.rs`.

### 1.2 `src/module/signal.rs` — Signal types

All heap-heavy variants Arc-wrapped for O(1) clone during multi-port routing:

```rust
use std::sync::Arc;

pub enum Signal {
    Float(f32),
    Bool(bool),
    Trigger,                        // stateless bang
    Text(Arc<str>),                 // Arc-wrapped: O(1) clone
    AudioBlock(Arc<Vec<f32>>),      // Arc-wrapped: O(1) clone
    FrameRef(Arc<FrameBuffer>),     // shared ref to video frame
    Embedding([f32; 4]),            // projected vector
    PixelSample([f32; 4]),          // RGBA at a point
    Pattern(Arc<Vec<f32>>),         // Arc-wrapped: O(1) clone
}
```

`FrameBuffer` with optional GPU texture handle for zero-copy rendering:

```rust
pub struct FrameBuffer {
    pub pixels: Vec<u8>,                       // CPU-side RGBA8
    pub width: u32,
    pub height: u32,
    pub timestamp: f64,
    pub gpu_texture: Option<Arc<wgpu::Texture>>,  // optional GPU upload
}
```

### 1.3 `src/module/port.rs` — PortId + Port + PortRegistry

**PortId is `Copy` (`u32`)** — assigned by global atomic counter when ports
are created. Zero allocation on the emit path: modules store PortIds as
fields and copy 4 bytes into the buffer each tick.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

pub struct PortRegistry {
    names: HashMap<PortId, Arc<str>>,
}
```

`PortRegistry` maps IDs to human-readable names for UI/debug/ledger.
Built from module schemas when modules register with the SeedReactor.

```rust
#[derive(Clone, Debug)]
pub enum PortRate {
    Audio,    // 44.1kHz (audio thread)
    Block,    // ~60Hz (frame rate)
    Llm,      // ~2-10Hz (inference rate)
    Event,    // sporadic (triggers, key presses)
}

pub struct Port {
    pub id: PortId,
    pub name: Arc<str>,           // human-readable, for schema introspection
    pub direction: PortDirection,
    pub signal_type: SignalType,
    pub rate: PortRate,
    pub range: Option<(f32, f32)>,
    pub description: String,
}
```

### 1.4 `src/module/schema.rs` — Module metadata

```rust
pub struct ModuleSchema {
    pub name: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub side_effects: Vec<String>,
    pub category: ModuleCategory,
}
```

Lookup methods: `schema.input("pitch")`, `schema.input_id("pitch")`.

### 1.5 `src/module/ui.rs` — Feature-gated ModuleUi

```rust
#[cfg(feature = "ui-egui")]
pub trait ModuleUi {
    fn ui(&mut self, ui: &mut egui::Ui);
}
```

Decoupled from ModuleCore. Modules can run headless (bottled scenarios,
CLI mode, audio-only servers) without egui as a dependency.

### 1.6 `src/substrate/audio.rs` — cpal audio substrate

```rust
pub struct AudioSubstrate {
    _stream: cpal::Stream,
    cmd_tx: Sender<AudioCommand>,
    analysis_rx: Receiver<AudioAnalysis>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub enum AudioCommand {
    SpawnVoice { id: u64, freq: f32, cutoff: f32, amp: f32 },
    KillVoice(u64),
    SetParam { id: u64, param: VoiceParam, value: f32 },
    Panic,
}
```

Audio callback constraints:
- Lock-free: no allocations, no mutexes, no panics
- Read AudioCommands from ring buffer (non-blocking pop)
- F32 sample format (I16 fallback deferred to S05)
- Graceful fallback: returns `None` if no audio device

### 1.7 `src/substrate/channel.rs` — Lock-free SPSC channels

```rust
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>);
```

Wraps `ringbuf::HeapRb`. Sender/Receiver are `Send` (compiler-verified,
no manual unsafe). Methods: `try_send`, `try_recv`, `drain`, `available`.

### 1.8 `src/substrate/isf/` — ISF shader parser

Parses `/* { JSON } */` headers from ISF shaders. Scans all comment
blocks, picks the first valid JSON object. Converts ISF parameters to
Module ports via `to_module_schema()`:

| ISF Type | Signal Type | Port Rate |
|----------|-------------|-----------|
| float | Float | Block |
| bool | Bool | Event |
| long | Float | Event |
| event | Trigger | Event |
| point2D | Embedding | Block |
| color | PixelSample | Block |
| image | FrameRef | Block |

Every ISF visual module gets a `rendered_frame` FrameRef output port.

### 1.9 Dependencies

```toml
[features]
default = ["ui-egui"]
ui-egui = []

[dependencies]
cpal = "0.15"
ringbuf = "0.4"
# nannou_osc deferred — add when OSC actually needed
```

### 1.10 Deferred from original spec

- **nannou_wgpu fork** — existing renderer works; fork when new modules need
  pipeline helpers (S09)
- **nannou_osc** — add as dependency when OSC input module is built
- **I16 audio fallback** — add in S05 when voice DSP lands

## Files Created

```
src/module/mod.rs             — ModuleCore trait, ModuleId, SignalError
src/module/signal.rs          — Signal enum (9 types, Arc-wrapped), FrameBuffer
src/module/port.rs            — PortId(u32), Port, PortRegistry, PortRate
src/module/schema.rs          — ModuleSchema, ModuleCategory
src/module/ui.rs              — ModuleUi trait (feature-gated)
src/substrate/mod.rs          — pub mod audio, channel, isf;
src/substrate/audio.rs        — AudioSubstrate, cpal setup, ringbuf channels
src/substrate/channel.rs      — lock-free SPSC Sender/Receiver
src/substrate/isf/mod.rs      — ISF module root
src/substrate/isf/types.rs    — IsfShader, IsfInput, IsfPass, to_module_schema()
src/substrate/isf/parser.rs   — parse_isf(), robust comment-block scanning
```

## Files Modified

```
src/main.rs                   — add `mod module; mod substrate;`
Cargo.toml                    — add cpal, ringbuf, [features] section
```

## Verification (31 tests passing)

1. Signal matching: `Float(1.0).matches_type(&SignalType::Float)` = true
2. Port compatibility: Float port rejects Bool signal
3. ModuleSchema round-trip: create schema, verify port names and IDs
4. PortId is Copy: assign and copy without move
5. PortRegistry lookup: register from schema, lookup by ID
6. ISF parser: reads test shader, extracts 4 inputs + 1 pass
7. ISF→ModuleSchema: brightness→Float, trigger→Trigger, image→FrameRef, color→PixelSample
8. Channel: send/recv round-trip, full buffer error, drain, cross-thread
9. Signal magnitude: AudioBlock RMS, Embedding L2 norm, empty Pattern = 0
10. FrameBuffer is Send+Sync (Arc-compatible)
11. Window still renders existing SDF content (nothing broken)
