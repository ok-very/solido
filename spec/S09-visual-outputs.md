# S09 — Visual Outputs + Organism Sim

> Organisms become soft, amoeba-like bodies that move, interact, merge, and express
> themselves on a 2D plane rendered via GPU SDF blending. The blobs learn to show
> what they hear. And see. And think.

**Layer**: L4–L5
**Depends on**: S01 (module contract), S02 (AffinityGraph), S04 (PitchGravity), S06 (TalaGrid)
**Status**: Prospect

## Goal

Build the blob renderer (circle SDF → multi-lobe metaballs with smin merging), the
organism simulation (soft bodies with pseudopods, interaction physics, fusion), the
gravity-to-visual bridge (emotion → edge sharpness, arousal → glow), and visual output
modules (tool glyphs, data diagrams, ASCII textures, ISF shaders).

This session makes organisms **visible**. S12 makes them **audible**. S13 wires both
together. S09 and S12 can run in parallel — visual and audio paths are independent
until S13.

DNA schema is defined in S12. This spec references the `BodyDna`, `RenderDna`, and
`PhysicsDna` sections of the unified `OrganismDna`.

---

## The Organism Body: Lobes

Each organism has between 1 and 12 **lobes** — circle metaballs with a position
offset relative to the organism centroid and an independent radius. Default lobe
count is 6, DNA-selectable.

Lobes serve two purposes:

- **Core lobes** (2–3) form the stable body mass. They stay close to the centroid.
- **Pseudopod lobes** (remaining) extend toward a driving direction — heading, a
  gradient, a signal — and retract when the organism turns or stops. This produces
  amoeba-like silhouette changes without meshes.

### Lobe Simulation Rules

- Each lobe has a target offset and target radius. Actual values lerp toward targets
  each frame at rates set by `extension_speed` and `retraction_speed` in DNA.
- The leading pseudopod lobe extends along `heading * pseudopod_gain * energy`.
- Trailing lobes retract toward the core radius.
- Unused lobe slots (index >= `lobe_count`) have radius 0 and are skipped by the shader.
- Extension amplitude modulated by `energy` (0 = contracted, 1 = fully extended).

### Organism Simulation State

```rust
pub struct OrganismState {
    pub id: OrganismId,
    pub dna: OrganismDna,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub heading: f32,
    pub energy: f32,
    pub lobes: Vec<LobeState>,
    pub consent_flags: u8,
    // Interaction tracking
    pub active_tethers: Vec<TetherId>,
    pub glob_group: Option<GlobGroupId>,
    pub integrate_timers: HashMap<OrganismId, f32>,
}

pub struct LobeState {
    pub offset: [f32; 2],        // current offset from centroid
    pub radius: f32,             // current radius
    pub target_offset: [f32; 2], // lerp target
    pub target_radius: f32,      // lerp target
}
```

---

## Rendering

### SDF Field Composition

The renderer evaluates a per-pixel SDF field for all lobes of all organisms.

**Per organism**: lobes blended using **smooth-minimum (`smin`)** with the organism's
own `smin_k` from DNA. Produces the characteristic soft-merge silhouette between
adjacent lobes of the same body.

