# Video Perception — CV Analysis + Wireframe Overlay

**Status**: Spec
**Replaces**: S07 (Camera) + S08 (LLaVA) — reframed as lightweight CV instead of heavyweight VLM
**Depends on**: L0-S01 (Signal types: FrameRef), L1-S02 (SeedReactor)
**Blocks**: None (additive feature)

---

## Goal

Extract 12 real-time control signals from video at 30Hz using classical CV — no ML models, no external dependencies beyond FFmpeg for decoding. Render the analysis as animated wireframe overlays on the main canvas: edge skeletons, Delaunay mesh, optical flow particles, signal constellations. The visualization IS the analysis — each overlay layer is both a visual output and a signal source that organisms react to through the affinity graph.

---

## Architecture: Two-Layer System

### Layer A: VideoAnalysisModule (CPU, 30Hz)

Decodes video frames, downsamples to analysis resolution (160x120), extracts 12 perceptual features. Runs on a dedicated thread, delivers results via ring buffer to the control thread.

### Layer B: VideoOverlayRenderer (GPU, 60Hz)

Renders analysis results as wireframe geometry overlays on the biofield canvas. Edge lines, triangulation mesh, flow field particles, signal constellation. Togglable view filters.

Optional **Layer C: VisionContextModule** (VLM, 0.5-2Hz) — deferred. Semantic narration ("I see a forest"), feature-gated behind `llm` flag. Not part of this spec.

---

## Signal Catalog (12 outputs, 30Hz)

| Signal | Extraction | Musical Mapping | Cost |
|--------|-----------|-----------------|------|
| `brightness` | Mean luminance of frame | Master amplitude envelope | ~0ms |
| `warmth` | (R_mean - B_mean) / max | Scale mood: warm→major, cool→minor | ~0ms |
| `motion_energy` | Frame diff RMS (current vs previous) | Arousal / chaos pressure | ~0.1ms |
| `motion_x` | Weighted centroid of diff frame, X | Pan field bias / organism drift | ~0.1ms |
| `motion_y` | Weighted centroid of diff frame, Y | Pitch register bias | ~0.1ms |
| `edge_density` | Sobel magnitude mean / 255 | Harmonic density | ~0.5ms |
| `dominant_hue` | HSV histogram mode (12 bins) | Well root selection (pitch class) | ~0.2ms |
| `spatial_freq_low` | 2D FFT low-band energy ratio | Bass/drone emphasis | ~0.5ms |
| `spatial_freq_high` | 2D FFT high-band energy ratio | Treble/sparkle emphasis | ~0.5ms |
| `symmetry` | L/R half correlation coefficient | Consonance bias | ~0.1ms |
| `scene_change` | Histogram chi-squared distance | Key change / exploration trigger | ~0.1ms |
| `visual_rhythm` | Autocorrelation peak of motion_energy history | BPM suggestion | ~0.1ms |

Total budget: ~2ms per frame at 160x120. Well within 30Hz.

---

## Video Decoder

### Crate: `video-rs` (FFmpeg bindings, high-level API)

```toml
[dependencies]
video-rs = { version = "0.9", features = ["ndarray"] }
```

Supports: mp4, mkv, webm, mov, avi, gif — any FFmpeg-supported container/codec.

### VideoDecoder thread

```rust
pub struct VideoDecoder {
    path: PathBuf,
    width: u32,
    height: u32,
    fps: f32,
    frame_tx: Sender<Arc<FrameBuffer>>,
    running: Arc<AtomicBool>,
    looping: bool,
    playback_rate: f32,
}
```

- Dedicated thread decodes frames at native FPS (or adjusted by playback_rate)
- Decoded frames → `Arc<FrameBuffer>` (RGB u8 pixels) → ring buffer to analysis thread
- Looping by default (restart from beginning when file ends)
- Graceful stop via `running` flag

### FrameBuffer

