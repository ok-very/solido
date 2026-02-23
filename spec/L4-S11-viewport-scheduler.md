# L4-S11 — Viewport Scheduler

> The eye has many rooms. Each one listens to a different part of the machine.

## Goal

Replace the single `CentralPanel` + one paint callback in `app.rs` with a
`ViewportScheduler` that owns N typed viewports inside one eframe window.
Each viewport has its own render mode, offscreen texture, and a 3-slot ring
of readback buffers. CPU threads feed state into the scheduler through
lock-free channels. The GPU sees one coordinated submit per frame tick.
Readback is pipelined N frames behind render so the loop never stalls.

## Ancestry (MAKE A BABY)

The original patch had separate Max windows for the spectral drone, the
organism field, the ASCII camera feed, and the timeline scrubber — each
polling at its own rate, loosely synced by the global transport. We replace
that with a single eframe window whose interior is tiled by the scheduler.
Each tile is a room with its own rendering voice. The scheduler is the
transport.

## Depends On

- L0-S01 (Module trait — viewport modes wrap module outputs)
- L4-S09 (BlobGpuData, blob_renderer, ISF visual module — primary consumer
  of the main organism viewport)
- L5-S10 (UX integration — panel layout decisions feed back into slot config)

## Tasks

### 11.1 Create `src/renderer/viewport.rs` — `ViewportSlot` and `Viewport`

```rust
use wgpu::{Device, Texture, TextureView, Buffer};

/// The rendering mode for a single viewport tile.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewportSlot {
    Organism,    // blob/SDF renderer (main organism field)
    Glyph,       // MSDF text / tool glyph overlay
    SdfDebug,    // raw SDF field as heatmap
    CameraFeed,  // pass-through FrameRef texture
    DataPlot,    // sparkline / data diagram (rendered to texture)
    Isf(String), // ISF shader module by name
}

/// Ring index — we keep 3 slots so frame N can read back frame N-2 safely.
pub const READBACK_RING: usize = 3;

pub struct ReadbackSlot {
    pub buffer: Buffer,
    pub width: u32,
    pub height: u32,
    /// Set to Some(frame_index) when a copy has been encoded but not yet mapped.
    pub pending_frame: Option<u64>,
}

pub struct Viewport {
    pub slot: ViewportSlot,
    /// Normalized rect within the egui window [0..1 x 0..1]; scheduler maps to
    /// physical pixels each frame.
    pub norm_rect: egui::Rect,
    /// Offscreen RGBA8 texture sized to this viewport's physical pixel dimensions.
    pub texture: Texture,
    pub texture_view: TextureView,
    pub width_px: u32,
    pub height_px: u32,
    /// Round-robin readback ring.
    pub readback_ring: [ReadbackSlot; READBACK_RING],
    pub ring_write_head: usize,
    /// Most recently fully-mapped frame data, ready for CPU consumers.
    pub last_ready: Option<(u64, Vec<u8>)>,
}

impl Viewport {
    pub fn new(
        device: &Device,
        slot: ViewportSlot,
        norm_rect: egui::Rect,
        width_px: u32,
        height_px: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_offscreen"),
            size: wgpu::Extent3d { width: width_px, height: height_px, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&Default::default());

        let bytes_per_row = align_to(width_px * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf_size = (bytes_per_row * height_px) as u64;

        let readback_ring = std::array::from_fn(|_| ReadbackSlot {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("viewport_readback"),
                size: buf_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            width: width_px,
            height: height_px,
            pending_frame: None,
        });

        Self {
            slot,
            norm_rect,
            texture,
            texture_view,
            width_px,
            height_px,
            readback_ring,
            ring_write_head: 0,
            last_ready: None,
        }
    }

    /// Encode a `copy_texture_to_buffer` for the current frame into `encoder`.
    /// Advances the write head.
    pub fn encode_readback(&mut self, encoder: &mut wgpu::CommandEncoder, frame_index: u64) {
        let head = self.ring_write_head % READBACK_RING;
        let bytes_per_row = align_to(self.width_px * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.readback_ring[head].buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(self.height_px),
                },
            },
            wgpu::Extent3d { width: self.width_px, height: self.height_px, depth_or_array_layers: 1 },
        );
        self.readback_ring[head].pending_frame = Some(frame_index);
        self.ring_write_head += 1;
    }

    /// Non-blocking: poll the slot that is 2 frames behind the write head.
    /// If mapped, collect bytes into `last_ready` and unmap.
    pub fn poll_readback(&mut self) {
        // Read from head - 2 (wrapping), giving the GPU 2 frames to finish.
        let read_head = self.ring_write_head.wrapping_sub(2) % READBACK_RING;
        let slot = &mut self.readback_ring[read_head];
        let Some(frame_index) = slot.pending_frame else { return };

        let slice = slot.buffer.slice(..);
        // Non-blocking: check if already mapped from a previous poll or device.poll().
        if let Ok(wgpu::BufferAsyncStatus::Success) = slice.get_mapped_range_future().now_or_never() {
            let data = slice.get_mapped_range().to_vec();
            slot.buffer.unmap();
            slot.pending_frame = None;
            self.last_ready = Some((frame_index, data));
        }
    }
}

fn align_to(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}
```

