# HUD Architect Brief — LCARS Glass Workbench HUD

This brief defines the v1 architecture for a clean LCARS-block, colored-glass HUD used to drive Solido Tri‑D’s procedural/shader workflow.

## 0) Product intent
Solido Tri‑D is a Godot 4 “graphics construction apparatus” focused on GLSL-like shader development, experimental control, and extensible architecture.

The HUD is not a game HUD; it is a workbench for:
- Parameter editing.
- Layering/stack operations (compositing-adjacent).
- Preview control.
- Diagnostics/telemetry.

The HUD must be modular and schema-driven so new modules can be added by writing schema + generator code, not by rebuilding bespoke UI.

## 1) Design pillars

### Clean LCARS + glass (not Metro)
- Macro language: big segmented blocks, thick rails, endcaps, elbows, confident zoning, strong role color separation.
- Micro language: smooth bevels throughout, subtle rim highlights, and colored glass translucency over the world viewport (baseline ~33% alpha), with gentle blur + grain.
- Avoid “Microsoft 8”: no uniform tile grids, no identical radii everywhere, no flat-only state changes.

### Resolution flexibility (from day one)
- Primary design space: 1920×1080.
- Must expand/reflow cleanly for ultrawide and 4K with no distortion and no black bars (use Stretch Aspect `expand`).
- All sizes are tokenized, expressed in base unit `u` (recommend `u = 12 px` @ 1080p), and scaled via one UI scale factor.

## 2) Typography system (Okuda + Rajdhani + Oxanium)
Fonts are expected under `/fonts/` at repo root.

Use a three-tier strategy:

### Font roles
- Okuda: display only — rails, section headers, big mode labels, short identifiers.
- Rajdhani: UI text — labels, buttons, section titles, tooltips.
- Oxanium: numeric/technical readouts — values, units, telemetry, timecodes.

### Style rules
- Prefer uppercase for LCARS labels (or a consistent transform step).
- Slight tracking increase for display labels; keep numeric readouts tight.
- Align values in columns; align units to a stable baseline.

### Type scale (1080p baseline)
- Rail header: 28–36 px.
- Section header: 18–22 px.
- Label: 14–16 px.
- Value: 16–20 px.
- Micro telemetry: 12–14 px.

## 3) Layout schema (modules + primitives)

### Canonical workbench layout
- Left: Stack (layers/nodes/history) + navigation rail.
- Center: World preview viewport.
- Right: Inspector (parameter editing) with collapsible sections and consistent parameter rows.
- Bottom: Status strip (mode, selection, perf/telemetry).

### Primitive library (only these are “hard” components)
1. GlassPane (tinted, blurred, beveled panel surface).
2. Rail (solid segmented LCARS bar).
3. Endcap (half-pill / stepped termination).
4. Elbow (orthogonal join; structural anchor).
5. Chip (small LCARS tag/button).
6. Bracket/Ticks (schematic framing).
7. SplineConnector (arc/spline routing trace).

Everything else is composition.

## 4) Rendering + shader contract (Godot 4)

### CanvasItem shader baseline
All core surfaces (blocks, panes, connectors, brackets) are rendered using CanvasItem shaders for consistent 2D/UI rendering.

### GlassPane (world-viewport blur)
Glass panes sample the rendered screen (screen-reading shader) to blur and tint the world behind the HUD.

Rules:
- Apply blur/tint only to pane backgrounds.
- Text/icons render above glass (no blur).
- Provide a per-pane fallback preset (tint + grain only) for performance/platform safety.

Uniform contract (v1)
- u_tint_color: Color
- u_alpha: float
- u_blur_lod: float
- u_grain: float
- u_radius_px: float
- u_bevel_px: float
- u_rim_px: float
- u_state: float (0 normal, 1 hover, 2 active, 3 disabled)

### Bevel language
Bevel is first-class:
- Outer rim highlight (thin).
- Inner highlight/shadow band (soft).
- Optional subtle edge chroma (very small).

All bevel widths are expressed in logical pixels (scaled) so 1080p and 4K feel consistent.

### Connector rendering (arcs/splines)
Connectors are routed as engineered splines (bounded curvature, discrete lanes, deterministic tie-breakers) and rendered as ribbons with bevel/rim.

Endpoint caps: dot/endcap/bracket depending on role and priority.

## 5) System architecture (how the HUD is built)

### Data → UI pipeline
- TOML schema describes parameters and UI intent.
- Parser produces a Dictionary representation.
- UI builder instantiates Controls in a predictable hierarchy.
- Preview viewport manager drives live preview.

### Required interfaces (architect defines exact class names)
- UiTokens: token store (unit sizes, radii, palette roles, type scale).
- UiThemeBinder: applies fonts + Theme defaults + shader presets.
- UiModule base: stable module_id, layout hints, connector ports, reserved zones.
- ConnectorRouter: deterministic routing engine.

### Debuggability requirements
Provide a toggle overlay showing:
- module rects
- port locations + IDs
- reserved zones
- connector paths + lane IDs
- reroute/constraint decisions

## 6) Acceptance tests

### Visual
- LCARS reads immediately (rails/endcaps/elbows; strong role colors; short labels).
- No tile-OS feel (bevel response visible; connector logic adds structure; controlled asymmetry).

### Readability
- Glass auto-adjusts (slightly) to keep labels/values legible over bright/chaotic viewport content.

### Multi-resolution
Test: 1920×1080, 2560×1440, 3440×1440, 3840×2160, and a small-window case (e.g., 1280×720).
Pass:
- Type scale remains consistent.
- Rim/bevel widths feel constant.
- Connector routing stable and respects reserved zones.
- Reflow avoids overlap and clipping.

### Performance
- Blur is bounded (LOD-based) and can be disabled per pane.

## 7) Immediate v1 tasks
1. Define UiTokens and publish v1 token values.
2. Implement the 7 primitives as reusable scenes with stable API/uniform contract.
3. Assemble the Workbench demo scene using only primitives.
4. Integrate connector ports/reserved zones and make routing deterministic.
5. Add debug overlay + layout test scenes.
