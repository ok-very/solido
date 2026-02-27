<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# draft a full conceptual and technical spec .md

Here is the full spec — 659 lines, styled to match the existing `spec/` conventions.

***

# L5-S12 — Tala Mandala

> The chamber's face. Every ring a layer of time. Every bead a voice made visible.

## Goal

Build a concentric-ring polyrhythm display — the **Tala Mandala** — that renders the metric hierarchy of all active voices as a GPU-rendered, LLaVA-legible visual. Each ring represents one subdivision level of the tala; radial spokes mark each tick boundary; color-coded beads on spokes show which organism/voice is active at which ring+tick. The display is simultaneously a performance visualization, a rhythm-editing surface, and a pixel-space encoding that a vision model can parse from a screenshot.

***

## Ancestry (MAKE A BABY)

Andy Chamberlain's **MeTr / Metric Trees** (https://metrictrees.netlify.app) renders polyrhythmic structures as branching trees — nested cycles expressed as a dendrogram drawn with p5.js in a React app. The metric expression `"3+2*4"` from a shared Metr URL decodes to: a cycle of (3 + 2×4 = 11) ticks at the top level, with an inner grouping of two 4-groups. We take this same data structure and project it onto concentric rings: **tree depth → ring radius; sibling nodes → angular sectors**. The visual metaphor shifts from org-chart to mandala — same math, different projection.

The Max/MSP patch used color-coded GIFs (blue, red, purple, yellow) to distinguish voice groups. We honor that lineage: organism color tags carry through from `BlobGpuData.hue_shift` into the bead palette, so the same chromatic identity that drives the blob renderer drives the mandala.

Metr's source is not under a public license (its GitHub repo is not locatable under `apc518` for the Netlify deployment), so we treat it as a visual reference only and reimplement the concepts independently in wgpu + Rust.

***

## Depends On

- **L0-S01** — Module trait, Signal types (Pattern, Float, Trigger, FrameRef)
- **L1-S02** — SeedReactor, organism registry, organism color assignment
- **L3-S06** — TalaGrid (`beat_phase`, `beat_trigger`, tala definition, `divisions`)
- **L4-S05** — VoiceModule — beat activation patterns, velocity
- **L4-S09** — BlobGpuData — organism `hue_shift`, `organism_id`
- **L4-S11** — ViewportScheduler — supplies the render target / frame slot
- **L2-S08** — LLaVAModule — consumes the mandala `FrameRef` output for visual analysis

***

## Conceptual Model

### Rings

Each ring `i` (0-indexed outward to inward) represents one level of the metric tree:


| Ring | Level | Example (Teentaal 16-beat) |
| :-- | :-- | :-- |
| 0 | Full cycle | 1 tick (the whole cycle) |
| 1 | Vibhag group | 4 ticks (4 groups of 4 beats) |
| 2 | Beat | 16 ticks (16 individual beats) |
| 3 | Sub-beat | 48 ticks (triplet subdivision) |
| N | … | derived from voice patterns |

Ring count is driven by the depth of the active metric tree, currently up to 6. Additional rings appear when voices introduce subdivisions not present in the base tala.

**Radius assignment** — logarithmic spacing so outer rings are spacious and inner rings are compressed but visible:

$$
r_i = r_\text{max} \times \left(1 - \frac{\ln(i+1)}{\ln(N+2)}\right)
$$

***

### Spokes

A spoke is a radial line at a fixed angular position. The **spoke grid** is built from the LCM of all ring periodicities so that every ring's tick boundaries fall exactly on a spoke.

```
spoke_count = lcm(n_0, n_1, …, n_N)
spoke_angle_j = 2π × j / spoke_count
```

Spoke 0 is at 12 o'clock (the *sam* / cycle start). The **phase needle** — a thin arc from center to outermost ring — rotates clockwise at the current tala phase. For ring `i` with `n_i` ticks, tick `k` falls on spoke `j = k × (spoke_count / n_i)`.

***

### Beads

A bead at `(ring_i, spoke_j)` means a voice is active at phase position `j` on subdivision level `i`.


| Channel | Value | LLaVA note |
| :-- | :-- | :-- |
| **Color** | Organism hue (from `hue_shift` palette) | Stable, high-contrast, ≤8 hues |
| **Shape** | Per-organism symbol (○ ◆ ▲ □ + ✕ ⬠ …) | Redundant encoding |
| **Size** | Voice velocity / beat weight | Bigger = louder / more stressed |
| **Glyph** | 1-char organism ID (egui toggle) | Direct text for LLM |

**Multi-voice stacking** — when multiple voices share the same `(ring, spoke)`: draw a radial fan of micro-beads offset along the spoke; max 4 visible; if more, the outermost bead is replaced by a `+N` overflow glyph. Fan angle is ±3° tangentially so beads never overlap adjacent spokes.

