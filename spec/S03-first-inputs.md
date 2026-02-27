# L2-S03 — First Input Modules

> Three ways in: keys, cursor, and an ear to the ground.

## Goal

Build the three simplest input modules — keyboard, cursor/pixel sampler,
and audio analysis stub — and wire them through the SeedReactor. This
proves end-to-end routing: a module emits signals, the reactor delivers
them through affinity edges, and we can observe the learning in action.

## Ancestry (MAKE A BABY)

The Max/MSP patch had keyboard-driven performance (arrow keys for preset
navigation, enter for looper start/stop, spacebar for test triggers,
"p" for roll mode). We start with the same primitive: keyboard input
as the first human interface to the module system.

## Depends On

- L0-S01 (Module trait, Signal types)
- L1-S02 (SeedReactor, AffinityGraph)

## Tasks

### 3.1 Create `src/modules/keyboard_input.rs`

```rust
pub struct KeyboardInputModule {
    schema: ModuleSchema,
    pending_signals: Vec<(PortId, Signal)>,
    last_key: Option<egui::Key>,
}
```

**Schema**:
- Outputs:
  - `raw_pitch` (Float, Event) — 0.0-1.0 mapped from key 1-7
  - `trigger` (Trigger, Event) — emitted on any note key press
  - `gravity_delta` (Float, Event) — ±0.1 from up/down arrows
  - `tempo_delta` (Float, Event) — ±5.0 from left/right arrows

**Key mapping** (from old S09):

| Key | Signal | Value |
|-----|--------|-------|
| 1-7 | raw_pitch + trigger | scale degree / 7.0 |
| Space | trigger | drone toggle |
| Up/Down | gravity_delta | ±0.1 |
| Left/Right | tempo_delta | ±5.0 |
| R | trigger (raga_cycle) | cycle to next raga |
| T | trigger (tala_cycle) | cycle to next tala |
| P | trigger (panic) | kill all, reset gravity |
| Esc | trigger (stop) | stop all audio |

Feed key events from eframe's `egui::Context` input state in the
app.rs update loop.

### 3.2 Create `src/modules/cursor_input.rs`

```rust
pub struct CursorInputModule {
    schema: ModuleSchema,
    cursor_x: f32,
    cursor_y: f32,
    framebuffer_sample: Option<[f32; 4]>,
}
```

**Schema**:
- Outputs:
  - `x` (Float, Block) — normalized cursor X [0, 1]
  - `y` (Float, Block) — normalized cursor Y [0, 1]
  - `pixel` (PixelSample, Block) — RGBA at cursor position

The cursor module is the first video-adjacent module: it samples pixel
color at cursor position from the SDF framebuffer, emitting PixelSample
signals. This establishes the pattern for all future pixel-sampling
modules (PixelProbeModule in S07).

**Pixel sampling**: Read from the GPU readback buffer (the capture
pipeline already exists in 0.5's organism_renderer.rs). If readback
is not available, emit black.

### 3.3 Create `src/modules/audio_analysis.rs`

```rust
pub struct AudioAnalysisModule {
    schema: ModuleSchema,
    rms: f32,
    peak: f32,
}
```

**Schema**:
- Inputs:
  - `rms_input` (Float, Block) — fed by VoiceModule in S05
  - `peak_input` (Float, Block) — fed by VoiceModule in S05
- Outputs:
  - `rms` (Float, Block) — current RMS level
  - `peak` (Float, Block) — current peak level
  - `is_active` (Bool, Block) — true if rms > threshold

This is a stub for now — it just passes through values. In S05, the
VoiceModule will feed real audio analysis data back through the reactor,
and this module will route it to visual output modules.

### 3.4 Register with SeedReactor

In `app.rs`, create and register all three modules:

```rust
let kbd_id = reactor.register(Box::new(KeyboardInputModule::new()) as Box<dyn ModuleCore>);
let cursor_id = reactor.register(Box::new(CursorInputModule::new()) as Box<dyn ModuleCore>);
let analysis_id = reactor.register(Box::new(AudioAnalysisModule::new()) as Box<dyn ModuleCore>);
```

### 3.5 Debug logging

Add temporary debug output to verify routing:
- Log every signal emission: `[keyboard] raw_pitch = 0.428`
- Log every delivery: `[reactor] keyboard:raw_pitch → quantizer:raw_pitch (weight=0.6)`
- Log affinity changes: `[affinity] edge keyboard→quantizer: 0.5 → 0.52 (Delivery)`

This debug output validates the entire L0-L1-L2 stack before any
audio or visual modules exist.

## Files Created

```
src/modules/mod.rs               — pub mod keyboard_input, cursor_input, audio_analysis;
src/modules/keyboard_input.rs    — KeyboardInputModule
src/modules/cursor_input.rs      — CursorInputModule
src/modules/audio_analysis.rs    — AudioAnalysisModule
```

## Files Modified

```
src/main.rs                      — add `mod modules;`
src/app.rs                       — create SeedReactor, register modules,
                                   feed key events, feed cursor position
```

## Verification

1. `cargo run` — window renders, no crashes
2. Press 1-7 — debug log shows `raw_pitch` emission with correct values
3. Move mouse — debug log shows `x`, `y` emission every frame
4. Cursor pixel sampling: debug log shows RGBA values changing with position
5. Affinity edges auto-created between compatible ports
6. After 100 frames: edges between active ports strengthen (visible in log)
7. Keyboard module's emotion: moderate activity → neutral valence
8. Cursor module's emotion: constant activity → stable homeostasis
9. Audio analysis module: zero input → arousal rises → explore triggers
