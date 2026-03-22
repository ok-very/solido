# Video Substrate — Living Visual Ecology

**Status**: Spec
**Depends on**: Video Perception (Phase 1 complete), S36-S39 Ecology Arc, Nutrient System
**Blocks**: None (additive — existing system works without video)

---

## Goal

Video frames become the world — the living substrate that organisms inhabit, feed on, and transform. The video IS the biome. Organisms consume pixel energy as they move, leaving bioluminescent trails. The original image dissolves into the organisms' interpretation of it. Sound emerges as the byproduct of visual digestion.

---

## Architecture

### Rendering Pipeline

**Current:**
```
BioField pass: organisms → intermediate RGBA16Float
Composite pass: checkerboard_bg + biofield_organisms + post-fx → screen
```

**Proposed:**
```
Video upload:   CPU FrameBuffer → GPU video_texture (RGB8, per-frame)
BioField pass:  organisms → intermediate RGBA16Float (unchanged)
Substrate pass: video_texture + trail_texture → substrate (gaussian splat + depletion)
Composite pass: substrate_bg + biofield_organisms + post-fx → screen
```

The checkerboard in `composite.wgsl` is replaced by a substrate texture that blends video pixels with organism trail deposits. Organisms render on top via premultiplied alpha (existing compositing, unchanged).

### Video Texture Upload

- `VideoDecoder` delivers `Arc<FrameBuffer>` (RGB24, 160×max) at 30fps (existing)
- New `VideoSubstrate` struct: holds `wgpu::Texture` (RGBA8, analysis-resolution)
- Per-frame: upload latest FrameBuffer pixels via `queue.write_texture()`
- Gaussian splat upsampling in shader: each analysis pixel → soft radial blob (sigma ≈ viewport_width / analysis_width / 2). At 160px into 1920px, each pixel ≈ 6px radius splat. Organic, painterly substrate.
- Higher-res video → smaller splats → denser energy → richer biome

### Substrate Energy Model

Persistent GPU texture (`RGBA16Float`, full viewport resolution):

```
R,G,B = video color energy (replenished by video, depleted by organisms)
A     = depletion mask (1.0 = full energy, 0.0 = fully consumed)
```

**Replenishment** (per video frame):
```
substrate[px] = lerp(substrate[px], video_splat[px], refresh_rate)
```
- `refresh_rate` scales with video FPS
- Low FPS video → slow replenishment → organisms outpace regeneration
- No video → substrate decays to black → organisms starve

**Depletion** (per organism, per physics tick):
- Sample energy grid at organism position (block-based)
- Drain proportional to `node_absorption_rate × species_appetite`
- Upload depletion changes to GPU substrate alpha channel
- Depleted areas → organism trail color shows through (alpha blend replace)

**Carrying capacity**: `pixel_count × refresh_rate × mean_brightness`

### CPU-Side Energy Grid

Block-based grid (16×16 pixel blocks → ~120×68 cells at 1920×1080):
- Authoritative energy state lives on CPU
- Updated by video frames (replenishment) and organism positions (depletion)
- Organisms sample grid cells for nutrition and local perception
- GPU renders from this grid however looks best (no readback needed)
- We own both sides of the pipeline

### Organism Sight (Local Perception)

Each organism perceives its LOCAL environment, not global averages:
- Sample energy grid in a radius around organism position
- DNA `sight_radius` and `sight_sensitivity` determine perception range
- The 4 features (brightness, warmth, motion, edge) computed locally per organism
- Different species see different things (DNA-driven feature preference)
- The membrane IS the retina (connects to membrane sight variant)

### Well Lens Optics

Gravity wells become convex lenses focusing the substrate:

```wgsl
fn well_lens(uv: vec2f, well_pos: vec2f, well_radius: f32) -> vec2f {
    let offset = uv - well_pos;
    let dist = length(offset);
    let t = clamp(dist / well_radius, 0.0, 1.0);
    let focus = mix(0.3, 1.0, t * t);  // 3.3× magnification at center
    return well_pos + offset * focus;
}
```

Pixels near well center sampled from wider area → energy concentration. Wells become oases.

### Trail-Substrate Interaction

Alpha blend replace: smooth crossfade from video to organism trail color in depleted areas. Video structure visible through partial depletion, fully replaced where thoroughly consumed.

- Trail persistence via existing `rd_feed`/`rd_kill`/`rd_reactivity` DNA params
- If organisms leave, video energy replenishes, overwriting old trails
- Dynamic equilibrium: graze → trails appear → video replenishes → trails fade

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Substrate resolution | Full viewport | GPU texture memory abundant; shader is compute-bound not memory-bound |
| Sight model | CPU-side block grid | Authoritative state on CPU, no GPU readback; organisms see triangulated visual blocks |
| Trail mode | Alpha blend replace | Smooth crossfade, no dark gap between depletion and trail fill |

---

## Implementation Phases

### Phase 1: Video Background
- Upload video texture to GPU
- Replace checkerboard with gaussian-splat video in composite shader
- No energy model — just visual. Organisms float on top of video.

### Phase 2: Substrate Energy
- CPU-side energy grid (block-based)
- Persistent substrate texture with depletion + replenishment
- Organisms drain energy → alpha decreases → trails show through
- Carrying capacity emerges naturally

### Phase 3: Local Sight
- Per-organism local feature sampling from energy grid
- DNA-driven sight parameters
- video_cv_cell reads local features instead of global averages

### Phase 4: Well Lenses
- UV warp in composite shader near well positions
- Energy focusing → wells become high-energy zones

### Phase 5: Ecosystem Tuning
- Depletion vs replenishment balance
- Species appetite profiles (DRON grazes slowly, ACID aggressively)
- Scarcity → wanderlust triggers
- Visual polish

---

## Critical Files

| File | Change |
|------|--------|
| `src/renderer/composite.wgsl` | Replace checkerboard with substrate sampling + well lens UV warp |
| `src/renderer/biofield_renderer.rs` | Video texture + substrate texture GPU resources, upload pipeline |
| `src/substrate/video.rs` | Expose latest frame for GPU upload |
| `src/modules/video_analysis.rs` | Per-organism local feature sampling |
| `src/organism/registry.rs` | Substrate energy as nutrient source |
| `src/organism/sim.rs` | Substrate depletion at organism position |
| `src/app.rs` | Wire video texture upload, energy grid, per-organism sight |

---

## Verification

1. Phase 1: `cargo run` with video → video visible behind organisms as splattered background
2. Phase 2: organisms leave visible trails where video fades → smooth crossfade
3. Phase 3: organisms react differently to local substrate (ACID chases motion-rich areas)
4. Phase 4: wells concentrate substrate → organisms cluster at focused zones
5. Phase 5: stable ecosystem — depletion/replenishment equilibrium, wanderlust when local area depleted