***

### Voice Windows

A voice's activation pattern is defined by one or more **windows** on the outermost ring — contiguous arcs of spokes. The voice is active on all ticks within that arc, projecting inward to rings up to a configurable `depth`:

- `depth = 0` → outer ring only
- `depth = max` → all rings

This implements "locks several ticks": one drag gesture on the outer ring governs multiple ticks at multiple subdivision levels simultaneously.

***

## Tasks

### 12.1 Create `src/renderer/mandala/geometry.rs`

```rust
pub struct MandalaGeometry {
    pub ring_radii: Vec<f32>,
    pub ring_counts: Vec<u32>,
    pub spoke_count: u32,
    pub spoke_angles: Vec<f32>,
    pub r_max: f32,
    pub r_min: f32,
}

impl MandalaGeometry {
    pub fn build(
        tala_divisions: &[u32],
        voice_subdivisions: &[u32],
        r_max: f32,
        r_min: f32,
    ) -> Self {
        let all_counts: Vec<u32> = tala_divisions
            .iter()
            .chain(voice_subdivisions.iter())
            .cloned()
            .collect();
        let spoke_count = all_counts.iter().fold(1u32, |acc, &n| lcm(acc, n));
        let n = all_counts.len();
        let ring_radii = (0..n)
            .map(|i| {
                let t = (i as f32 + 1.0).ln() / (n as f32 + 1.0).ln();
                r_max * (1.0 - t) + r_min * t
            })
            .collect();
        let spoke_angles = (0..spoke_count)
            .map(|j| 2.0 * std::f32::consts::PI * j as f32 / spoke_count as f32)
            .collect();
        Self { ring_radii, ring_counts: all_counts, spoke_count, spoke_angles, r_max, r_min }
    }

    /// For ring i, tick k → spoke index
    pub fn spoke_for_tick(&self, ring: usize, tick: u32) -> u32 {
        tick * (self.spoke_count / self.ring_counts[ring])
    }
}

fn gcd(a: u32, b: u32) -> u32 { if b == 0 { a } else { gcd(b, a % b) } }
fn lcm(a: u32, b: u32) -> u32 { a / gcd(a, b) * b }
```


***

### 12.2 Create `src/renderer/mandala/bead.rs`

```rust
/// 32 bytes — fits 64 beads in a single 2KB push constant block.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BeadInstance {
    pub pos: [f32; 2],
    pub radius: f32,
    pub hue: f32,
    pub saturation: f32,      // desaturated when ghost/inactive
    pub shape_id: u32,        // 0=circle 1=diamond 2=triangle 3=square …
    pub glyph_start: u32,     // MSDF atlas index (0 = no label)
    pub flags: u32,           // bit0=active  bit1=coincidence_bloom  bit2=overflow
}
```


***

### 12.3 Create `src/renderer/mandala/needle.rs`

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NeedleInstance {
    pub angle: f32,           // current phase in radians (0 = sam / 12 o'clock)
    pub r_inner: f32,
    pub r_outer: f32,         // r_max + small overshoot
    pub width: f32,           // arc width in radians
    pub brightness: f32,
    pub beat_weight: f32,     // drives needle thickness pulse
    pub _pad: [f32; 2],
}
```


***

### 12.4 Create `src/renderer/mandala/window.rs`

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindowArc {
    pub angle_start: f32,
    pub angle_end: f32,
    pub hue: f32,
    pub depth: f32,          // normalized projection depth 0.0–1.0
    pub thickness: f32,      // ring-width fraction
    pub selected: u32,       // 1 if being dragged/edited
    pub _pad: [f32; 2],
}
```


***

### 12.5 Create `src/renderer/mandala/mandala_renderer.rs`

```rust
pub struct MandalaRenderer {
    geometry: MandalaGeometry,
    ring_pipeline: wgpu::RenderPipeline,
    spoke_pipeline: wgpu::RenderPipeline,
    bead_pipeline: wgpu::RenderPipeline,
    needle_pipeline: wgpu::RenderPipeline,
    window_pipeline: wgpu::RenderPipeline,
    bead_buffer: wgpu::Buffer,
    needle_buffer: wgpu::Buffer,
    window_buffer: wgpu::Buffer,
    uniforms: MandalaUniforms,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bead_instances: Vec<BeadInstance>,
    window_arcs: Vec<WindowArc>,
    max_beads: usize,   // default 1024
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MandalaUniforms {
    pub viewport: [f32; 2],
    pub center: [f32; 2],
    pub time: f32,
    pub beat_phase: f32,
    pub bloom_strength: f32,
    pub show_labels: u32,
}
```

`update()` iterates rings → ticks → active voices, builds `BeadInstance` list with fan spreading, sets coincidence bloom flags, and uploads to GPU via `queue.write_buffer`.

