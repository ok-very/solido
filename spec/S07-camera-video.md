# L2-S07 — Camera + Video Modules

> Eyes open. The system begins to see.

## Goal

Build frame capture and pixel-stream modules: CameraModule, PixelProbeModule,
and VideoFileModule. These are the visual input layer — they emit FrameRef
signals that any downstream module can consume through the affinity graph.
This establishes the pattern for all video-adjacent processing.

## Ancestry

The Max/MSP patch had no visual input — it was pure audio synthesis.
The roadmap.md envisions a full perception pipeline: camera → OpenCV →
LLM → Seed Reactor. This session builds the first two links: camera
capture and pixel-level data extraction.

The user's vision: "a cursor-over on a frame or video that streams
pixel data, long analysis through LLaVAs, with tool glyphs actually
being used by the various scripts to show patterning or data diagrams."

## Depends On

- L0-S01 (Module trait, Signal types — especially FrameRef, PixelSample)
- L1-S02 (SeedReactor)

Can run in parallel with S04-S06 (tuning/gravity).

## Tasks

### 7.1 Create `src/substrate/camera.rs`

Camera capture on a dedicated thread:

```rust
pub struct CameraThread {
    tx: ringbuf::Producer<Arc<FrameBuffer>>,
    running: Arc<AtomicBool>,
    config: CameraConfig,
}

pub struct CameraConfig {
    pub device_index: usize,
    pub width: u32,        // default: 640
    pub height: u32,       // default: 480
    pub fps: u32,          // default: 30
}
```

- Use `nokhwa` crate for cross-platform camera access
- Camera thread captures frames, wraps in `Arc<FrameBuffer>`, sends via ringbuf
- Main thread consumes via `ringbuf::Consumer<Arc<FrameBuffer>>`
- Zero-copy: FrameRef(Arc<FrameBuffer>) shared across all consumers

### 7.2 Create `src/modules/camera_module.rs`

```rust
pub struct CameraModule {
    schema: ModuleSchema,
    camera_rx: Option<ringbuf::Consumer<Arc<FrameBuffer>>>,
    last_frame: Option<Arc<FrameBuffer>>,
    motion_detector: SimpleMotionDetector,
}
```

**Schema**:
- Outputs:
  - `frame` (FrameRef, Block) — current camera frame
  - `brightness` (Float, Block) — average frame luminance [0, 1]
  - `motion` (Float, Block) — frame-to-frame difference [0, 1]

**Simple motion detector**: compare current frame brightness to
previous frame, emit magnitude of change. This is a minimal CV
signal that can drive arousal through the affinity graph.

**Custom UI panel**:
- Camera device selector dropdown
- Resolution / FPS controls
- Preview thumbnail (small egui image)
- Motion threshold slider

If no camera is available (or permission denied), the module gracefully
degrades: emits no frames, logs a warning, but doesn't crash.

### 7.3 Create `src/modules/pixel_probe.rs`

The "cursor-over-frame" pattern:

```rust
pub struct PixelProbeModule {
    schema: ModuleSchema,
    probe_x: f32,  // normalized [0, 1]
    probe_y: f32,
    last_sample: [f32; 4],
}
```

**Schema**:
- Inputs:
  - `frame` (FrameRef, Block) — from CameraModule or VideoFileModule
  - `probe_x` (Float, Block) — X position to sample (from CursorInputModule)
  - `probe_y` (Float, Block) — Y position to sample
- Outputs:
  - `pixel` (PixelSample, Block) — RGBA at probe position
  - `luminance` (Float, Block) — brightness at probe position
  - `hue` (Float, Block) — hue angle at probe position [0, 360]

PixelProbeModule receives a FrameRef on one port and cursor position
on another. It samples the pixel at that position and emits PixelSample.
The affinity graph naturally connects CursorInputModule.x → PixelProbe.probe_x.

Multiple PixelProbeModules can sample the same frame at different positions.

### 7.4 Create `src/modules/video_file_module.rs`

```rust
pub struct VideoFileModule {
    schema: ModuleSchema,
    frames: Vec<Arc<FrameBuffer>>,  // pre-loaded or streaming
    current_index: usize,
    playback_rate: f32,  // 1.0 = real-time
    looping: bool,
}
```

**Schema**:
- Inputs:
  - `playback_rate` (Float, Block) — speed control
- Outputs:
  - `frame` (FrameRef, Block) — current video frame
  - `progress` (Float, Block) — playback position [0, 1]

For the initial implementation, load a sequence of PNG frames from a
directory. Full video decoding (ffmpeg bindings) can be added later.

**Custom UI panel**:
- File/directory selector
- Play/pause/loop controls
- Playback speed slider
- Frame scrubber

### 7.5 GPU texture upload

When a FrameBuffer is first consumed by a visual output module,
upload it as a wgpu texture if `gpu_texture` is None:

```rust
impl FrameBuffer {
    pub fn ensure_gpu_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.gpu_texture.is_none() {
            // Create texture from self.pixels
        }
    }
}
```

The forked nannou_wgpu TextureBuilder helpers from S01 are used here.

### 7.6 Add dependency

```toml
nokhwa = { version = "0.10", features = ["input-native"] }
```

## Files Created

```
src/substrate/camera.rs           — CameraThread, CameraConfig
src/modules/camera_module.rs      — CameraModule (Module impl)
src/modules/pixel_probe.rs        — PixelProbeModule (Module impl)
src/modules/video_file_module.rs  — VideoFileModule (Module impl)
```

## Files Modified

```
src/substrate/mod.rs              — add pub mod camera;
src/modules/mod.rs                — add pub mod camera_module, pixel_probe, video_file_module;
src/app.rs                        — register camera/pixel/video modules with SeedReactor
Cargo.toml                        — add nokhwa
```

## Verification

1. `cargo run` — camera opens (if available), no crash if unavailable
2. Camera frames flow through reactor: debug log shows FrameRef emissions
3. Pixel probe samples color at cursor position: debug log shows RGBA values
4. Move cursor over bright area → luminance output increases
5. Camera motion: wave hand → motion output spikes
6. Camera brightness → routed to quantizer via affinity → audible pitch change
7. PixelSample hue → routed to raga_module → color influences blob tint
8. VideoFileModule: load PNG sequence → frames play back at correct rate
9. Multiple pixel probes on same frame: different positions, independent samples
10. No memory growth from frame accumulation (Arc drops correctly)