### 11.2 Create `src/renderer/viewport_scheduler.rs`

The scheduler owns all `Viewport`s and mediates between CPU producer
threads and the single-submit GPU render tick.

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crossbeam_channel::{Receiver, TryRecvError};
use crate::renderer::viewport::{Viewport, ViewportSlot};

/// A snapshot of state that a CPU producer thread pushes each tick.
/// Each ViewportSlot variant carries its own payload.
pub enum ViewportInput {
    Organism { blobs: Vec<BlobGpuData>, uniforms: Uniforms },
    Glyph     { glyphs: Vec<GlyphGpuData>, uniforms: Uniforms },
    SdfDebug  { uniforms: Uniforms },
    CameraFeed { frame: Arc<Vec<u8>>, width: u32, height: u32 },
    DataPlot  { values: Vec<f32> },
    Isf       { name: String, params: HashMap<String, f32> },
}

pub struct ViewportScheduler {
    pub viewports: Vec<Viewport>,
    /// One channel receiver per viewport, indexed to match `viewports`.
    pub receivers: Vec<Receiver<ViewportInput>>,
    pub frame_index: u64,
}

impl ViewportScheduler {
    pub fn new() -> Self {
        Self { viewports: vec![], receivers: vec![], frame_index: 0 }
    }

    pub fn add_viewport(
        &mut self,
        viewport: Viewport,
        rx: Receiver<ViewportInput>,
    ) {
        self.viewports.push(viewport);
        self.receivers.push(rx);
    }

    /// Called from `app.rs` update() before painting.
    /// Drains the latest input from each channel (discard stale frames).
    pub fn drain_inputs(&self) -> Vec<Option<ViewportInput>> {
        self.receivers.iter().map(|rx| {
            let mut latest = None;
            loop {
                match rx.try_recv() {
                    Ok(input) => { latest = Some(input); }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            latest
        }).collect()
    }

    /// Called from app.rs after eframe's paint callbacks have run.
    /// Encodes readback for all viewports, then polls non-blocking.
    pub fn end_of_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let mut encoder = device.create_command_encoder(&Default::default());
        for vp in &mut self.viewports {
            vp.encode_readback(&mut encoder, self.frame_index);
        }
        queue.submit(std::iter::once(encoder.finish()));
        device.poll(wgpu::Maintain::Poll);
        for vp in &mut self.viewports {
            vp.poll_readback();
        }
        self.frame_index += 1;
    }
}
```

### 11.3 Create `src/renderer/viewport_thread.rs` — CPU producer helper

A thin wrapper for spawning a CPU worker that pushes `ViewportInput`
through a channel. One per viewport that requires CPU-side computation.

```rust
use std::thread;
use crossbeam_channel::{bounded, Sender};
use crate::renderer::viewport_scheduler::ViewportInput;

pub struct ViewportThread {
    pub sender: Sender<ViewportInput>,
    handle: thread::JoinHandle<()>,
}

impl ViewportThread {
    /// Spawn a thread that calls `work_fn` in a loop, sending results.
    /// `work_fn` receives a `Sender<ViewportInput>` and a stop signal.
    pub fn spawn<F>(work_fn: F) -> Self
    where
        F: FnOnce(Sender<ViewportInput>) + Send + 'static,
    {
        let (tx, rx) = bounded(4); // small buffer — drop stale frames
        let tx_clone = tx.clone();
        let handle = thread::spawn(move || work_fn(tx_clone));
        Self { sender: tx, handle }
    }
}
```

**Example organism thread** (replaces inline uniform computation in app.rs):

```rust
ViewportThread::spawn(move |tx| {
    loop {
        // Compute new blob state on this CPU thread.
        let blobs = compute_blobs(&reactor_handle);
        let uniforms = build_uniforms(&gravity_state, time);
        let _ = tx.send(ViewportInput::Organism { blobs, uniforms });
        // Throttle to ~120Hz max; actual render rate governed by eframe.
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
});
```

### 11.4 Refactor `src/app.rs` — wire scheduler into eframe update

Replace the single `OrganismRenderResources` callback with a loop over
scheduler viewports. The key structural change:

```rust
pub struct SolidoApp {
    last_frame_time: Option<f64>,
    start_time: f64,
    recorder: Recorder,
    render_state: Option<egui_wgpu::RenderState>,
    scheduler: ViewportScheduler,          // NEW
    _threads: Vec<ViewportThread>,         // NEW — keep alive
}
```

In `update()`:

```rust
fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 1. Drain readbacks from N-2 frames ago (non-blocking).
    //    Each vp.last_ready is now available to any CPU consumer.
    for vp in &self.scheduler.viewports {
        if let Some((frame_idx, ref data)) = vp.last_ready {
            // dispatch data to whichever module/recorder cares
            self.dispatch_readback(&vp.slot, frame_idx, data);
        }
    }

    // 2. Drain latest CPU inputs.
    let inputs = self.scheduler.drain_inputs();

    // 3. Layout: allocate painter rects per viewport.
    let layout = self.compute_layout(ctx);  // returns Vec<egui::Rect>

    // 4. For each viewport: add a typed paint callback into its allocated rect.
    for (i, (vp, input)) in self.scheduler.viewports.iter()
        .zip(inputs.iter())
        .enumerate()
    {
        let rect = layout[i];
        if let Some(input) = input {
            let cb = create_viewport_callback(vp, input, rect);
            // Obtain painter for this rect and add callback.
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new(("vp", i)),
            )).add(cb);
        }
    }

    // 5. End of frame: encode readback copies + poll.
    if let Some(rs) = &self.render_state {
        self.scheduler.end_of_frame(&rs.device, &rs.queue);
    }

    ctx.request_repaint();
}
```

### 11.5 Layout system — `compute_layout`

A simple initial layout splits the window into tiles by `norm_rect`
on each viewport. The scheduler doesn't dictate layout — it asks each
viewport for its `norm_rect` and maps to the current window size:

```rust
fn compute_layout(&self, ctx: &egui::Context) -> Vec<egui::Rect> {
    let screen = ctx.input(|i| i.viewport_rect());
    self.scheduler.viewports.iter().map(|vp| {
        let r = vp.norm_rect;
        egui::Rect::from_min_max(
            egui::pos2(screen.min.x + r.min.x * screen.width(),
                       screen.min.y + r.min.y * screen.height()),
            egui::pos2(screen.min.x + r.max.x * screen.width(),
                       screen.min.y + r.max.y * screen.height()),
        )
    }).collect()
}
```

### 11.6 Recorder generalisation

The existing `Recorder` captures a single viewport. Extend it to accept
a `viewport_index` so any viewport can be independently recorded or
exported. `dispatch_readback` in app.rs routes to the appropriate recorder
instance based on `ViewportSlot`.

```rust
pub struct MultiRecorder {
    pub recorders: HashMap<usize, Recorder>,
}