***

### 12.6 Create `assets/shaders/bead.wgsl`

SDF circle + per-organism hue + coincidence bloom halo:

```wgsl
fn sdf_circle(p: vec2<f32>, r: f32) -> f32 { return length(p) - r; }

@fragment
fn fs_main(@location(0) uv: vec2<f32>, @location(1) hue: f32,
           @location(2) saturation: f32, @location(3) flags: u32) -> @location(0) vec4<f32> {
    let d = sdf_circle(uv, 1.0);
    if d > 0.05 { discard; }
    let color = hsl_to_rgb(hue, saturation, 0.55);
    var alpha = smoothstep(0.05, -0.05, d);
    if (flags & 2u) != 0u {
        let halo = exp(-max(d, 0.0) * 8.0) * 0.6;
        return vec4(color + vec3(halo), alpha + halo);
    }
    if (flags & 1u) == 0u { alpha *= 0.3; } // ghost
    return vec4(color, alpha);
}
```


***

### 12.7 Create `assets/shaders/ring.wgsl`

```wgsl
fn sdf_annulus(p: vec2<f32>, r: f32, thickness: f32) -> f32 {
    return abs(length(p) - r) - thickness * 0.5;
}

@fragment
fn fs_main_ring(@location(0) local_pos: vec2<f32>, @location(1) radius: f32) -> @location(0) vec4<f32> {
    let d = sdf_annulus(local_pos, radius, RING_STROKE);
    let alpha = smoothstep(0.008, -0.008, d);
    return vec4(RING_COLOR, alpha * RING_ALPHA);
}
```

Sam spoke (spoke 0) rendered at 2× width with `RING_COLOR` → gold.

***

### 12.8 Create `src/modules/mandala_module.rs`

```rust
pub struct MandalaModule {
    schema: ModuleSchema,
    renderer: MandalaRenderer,
    voice_states: Vec<VoiceState>,
    frame_out: Option<Arc<FrameBuffer>>,
}
```

**Schema inputs**: `beat_phase` (Float, Block), `beat_weight` (Float, Event), `beat_trigger` (Trigger, Event), `voice_pattern` (Pattern, Block), `organism_hue` (Float, Block)

**Schema outputs**:

- `frame` (FrameRef, Block) → LLaVAModule
- `coincidence` (Float, Block) — rises when ≥2 organisms share a tick → routes into arousal, gamaka depth, or gravity

***

### 12.9 Create `src/modules/voice_state.rs`

```rust
pub struct VoiceState {
    pub organism_id: u32,
    pub hue: f32,
    pub shape_id: u32,
    pub glyph_start: u32,
    pub velocity: f32,
    pub active: bool,
    pub masks: Vec<FixedBitSet>,        // masks[ring_i].contains(tick_k)
    pub windows: Vec<(u32, u32)>,       // (spoke_start, spoke_end) pairs on outer ring
    pub window_depth: u32,
}

impl VoiceState {
    pub fn mask_at(&self, ring: usize, tick: u32) -> bool {
        self.masks.get(ring).map(|m| m.contains(tick as usize)).unwrap_or(false)
    }

    pub fn rebuild_masks(&mut self, geometry: &MandalaGeometry) {
        for (i, mask) in self.masks.iter_mut().enumerate() {
            mask.clear();
            if i as u32 > self.window_depth { continue; }
            let n = geometry.ring_counts[i];
            let stride = geometry.spoke_count / n;
            for tick in 0..n {
                let spoke = tick * stride;
                for &(start, end) in &self.windows {
                    if spoke_in_window(spoke, start, end, geometry.spoke_count) {
                        mask.insert(tick as usize);
                        break;
                    }
                }
            }
        }
    }
}
```


***

### 12.10 LLaVA-readability conventions

The mandala is designed so a vision model receiving a single screenshot can extract structured information without prior context:

1. **Fixed legend panel** (bottom-left, 120×200px) — one row per active organism: color swatch + shape symbol + 1-char label. Rendered via existing MSDF + egui system.
2. **Stable geometry** — center, `r_max`, ring count, spoke grid never change mid-render.
3. **Color palette** — organism hues spaced ≥45° apart in HSL, never mid-gray. Maximum 8 simultaneous organisms.
4. **Redundant encoding** — color + shape + optional glyph all carry organism identity.
5. **Sam marker** — spoke 0 always has a thin gold radial from center to outer edge.
6. **Structured VQA prompt** (wired into S08):
```
"Describe the concentric ring diagram. For each ring (outer=0), list which
colors of beads are present and at which clock positions (12=top, clockwise).
Note any positions where multiple colors coincide."
```

The structured response is parseable into per-ring activation vectors and fed back into the affinity graph as a `Text` signal.

***

### 12.11 Interaction — outer ring editing

Handled in the existing UX layer (S10). Two modes:

