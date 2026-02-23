# L4-S09 — Visual Output Modules

> The blobs learn to show what they hear. And see. And think.

## Goal

Build tool glyphs, data diagrams, ASCII textures, ISF shader modules,
and the audio↔visual bridge as output modules. Wire gravity/affinity
state into the blob SDF renderer so organisms visually respond to the
entire system. This is where the project becomes audiovisual and where
the ISF-as-module pattern comes alive.

## Ancestry (MAKE A BABY)

The Max/MSP patch had `multiSlider` displays and color-coded GIFs
(blue, red, purple, yellow) to represent different voice groups.
We replace that with the thermal SDF shader from the emotive color
system plan: SDF depth → thermal palette, with arousal driving
overall temperature. And we go further: tool glyphs, data diagrams,
and ASCII textures make the data visible on the blobs themselves.

## Depends On

- L0-S01 (Module trait, ISF parser)
- L1-S02 (SeedReactor, AffinityGraph emotions)
- L2-S07 (FrameRef for ASCII texture input) — can start without, using audio signals
- L3-S04 (PitchGravity state)
- L3-S06 (TalaGrid beat events)

## Tasks

### 9.1 Create `src/tuning/gravity_control.rs` — GravityState

The emotion-to-gravity mapping from the microtonal plan:

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
        let pitch_gravity = (base_gravity - arousal_pull).clamp(0.0, 1.0);
        let rhythm_gravity = (base_gravity - arousal_pull * 0.5).clamp(0.0, 1.0);
        let gamaka_depth = emotion.arousal.clamp(0.0, 1.0);
        let morph_speed = (-emotion.valence * 0.5 + 0.5).clamp(0.1, 2.0);
        Self { pitch_gravity, rhythm_gravity, gamaka_depth, morph_speed }
    }
}
```

The texture ↔ music continuum:

| Emotion State | Gravity | Sound | Visual |
|---------------|---------|-------|--------|
| Calm, positive | high | Locked raga, clean tala | Sharp blob edges, cool colors |
| Slightly aroused | medium | Subtle gamaka, slight swing | Softening edges, warming |
| High arousal | low | Pitch drifts, rhythm dissolves | Soft glowing blobs, hot palette |
| Panic | zero | Free spectral drone — pure texture | Diffuse glow, white-hot |

### 9.2 Refactor organism_renderer.rs → blob_renderer.rs

The L-shaped organisms from 0.5 become round blob nodes:

```rust
// Old: OrganismGpuData (48 bytes)
// New: BlobGpuData
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlobGpuData {
    pub pos: [f32; 2],           // center position
    pub radius: f32,             // base radius (from EWMA activity)
    pub edge_softness: f32,      // from gravity state
    pub thermal_temp: f32,       // from emotion arousal
    pub hue_shift: f32,          // from emotion valence / raga hue
    pub pulse_phase: f32,        // from tala beat phase
    pub pulse_amplitude: f32,    // from arousal
    pub glyph_start: u32,        // MSDF text overlay (retained)
    pub glyph_count: u32,        // MSDF text overlay (retained)
    pub _pad: [f32; 2],
}
```

### 9.3 Extend Uniforms for audio-driven fields

```rust
pub struct Uniforms {
    // existing:
    pub viewport: [f32; 2],
    pub time: f32,
    pub blob_count: f32,  // was organism_count
    pub dpr: f32,
    // new audio-driven fields:
    pub beat_phase: f32,       // 0.0–1.0 within current beat
    pub gravity_strength: f32, // overall pitch gravity
    pub arousal: f32,          // drives thermal temperature
    pub valence: f32,          // drives color hue shift
}
```

### 9.4 Modify organism.wgsl → blob.wgsl

Replace L-shaped SDF with circle SDF + smin merging:

**Circle SDF** (replaces sdRoundedBox4):
```wgsl
fn sdCircle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}
```

**Smooth minimum** for blob merging (consult smoothman):
```wgsl
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * 0.25 * k;
}
```

**Beat pulse**: blob scale oscillates with beat_phase
```wgsl
let pulse = 1.0 + sin(uniforms.beat_phase * 6.283) * 0.02 * blob.pulse_amplitude;
// Apply pulse to radius before SDF evaluation
```

**Gravity → edge sharpness**: low gravity = softer SDF edges
```wgsl
let edge_softness = mix(4.0, 0.5, blob.edge_softness);
// Use in smoothstep threshold for SDF boundary
```

**Arousal → glow intensity**: high arousal = brighter glow halo
```wgsl
let glow = exp(-max(field, 0.0) * 0.03) * (0.1 + uniforms.arousal * 0.3);
```

**Thermal palette** (from emotive color system plan):
```wgsl
fn thermal_palette(t: f32) -> vec3<f32> {
    // 8-stop: black → indigo → blue → cyan → green → yellow → orange → white
    let colors = array<vec3<f32>, 8>(
        vec3(0.0, 0.0, 0.0),       // 0.0 black
        vec3(0.18, 0.0, 0.35),     // ~0.14 indigo
        vec3(0.0, 0.0, 0.8),       // ~0.28 blue
        vec3(0.0, 0.7, 0.9),       // ~0.42 cyan
        vec3(0.1, 0.8, 0.2),       // ~0.57 green
        vec3(1.0, 0.95, 0.2),      // ~0.71 yellow
        vec3(1.0, 0.5, 0.0),       // ~0.85 orange
        vec3(1.0, 1.0, 1.0),       // 1.0 white
    );
    let idx = t * 7.0;
    let i = u32(floor(idx));
    let f = fract(idx);
    return mix(colors[i], colors[min(i + 1u, 7u)], f);
}
```

**Valence → color temperature**: negative valence shifts cool, positive warm
```wgsl
let temp_bias = uniforms.valence * 0.15;
```

### 9.5 Consult smoothman

At this point, consult the smoothman agent for:
- Verifying the SDF edge softness parameter doesn't create artifacts
- Ensuring the beat pulse modulation on SDF dimensions is smooth
- Reviewing the thermal/glow additions to the existing shader
- smin k-factor tuning for blob merging (module connections)

### 9.6 Create `src/modules/tool_glyph_module.rs`

```rust
pub struct ToolGlyphModule {
    schema: ModuleSchema,
    glyph_buffer: Vec<char>,
    pattern_source: Vec<f32>,
}
```

**Schema**:
- Inputs:
  - `pattern` (Pattern, Block) — signal values to visualize
  - `text` (Text, Event) — text to display
- Outputs:
  - `glyphs` (Pattern, Block) — MSDF glyph indices for renderer

Reuses the existing font_atlas + MSDF system. Instead of static text
labels, generates dynamic glyph sequences driven by incoming signals.
The "tool glyphs actually being used by the various scripts to show
patterning or data diagrams."

### 9.7 Create `src/modules/data_diagram_module.rs`

```rust
pub struct DataDiagramModule {
    schema: ModuleSchema,
    history: VecDeque<f32>,
    history_capacity: usize,
}
```

**Schema**:
- Inputs:
  - `value` (Float, Block) — signal to plot
- Outputs: (renders directly in egui via ui())

**Custom UI panel**:
- Sparkline graph of signal history
- Min/max/current value display
- Auto-scaling Y axis

### 9.8 Create `src/modules/ascii_texture_module.rs`

```rust
pub struct AsciiTextureModule {
    schema: ModuleSchema,
    char_grid: Vec<Vec<char>>,
    grid_width: usize,
    grid_height: usize,
}
```

**Schema**:
- Inputs:
  - `frame` (FrameRef, Block) — from CameraModule or VideoFileModule
  - `pattern` (Pattern, Block) — alternative: map pattern values
- Outputs:
  - `ascii_texture` (Pattern, Block) — character density grid for renderer

Maps a FrameRef or Pattern to a grid of ASCII characters by
luminance/value. Characters ordered by visual density:
` .:-=+*#%@`. Renders the grid as MSDF text at blob positions.

