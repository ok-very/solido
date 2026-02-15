# WIDGETS_DIAL_AND_RAIL.md — LCARS Dial Knob + Rail Slider (v1)

This spec defines the first two “hero” widgets for the LCARS-glass HUD: **dial knobs** and **rail sliders**.

They must:
- Look like engineered LCARS instrumentation (segmented pills, endcaps, bevel/rim).
- Stay resolution-flexible (derive geometry from `size` + token unit `u`).
- Share a single shader/uniform contract so presets/theme tokens can drive everything.

---

## 0) Dependencies / assumptions

- Fonts available under `/fonts/` at repo root:
  - Okuda (display).
  - Rajdhani (UI labels).
  - Oxanium (values/telemetry).

- UI tokens exist (or will exist) as a central resource `UiTokens`:
  - `unit_u_px` (default 12 at 1080p).
  - radii/bevel/rim defaults.
  - role palette.

- Rendering uses `shader_type canvas_item` materials on UI nodes for the surface look.

---

## 1) Shared visual grammar

### 1.1 Segmentation cadence
- Default segmentation: `major_every = 4`.
- Major segments are either:
  - slightly brighter rim, or
  - slightly larger pill height/width (<= +12%),
  - never both at once (avoid “toy” look).

### 1.2 Bevel/rim recipe
All pill surfaces share these layers:
1. Core fill (role tinted).
2. Rim line (1–3 px logical).
3. Inner highlight band (top-left bias).
4. Inner shadow band (bottom-right bias).

The bevel effect must be visible but subtle—no glossy gradients.

### 1.3 State styling
- Normal: baseline alpha + rim.
- Hover: rim +10–20%, slight highlight increase.
- Active/dragging: bevel “inset” (swap highlight/shadow emphasis), alpha +5–10%.
- Disabled: desaturate + reduce contrast, keep silhouette.

### 1.4 Typography
- Labels: Rajdhani, 14–16 px @ 1080p, uppercase preferred.
- Values: Oxanium, 16–20 px @ 1080p, right-aligned.
- Mode tags: Okuda, 18–22 px @ 1080p, short identifiers only.

---

## 2) Rail Slider — `LcarsRailSlider`

### 2.1 Purpose
A linear slider that reads like an LCARS rail: stacked pill segments with a filling pill train and a capsule thumb.

### 2.2 Node structure (recommended)
`LcarsRailSlider` (Control)
- `Track` (ColorRect) — ShaderMaterial draws unfilled segments.
- `Fill` (ColorRect) — ShaderMaterial draws filled segments (can be merged into Track shader if preferred).
- `Ticks` (Control) — optional `_draw()` tick overlay.
- `Label` (Label) — Rajdhani.
- `Value` (Label) — Oxanium.

Layout:
- Track fills available width.
- Label/value sit in a consistent row above or below (configurable).

### 2.3 Geometry rules
- Orientation: horizontal v1 (vertical later).
- Segment count:
  - `segments = floor((track_length_px - endcap_guard_px) / segment_pitch_px)`
  - `segment_pitch_px = clamp(2u, 5u)` depending on available width.
- Segment (pill) size:
  - pill height = `rail_thickness_px = 3u` (default).
  - pill radius = `min(rail_thickness_px/2, r_md)`.

### 2.4 Behavior
- Value range: `min..max`, normalized `v = (value-min)/(max-min)`.
- Fill modes:
  - `STEP_FILL`: lights whole segments up to value.
  - `SOFT_FILL`: partially fills the current segment to represent continuous value.
- Snapping:
  - `snap = true` by default for “LCARS stepped” feel.
  - `snap_step = 1 segment` (or schema-provided step).
- Fine adjust:
  - Holding Shift reduces sensitivity (0.1×).

### 2.5 Interaction
- Click on rail: jump thumb to clicked position.
- Drag thumb: continuous update.
- Mouse wheel: +/- one step (if snapping) or +/- small delta.

### 2.6 Shader uniforms (contract)
These uniforms must be identical across Track/Fill materials.

Geometry:
- `u_size_px: vec2` (Control size)
- `u_radius_px: float`
- `u_bevel_px: float`
- `u_rim_px: float`

Segmentation:
- `u_segments: int`
- `u_major_every: int`
- `u_gap_px: float` (gap between pills)