**View mode** (default): hover bead → tooltip (organism name, ring level, beat index, velocity). Needle read-only.

**Edit mode** (toggle `E`):

- **Drag outer ring** → creates/extends a window arc for the selected organism
- **Drag window edge** → resize
- **Drag window body** → rotate (shifts voice phase)
- **Double-click window** → depth slider
- **Right-click window** → delete

All edits call `VoiceState::rebuild_masks()` immediately.

***

### 12.11a Organism Panel Integration

The Organism Panel (`src/ui/panels/organism_panel.rs`) scaffolds the control surface
for VoiceState editing. `OrganismUiState` holds:

- `shape_id` — maps to BeadInstance.shape_id for bead rendering
- `mixer_mute` — organism active/inactive state (maps to VoiceState.active)
- `hue` — organism color identity (maps to BeadInstance.hue)

Future S12a fields to add to OrganismUiState:
- `window_depth: u32` — projection depth for voice windows
- `windows: Vec<(u32, u32)>` — outer ring spoke ranges

The organism panel is the S10 inspector foundation. S12a's edit mode
(§12.11) extends it with window drag controls.

***

### 12.12 Performance budget

| Element | Count | GPU cost |
| :-- | :-- | :-- |
| Ring outlines | 4–6 | 1 draw call (instanced) |
| Spoke tick marks | 48–192 | 1 draw call (instanced) |
| Beads | 64–512 | 1 draw call (instanced) |
| Voice window arcs | 4–16 | 1 draw call (instanced) |
| Phase needle | 1 | merged into ring draw |
| Legend (MSDF) | 8 rows | existing MSDF pipeline |

**Total: 4–5 draw calls, <0.3ms at 1440p.** The mandala renders into a dedicated 512×512 offscreen texture that composites into the main viewport *and* is exposed as a FrameRef for LLaVA — no extra readback needed.

***

## Files Created

```
src/renderer/mandala/geometry.rs         — MandalaGeometry, lcm/gcd, polar_to_ndc
src/renderer/mandala/bead.rs             — BeadInstance, fan_spread
src/renderer/mandala/needle.rs           — NeedleInstance
src/renderer/mandala/window.rs           — WindowArc
src/renderer/mandala/mandala_renderer.rs — MandalaRenderer, MandalaUniforms
src/renderer/mandala/mod.rs              — pub mod re-exports
src/modules/mandala_module.rs            — MandalaModule (Module impl)
src/modules/voice_state.rs               — VoiceState, rebuild_masks
assets/shaders/bead.wgsl                 — bead SDF + bloom
assets/shaders/ring.wgsl                 — ring outline + tick marks
assets/shaders/needle.wgsl               — phase needle
assets/shaders/window_arc.wgsl           — voice window arcs
```


## Files Modified

```
src/renderer/mod.rs             — pub mod mandala;
src/modules/mod.rs              — pub mod mandala_module, voice_state;
src/modules/llava_module.rs     — mandala_mode VQA prompt + structured response parsing
src/app.rs                      — register MandalaModule; wire TalaModule + VoiceModules
spec/L2-S08-llava-vision.md     — add mandala_mode prompt variant (back-reference)
```


***

## Verification

1. Teentaal at 120bpm: 3 rings visible; needle completes one revolution per 2s
2. Rings `[16, 4, 3]`: `spoke_count = 48`; ring-0 ticks every 3 spokes; ring-2 every 16
3. Single voice, window 0°–90°: beads on first quarter of each ring only
4. Two voices same spoke: fan of 2 beads; no bleed onto adjacent spokes
5. Five voices same spoke: 4 beads + "+1" glyph
6. `coincidence` signal rises → audible gamaka deepens or arousal increases
7. Bloom halo visible at coincidence positions
8. Sam marker (gold spoke) stable at 12 o'clock across all tala types
9. Edit mode: drag outer ring → window arc appears; inner beads update in real time
10. `window_depth=0`: only outer ring beads for that voice
11. `window_depth=max`: all rings respond to window
12. LLaVA path: `FrameRef` arrives; structured description includes ring-level bead colors
13. Legend matches hues on rings
14. 60fps maintained with 6 rings, 8 voices, 512 beads
15. No GPU artifacts; SDF edges anti-aliased; no z-fighting

***

## The Moment

Before S12, rhythm in solido is audible but invisible — you hear the tala, you hear the voices interleave, but you cannot *see* the structure at a glance. After S12, the mandala makes the whole rhythmic topology visible in a single frame. A performer sees which organisms are aligned, which are offset, which windows are dense. LLaVA describes the pattern in words and feeds that description back into the gravity system. The system begins to reflect on its own rhythmic structure — and change in response to what it sees.

***

Want me to push this directly to `spec/L5-S12-tala-mandala.md` in `ok-very/solido`?