```rust
pub struct FrameBuffer {
    pub pixels: Vec<u8>,     // RGB, row-major
    pub width: u32,
    pub height: u32,
    pub timestamp_ms: u64,
}
```

---

## Overlay Layers (View Filters)

### Layer 1: Edge Wireframe `[Wire]`

- Sobel edge detection on analysis frame
- Non-maximum suppression → thin edges
- Render as thin glowing line segments (1px, edge brightness → alpha)
- Upscale to viewport coordinates
- Edges pulse brighter where `motion_energy` is high (diff frame × edge frame)

### Layer 2: Triangulation Mesh `[Mesh]`

- Extract feature points: edge intersections + local maxima (Harris corners, simplified)
- Cap at ~200 points for performance
- Delaunay triangulation (incremental insertion, ~200 points is trivial)
- Render as wireframe triangles: line color = local dominant hue (sampled from source frame at triangle centroid)
- Triangle area → line alpha (small triangles = bright lines = dense detail)
- Mesh deforms each frame as feature points shift — breathing geometry

### Layer 3: Flow Field `[Flow]`

- Optical flow via block matching (16x16 blocks, search radius 8px)
- Cheaper than Lucas-Kanade, good enough for visual effect
- Render as:
  - Short oriented line segments at each block center (length ∝ velocity)
  - OR animated particles that drift along flow vectors (particle pool, ~500 particles)
- Flow convergence zones glow brighter (divergence = fade)

### Layer 4: Signal Constellation `[Signals]`

- The 12 extracted signals rendered as labeled dots on the canvas
- Position: each signal has a fixed "home" position on a circle (like the Circle of Fifths)
- Size: proportional to current signal magnitude
- Brightness: proportional to signal rate-of-change (active signals glow)
- Constellation lines: connect signals that are currently correlated (>0.5 Pearson r over last 30 frames)
- Scrolling values along the right edge of the canvas:
  ```
  brightness  0.42 ████░░░░
  motion      0.15 ██░░░░░░
  edges       0.67 ██████░░
  warmth      0.58 █████░░░
  ```

### View Filter Controls

Toggleable buttons in a small overlay bar or the controls panel:

```
[Wire] [Mesh] [Flow] [Signals] [All] [Off]
```

Each is independent — any combination of layers can be active simultaneously.
"All" activates everything at reduced opacity. "Off" hides all overlays.

---

## VideoAnalysisModule (SeedReactor Module)

```rust
pub struct VideoAnalysisModule {
    schema: ModuleSchema,
    decoder_rx: Option<Receiver<Arc<FrameBuffer>>>,
    analysis_thread: Option<JoinHandle<()>>,

    // Latest analysis results (updated at 30Hz from analysis thread)
    current: VideoFeatures,

    // Ring buffers for temporal features
    motion_history: [f32; 128],    // for visual_rhythm autocorrelation
    histogram_prev: [f32; 256],    // for scene_change detection
    frame_prev: Option<Vec<u8>>,   // for motion/flow computation
}

pub struct VideoFeatures {
    pub brightness: f32,
    pub warmth: f32,
    pub motion_energy: f32,
    pub motion_x: f32,
    pub motion_y: f32,
    pub edge_density: f32,
    pub dominant_hue: f32,
    pub spatial_freq_low: f32,
    pub spatial_freq_high: f32,
    pub symmetry: f32,
    pub scene_change: f32,
    pub visual_rhythm: f32,

    // Overlay data (passed to renderer)
    pub edge_map: Vec<u8>,           // analysis-resolution edge intensities
    pub feature_points: Vec<[f32; 2]>, // for triangulation
    pub flow_vectors: Vec<[f32; 4]>,   // [x, y, dx, dy] per block
}
```

**Schema outputs** (12 Float ports):
```
brightness, warmth, motion_energy, motion_x, motion_y,
edge_density, dominant_hue, spatial_freq_low, spatial_freq_high,
symmetry, scene_change, visual_rhythm
```

**Schema inputs**:
```
source_path (Text, Event) — video file path to load
playback_rate (Float, Block) — speed control
```