**Across organisms**: by default, composited with hard `min()` (no visual merge).
When two organisms are in **glob mode**, a shared `cross_smin_k` (average of both
organisms' `smin_k`) produces the appearance of bodies flowing together.

```
smin(a, b, k) = min(a,b) - max(k - |a-b|, 0)^2 / (4k)
```

### Per-Organism Shader Parameters

Each organism passes the following to the shader per frame:

| Parameter | Source | Effect |
|-----------|--------|--------|
| `smin_k` | DNA `render.smin_k` | Intra-organism lobe blend softness |
| `edge_softness` | DNA / gravity state | SDF AA band width |
| `thermal_temp` | Emotion arousal | Position in thermal palette |
| `hue_shift` | DNA base hue + valence | Hue rotation on thermal color |
| `glow` | DNA base glow + arousal | Halo intensity outside SDF boundary |
| `pulse_phase` | Beat phase (tala grid) | Beat-sync scale oscillation |
| `pulse_amp` | DNA `render.pulse_response` | Breathing amplitude with beat |
| `glyph_start/count` | Glyph buffer | MSDF text overlay (separate from blob SDF) |

### Shading Pipeline (per fragment)

1. Evaluate organism's lobe SDFs, blend with `smin` -> `d`
2. Compute edge fill: `smoothstep(0, edge_softness, d)` -> `fill`
3. Compute glow halo: `exp(-max(d, 0) * falloff) * glow * arousal`
4. Apply thermal palette at `thermal_temp`, hue-rotate by `hue_shift`
5. Composite glyph overlay (unchanged MSDF pipeline)
6. Background: checkerboard (existing)

### GPU Data Structures

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlobOrgData {
    pub pos: [f32; 2],
    pub smin_k: f32,
    pub edge_softness: f32,
    pub thermal_temp: f32,
    pub hue_shift: f32,
    pub pulse_phase: f32,
    pub pulse_amp: f32,
    pub glow: f32,
    pub lobe_start: u32,      // index into lobe buffer
    pub lobe_count: u32,
    pub _pad: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LobeGpu {
    pub offset: [f32; 2],
    pub radius: f32,
    pub _pad: f32,
}

pub struct BlobUniforms {
    pub viewport: [f32; 2],
    pub time: f32,
    pub organism_count: f32,
    pub dpr: f32,
    pub beat_phase: f32,
    pub gravity_strength: f32,
    pub _pad: f32,
}
```

### WGSL Shader Primitives

```wgsl
fn sdCircle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * 0.25 * k;
}

fn thermal_palette(t: f32) -> vec3<f32> {
    // 8-stop: black -> indigo -> blue -> cyan -> green -> yellow -> orange -> white
    let colors = array<vec3<f32>, 8>(
        vec3(0.0, 0.0, 0.0),
        vec3(0.18, 0.0, 0.35),
        vec3(0.0, 0.0, 0.8),
        vec3(0.0, 0.7, 0.9),
        vec3(0.1, 0.8, 0.2),
        vec3(1.0, 0.95, 0.2),
        vec3(1.0, 0.5, 0.0),
        vec3(1.0, 1.0, 1.0),
    );
    let idx = t * 7.0;
    let i = u32(floor(idx));
    let f = fract(idx);
    return mix(colors[i], colors[min(i + 1u, 7u)], f);
}
```

---

## Gravity State (Emotion-to-Visual Bridge)

```rust
pub struct GravityState {
    pub pitch_gravity: f32,
    pub rhythm_gravity: f32,
    pub gamaka_depth: f32,
    pub morph_speed: f32,
}

impl GravityState {
    pub fn from_emotion(emotion: &ModuleEmotion) -> Self {
        let base_gravity = emotion.valence * 0.5 + 0.5;
        let arousal_pull = emotion.arousal * 0.6;
        Self {
            pitch_gravity: (base_gravity - arousal_pull).clamp(0.0, 1.0),
            rhythm_gravity: (base_gravity - arousal_pull * 0.5).clamp(0.0, 1.0),
            gamaka_depth: emotion.arousal.clamp(0.0, 1.0),
            morph_speed: (-emotion.valence * 0.5 + 0.5).clamp(0.1, 2.0),
        }
    }
}
```

| Emotion State | Gravity | Sound | Visual |
|---------------|---------|-------|--------|
| Calm, positive | high | Locked raga, clean tala | Sharp blob edges, cool colors |
| Slightly aroused | medium | Subtle gamaka, slight swing | Softening edges, warming |
| High arousal | low | Pitch drifts, rhythm dissolves | Soft glowing blobs, hot palette |
| Panic | zero | Free spectral drone — pure texture | Diffuse glow, white-hot |

---

## Interaction Physics

Interactions are evaluated every frame per neighboring organism pair. Each organism's
`interaction_rules` (from `PhysicsDna`) are checked against neighbors.

### Tag Matching

- Exact match on neighbor's `species`
- Wildcard `"*"` matches all
- Match on any entry in neighbor's `affinity_tags`
- If `affinity_threshold` is set, requires runtime affinity >= threshold

### Interaction Modes

| Mode | Behavior | Continuous |
|------|----------|------------|
| `Repel` | Outward force, `(1 - dist/range)^2` scaling | yes |
| `Bounce` | Repel + velocity projection onto normal + friction | yes |
| `Slow` | Viscous drag on relative velocity proportional to overlap | yes |
| `Attach` | Spring force toward `rest_length`; tether persists until break | yes |
| `Glob` | Mid-band attraction + high viscosity + centroid pull | yes |
| `IntegratePropose` | Accumulate dwell timer; fire fusion event at threshold | CPU event |

Multiple rules can match simultaneously — all apply additively except IntegratePropose
which fires a single event.

### Attach Breakup Conditions

A tether snaps on **any** of:
- Distance exceeds `break_distance`
- Spring force exceeds `break_force`
- Antagonistic rule fires simultaneously (`break_on_repel: true`)
- Affinity drops below `affinity_threshold`
- Consent cleared via egui

### Glob Mode

When organisms share a glob group:

```
total_impulse = sum(member.velocity * weight(member))
median_vel = total_impulse / sum(weights)
```

Each member pulled toward `median_vel` with `strength` damping. Visual: renderer
switches cross-organism SDF from hard `min()` to `smin(cross_smin_k)`.

---

## Integration (Fusion) Pipeline

Integration is the only event that destroys two organisms and creates one.
Opt-in, bilateral, consent-gated.

### Trigger

1. A and B satisfy IntegratePropose tag/affinity conditions
2. Within `range` for `dwell_secs` continuously
3. Both have `consent_flags bit 0 == 1` (computed from DNA — has IntegratePropose rule)

### DNA Merge (interim — genetic rules TBD)

- `seed`: new random
- `species`: keep higher-energy organism's species
- `affinity_tags`: union
- `lobe_count`: `max(A, B)`
- `core_radius`: `sqrt(A^2 + B^2)` (area-conserving)
- Numeric `render`/`physics` params: energy-weighted average
- `cells`: union of both cell lists
- `interaction_rules`: union; duplicate species+mode pairs averaged

New organism C spawns at centroid of A and B. A and B despawned.

---

## Visual Output Modules

### ToolGlyphModule

```rust
pub struct ToolGlyphModule {
    schema: ModuleSchema,
    glyph_buffer: Vec<char>,
    pattern_source: Vec<f32>,
}
```

Inputs: `pattern` (Pattern), `text` (Text). Outputs: `glyphs` (Pattern).
Reuses existing font_atlas + MSDF system for dynamic glyph sequences.

### DataDiagramModule

```rust
pub struct DataDiagramModule {
    schema: ModuleSchema,
    history: VecDeque<f32>,
    history_capacity: usize,
}
```

Input: `value` (Float). Custom egui panel: sparkline graph, min/max/current.

### AsciiTextureModule

```rust
pub struct AsciiTextureModule {
    schema: ModuleSchema,
    char_grid: Vec<Vec<char>>,
    grid_width: usize,
    grid_height: usize,
}
```

Inputs: `frame` (FrameRef), `pattern` (Pattern).
Maps luminance to character density: ` .:-=+*#%@`.

### IsfVisualModule

```rust
pub struct IsfVisualModule {
    schema: ModuleSchema,
    isf_shader: IsfShader,
    param_values: HashMap<String, f32>,
    output_texture: Option<wgpu::Texture>,
}
```

Schema auto-generated from ISF shader header. Each ISF input param becomes a typed
affinity port. Drop a new ISF shader file → system auto-generates a Module with
matching ports → affinity edges form.

---

## Render Handles (egui, live-editable per organism)

### Render handles
- `smin_k` — slider [0.05 .. 2.0]
- `edge_softness` — slider [0.5 .. 10.0]
- `glow` — slider [0.0 .. 1.0]
- `hue` — hue wheel [0 .. 1]
- `palette_variant` — dropdown
- `pulse_response` — slider [0.0 .. 1.0]
- `thermal_enabled` — toggle

### Physics handles
- `drag`, `max_speed`, `mass` — sliders
- `interaction_rules` — table view (add, remove, edit inline)
- `consent_flags bit 0` — checkbox (session override)

### Body handles
- `lobe_count` — int slider [1 .. 12] (lobes fade in/out)
- `pseudopod_gain` — slider [0 .. 1]
- `extension_speed`, `retraction_speed` — sliders

### Session actions
- **Clone** — spawn copy with same DNA at offset position
- **Save DNA** — export current DNA to JSON
- **Load DNA** — replace DNA, recompute consent, respawn lobes
- **Kill** — despawn immediately
- **Propose Integrate** — manual fusion trigger (disabled if not consent-eligible)

---

## Reactor Integration (Load Scaling)

Frame time monitoring adjusts spawn pressure:
- Frame time > target: suppress new spawns
- Consistently 2x target: reduce `lobe_count` globally by 1 (floor: 1)
- Recovery: restore toward DNA defaults, +1 per second
- LOD changes smooth (lobes fade radius to 0 over 0.3s)

## World Boundary

Soft wall constraint layer (separate from interaction rules). Inward pressure force
near boundary. Shape configurable (rect, circle, custom SDF). Does not fire
ContactEvents or appear in interaction table.

---

## Files Created

```
src/organism/sim.rs               — OrganismState, LobeState, OrganismSim::tick()
src/organism/interaction.rs       — ContactEvent, DetachReason, interaction modes
src/organism/registry.rs          — OrganismRegistry: spawn/despawn/save/load/build_gpu_payload
src/renderer/blob_renderer.rs     — BlobOrgData, LobeGpu, BlobUniforms, BlobCallback
src/renderer/blob.wgsl            — circle SDF, smin, lobe loop, thermal palette, glow, hue
src/tuning/gravity_control.rs     — GravityState, from_emotion mapping
src/modules/tool_glyph_module.rs
src/modules/data_diagram_module.rs
src/modules/ascii_texture_module.rs
src/modules/isf_visual_module.rs
```

## Files Modified

```
src/renderer/mod.rs               — add pub mod blob_renderer
src/tuning/mod.rs                 — pub mod gravity_control
src/modules/mod.rs                — add visual module mods
src/app.rs                        — gravity state -> uniforms, OrganismSim::tick(), BlobCallback
src/reactor/mod.rs                — handle IntegrateProposal, Spawn, Despawn events
```

---

## Implementation Steps

### Step 1: GravityState (`src/tuning/gravity_control.rs`)
Emotion-to-gravity mapping. Pure computation, no dependencies.

### Step 2: Blob renderer refactor (`src/renderer/blob_renderer.rs`)
Replace L-shaped organisms with circle SDF + smin merging. BlobOrgData, LobeGpu,
BlobUniforms GPU data structures. BlobCallback paint callback.

### Step 3: blob.wgsl shader
sdCircle, smin, thermal_palette, beat pulse, edge softness, glow halo.
Per-organism lobe loop with smin blending.

### Step 4: Organism simulation (`src/organism/sim.rs`)
OrganismState, LobeState, lobe simulation (extension/retraction lerp).
CPU-side, runs at frame rate.

### Step 5: Interaction physics (`src/organism/interaction.rs`)
Repel, Bounce, Slow, Attach, Glob, IntegratePropose. Tag matching.
Tether tracking and breakup conditions.

### Step 6: Organism registry (`src/organism/registry.rs`)
Spawn/despawn/save/load. Build GPU payload (BlobOrgData + LobeGpu arrays).
Glob group computation.

### Step 7: Integration pipeline
Fusion trigger, DNA merge, consent model. Wire into reactor.

### Step 8: Visual output modules
ToolGlyphModule, DataDiagramModule, AsciiTextureModule, IsfVisualModule.

### Step 9: App integration
Wire OrganismSim::tick() into app loop. Feed gravity state + beat phase into
shader uniforms. Build BlobOrgData per organism per frame.

### Step 10: egui handles
Per-organism inspector: render/physics/body handles, session actions.

---

## Verification

1. Single organism with 6 lobes displays amoeba-like silhouette that changes as it moves
2. `lobe_count` change makes lobes appear/disappear smoothly (radius fade)
3. Two organisms in Glob mode visually merge (smin blend) and co-move
4. Repel pushes apart; Bounce preserves tangential velocity
5. Attach creates tether; overstretching breaks it
6. Fusion-eligible pair dwells → integration fires → C spawns, A+B despawn
7. Non-eligible organism never fires integration regardless of dwell
8. DNA saved to JSON reloads as visually similar organism
9. Frame time spike reduces lobe count; recovery restores it
10. Glyph overlay renders on new blob SDF body
11. Blob edges soften when gravity drops, sharpen when gravity rises
12. Blobs pulse with tala beat
13. High arousal → brighter, warmer colors; low valence → cooler tones
14. Tool glyphs: connect audio_analysis → dynamic text on blobs
15. ISF shader: load test ISF → params auto-wire to nearby signals
16. smin merging: organisms with strong affinity edges visually merge
17. Performance: 60fps with 12 organisms