impl MultiRecorder {
    pub fn push(&mut self, viewport_index: usize, frame_num: u32,
                data: Vec<u8>, width: u32, height: u32) { ... }
}
```

## Files Created

```
src/renderer/viewport.rs           — ViewportSlot, Viewport, ReadbackSlot, ring logic
src/renderer/viewport_scheduler.rs — ViewportScheduler, ViewportInput enum
src/renderer/viewport_thread.rs    — ViewportThread CPU worker helper
```

## Files Modified

```
src/renderer/mod.rs  — pub mod viewport; pub mod viewport_scheduler;
                       pub mod viewport_thread;
src/app.rs           — SolidoApp gains scheduler + _threads fields;
                       update() loop refactored per above
src/recorder.rs      — MultiRecorder wrapping existing Recorder per viewport index
```

## Verification

1. Two viewports (`Organism` + `SdfDebug`) tile side-by-side; both animate
   continuously at 60fps with no visual tearing between tiles.
2. Organism thread running on its own CPU thread; profiler shows update() not
   blocked on blob computation.
3. Readback of the `Organism` viewport yields correct RGBA bytes 2 frames
   behind the displayed frame (pixel-sample a known-color region to confirm).
4. `device.poll(Maintain::Poll)` never causes a perceptible hitch; measured
   frame time variance < 1ms across 300 frames.
5. Recorder can independently capture viewport 0 or viewport 1 without
   interfering with each other.
6. Adding a third viewport (`CameraFeed`) at runtime does not require a
   GPU device reset — only a new texture + ring buffer allocation.
7. CPU thread for `DataPlot` pushes at 30Hz; main render loop still runs
   at 60fps, showing last-available data (no stall).
8. `crossbeam_channel` bounded(4) prevents backpressure if the render
   thread is faster than a CPU producer — oldest frames are silently dropped.

## The Moment

Before S11, Solido has one voice: the organism field. After S11, the window
is a switchboard. The SDF debug tile shows the raw field the organisms live
in. The camera tile shows what the system sees. The data plot tile graphs
the arousal signal in real time. The organism tile renders the blobs
responding to all of it. Every tile is reading back to the CPU, feeding
downstream consumers — recorders, CV modules, LLaVA — on their own
threads. The screen becomes an instrument panel.