---

## Rendering Architecture

The overlay renderer lives alongside the biofield renderer in the wgpu pipeline.

### Data flow:
```
VideoDecoder thread → FrameBuffer → ring buffer
    ↓
Analysis thread (30Hz) → VideoFeatures → ring buffer
    ↓
Control thread (60Hz):
  → VideoAnalysisModule.current (signals for affinity graph)
  → VideoOverlay upload (edge_map, feature_points, flow_vectors → GPU)
    ↓
wgpu render pass:
  1. Biofield (existing)
  2. Video overlay (new pass, additive blend)
     → Edge lines shader
     → Mesh triangulation shader
     → Flow particle shader
     → Signal constellation (egui painter, not shader)
```

### GPU resources:
- Edge texture: 160x120 R8, uploaded per frame (~19KB)
- Feature point buffer: ~200 × 8 bytes = 1.6KB
- Flow vector buffer: ~80 × 16 bytes = 1.3KB
- Triangle index buffer: ~400 triangles × 6 bytes = 2.4KB
- Total: ~25KB per frame upload. Trivial.

### Shaders:
- `video_edges.wgsl` — samples edge texture, renders as screen-space glowing lines
- `video_mesh.wgsl` — renders triangle wireframe from vertex/index buffers
- `video_flow.wgsl` — renders oriented line segments or animated particles

Alternatively: all overlay rendering via egui painter (no custom shaders), which is simpler but slightly less performant for dense geometry. For ~200 triangles and ~500 flow lines, egui painter is fine.

---

## File Structure

```
src/substrate/video.rs           — VideoDecoder thread, FrameBuffer
src/modules/video_analysis.rs    — VideoAnalysisModule (Module impl + CV extraction)
src/renderer/video_overlay.rs    — Overlay rendering (edges, mesh, flow, signals)
```

Modified:
```
src/substrate/mod.rs             — pub mod video;
src/modules/mod.rs               — pub mod video_analysis;
src/renderer/mod.rs              — pub mod video_overlay;
src/app.rs                       — register module, wire overlay, view filter toggles
src/ui/panels/controls.rs        — view filter buttons (or new perception panel)
Cargo.toml                       — add video-rs
```

---

## Implementation Order

### Phase 1: Video decoder + basic features (this session)
1. `video-rs` dependency + `VideoDecoder` thread
2. `FrameBuffer` struct
3. Basic features: brightness, warmth, motion_energy, edge_density
4. `VideoAnalysisModule` registered with reactor, emitting 4 signals
5. Test with user-provided video file

### Phase 2: Full feature set + affinity routing
6. Remaining 8 signals (motion_xy, dominant_hue, spatial_freq, symmetry, scene_change, visual_rhythm)
7. Route signals through affinity graph → organisms react to visual features
8. Temporal features (motion history ring buffer, autocorrelation)

### Phase 3: Overlay renderer
9. Edge wireframe layer (egui painter or wgpu)
10. Triangulation mesh layer
11. Flow field layer
12. Signal constellation layer
13. View filter toggles

### Phase 4: Polish
14. Playback controls (pause, loop, scrub, rate)
15. File picker in settings/controls panel
16. Overlay opacity controls
17. Signal-to-organism mapping visualization in ecology graph

---

## Verification

1. `cargo run` with a test .mp4 → video decodes, features extracted, signals emitted
2. Brightness signal visible in ecology graph tooltips (if an organism consumes it)
3. Motion in video → arousal spike in organisms → chaos increases → audible change
4. Scene change → triggers key change jolt (via scene_change signal → exploration)
5. Edge wireframe overlay visible on canvas, synced to video content
6. Triangulation mesh deforms with motion, colors match video hues
7. Flow particles drift in the direction of video motion
8. Signal constellation shows all 12 features with live sparklines
9. Performance: <3ms analysis + <1ms render at 30Hz (total <4ms per frame)
10. Video formats tested: mp4, webm, gif all decode correctly