### 9.9 Create `src/modules/isf_visual_module.rs`

```rust
pub struct IsfVisualModule {
    schema: ModuleSchema,  // auto-generated from ISF header
    isf_shader: IsfShader,
    param_values: HashMap<String, f32>,
    output_texture: Option<wgpu::Texture>,
}
```

**Schema**: Auto-generated from ISF shader header. Each ISF input
parameter becomes a typed affinity port.

**The ISF-as-module pattern**:
1. Load ISF shader file from `assets/shaders/`
2. Parse JSON header → extract input parameters
3. Generate ModuleSchema with matching ports
4. Each tick: collect input signals, update uniform values
5. Render shader to output texture
6. Emit FrameRef of rendered result

Drop a new ISF shader file → the system auto-generates a Module with
matching ports → affinity edges form to compatible signal sources → the
blob renders the shader output. Every visual module is a live-patchable
shader unit.

### 9.10 Feed gravity state into shader each frame

In `app.rs` update loop:
1. Compute `GravityState::from_emotion(...)` (or from manual sliders)
2. Read `TalaGrid.phase` for beat_phase
3. Build BlobGpuData for each registered module
4. Pack into extended Uniforms
5. Pass to blob_renderer via existing paint callback

## Files Created

```
src/tuning/gravity_control.rs      — GravityState, from_emotion mapping
src/modules/tool_glyph_module.rs   — ToolGlyphModule (Module impl)
src/modules/data_diagram_module.rs — DataDiagramModule (Module impl)
src/modules/ascii_texture_module.rs — AsciiTextureModule (Module impl)
src/modules/isf_visual_module.rs   — IsfVisualModule (Module impl)
```

## Files Modified

```
src/tuning/mod.rs                  — pub mod gravity_control;
src/renderer/organism_renderer.rs  — refactored to blob_renderer.rs:
                                     BlobGpuData, extended Uniforms
organism.wgsl                      — sdCircle + smin, thermal palette,
                                     beat pulse, edge softness, glow
src/modules/mod.rs                 — add visual module mods
src/app.rs                         — gravity state → uniforms pipeline,
                                     build BlobGpuData per module
```

## Verification

1. Blob edges soften when gravity drops (audible: pitch drifts free)
2. Blob edges sharpen when gravity rises (audible: pitch locks to scale)
3. Blobs pulse with the tala beat (visible rhythmic breathing)
4. High arousal → blobs glow brighter, warmer colors (thermal palette)
5. Low valence → blobs shift toward cooler tones
6. RMS from audio analysis causes subtle blob intensity changes
7. No visual artifacts from the shader modifications
8. Performance: still 60fps with the additional uniforms
9. Tool glyphs: connect audio_analysis → see dynamic text on blobs
10. Data diagram: connect value → see sparkline in egui inspector
11. ASCII texture: connect camera → see ASCII video overlay on blob
12. ISF shader: load a test ISF → params auto-wire to nearby signals
13. smin merging: modules with strong affinity edges visually merge

## The Moment

This is where the project becomes audiovisual. Before S09, audio and
visual systems are separate. After S09, moving a gravity slider
simultaneously changes the pitch quantization you hear AND the visual
sharpness you see. Camera motion drives arousal which drives both
pitch drift and blob glow. The system becomes synesthetic.