Value:
- `u_value_norm: float` (0..1)
- `u_soft_fill: float` (0/1)
- `u_snap: float` (0/1)

Color:
- `u_track_color: vec4`
- `u_fill_color: vec4`
- `u_rim_color: vec4`
- `u_alpha: float`

State:
- `u_state: float` (0 normal, 1 hover, 2 active, 3 disabled)

### 2.7 Acceptance checks
- At 1080p and 4K, rim and bevel widths feel constant.
- Segments remain visually discrete (no merging) down to min width.
- Value label alignment remains stable during drag.

---

## 3) Dial Knob — `LcarsDialKnob`

### 3.1 Purpose
A circular dial that reads like a wrapped rail: arc segments (pills) around a glass face, with a bracket indicator and optional “stacked pill” companion meter.

### 3.2 Node structure (recommended)
`LcarsDialKnob` (Control)
- `Ring` (ColorRect) — ShaderMaterial draws segmented arc + fill.
- `Face` (ColorRect) — Glass/tint disk (subtle).
- `Indicator` (Control) — `_draw()` bracket/needle/ticks.
- `Label` (Label) — Rajdhani.
- `Value` (Label) — Oxanium.
- Optional: `PillStack` (Control/ColorRect) — vertical stacked pills that mirror value.

### 3.3 Dial sweep
- Default sweep: 270° (starts at ~225°, ends at ~-45°).
- Dead zone: leave a small gap for LCARS readability and to avoid “perfect gauge” look.
- `angle_min`, `angle_max` configurable.

### 3.4 Segmentation
- `segments = 24` default (adjust by radius and u).
- Major every 4.
- Segment thickness = `2u` to `3u` depending on dial size.

### 3.5 Value mapping
- Normalized value `v` maps to angle:
  - `angle = lerp(angle_min, angle_max, v)`
- Snapping optional:
  - default off for knobs (more “analog”), but enable if schema step is discrete.

### 3.6 Interaction model
- Primary: vertical drag adjusts value (coarse).
- Secondary: radial drag (angle) optional toggle.
- Fine adjust: Shift = 0.1×.
- Double-click: reset to default.

### 3.7 Shader uniforms (contract)
Ring shader:
- Same shared uniforms as rail slider (size, radius, bevel, rim, segments, major, value_norm, colors, state).
- Additional:
  - `u_angle_min: float` (radians)
  - `u_angle_max: float` (radians)
  - `u_inner_radius_px: float`
  - `u_outer_radius_px: float`

Face shader (glass-lite):
- `u_tint_color`, `u_alpha`, `u_grain`, `u_bevel_px`, `u_rim_px`.
- No heavy blur by default (keep knob crisp); blur can be a preset.

### 3.8 Indicator rules
- Indicator is always high-contrast.
- Use bracket tips or a small capsule pointer (not a photoreal needle).
- Optional major ticks drawn in `_draw()`.

### 3.9 Acceptance checks
- Knob remains readable at small sizes (>= ~7u diameter).
- Filled arc accurately tracks value, no aliasing gaps.
- Indicator never clashes with label/value placement.

---

## 4) Stacked pills (shared pattern)

A “stacked pill meter” is a small vertical or horizontal strip of N pill segments.

Rules:
- Same segmentation cadence as rails.
- Support peak-hold marker (later), but v1 just fill.
- Supports both STEP_FILL and SOFT_FILL.

This component should be reusable for VU meters later.

---

## 5) Implementation notes (v1 shortcuts)

- Start with one shader for both Ring and Rail: treat dial as “segmented rail in polar coords,” slider as Cartesian.
- Keep tick rendering in `_draw()` initially for fast iteration.
- Do not bake assumptions about 1080p; derive everything from `size` and `u`.

---

## 6) v1 deliverables
1. `scenes/ui/widgets/LcarsRailSlider.tscn` + script.
2. `scenes/ui/widgets/LcarsDialKnob.tscn` + script.
3. `scenes/ui/widgets/LcarsPillStack.tscn` (optional but recommended).
4. `shaders/ui/lcars_segmented_rail.gdshader` (cartesian).
5. `shaders/ui/lcars_segmented_ring.gdshader` (polar).
6. Demo scene showing:
   - 1 rail slider (snap on/off)
   - 1 dial (vertical drag)
   - 1 pill stack
   - with labels + Oxanium value readouts

---
