# HUD Architect Brief — LCARS Glass Workbench HUD

This brief defines the v1 architecture for the clean LCARS-block, colored-glass HUD driving Solido Tri-D's procedural/shader workflow.

## 0) Product intent

Solido Tri-D is a Godot 4 "graphics construction apparatus" focused on GLSL-like shader development, experimental control, and extensible architecture.

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
- Avoid "Microsoft 8": no uniform tile grids, no identical radii everywhere, no flat-only state changes.

### Resolution (3240x2160 native, Surface Book 2)
- Primary design space: 3240x2160 (3:2 aspect, 260 DPI).
- Must expand/reflow cleanly for ultrawide and 4K with no distortion and no black bars (use Stretch Aspect `expand`).
- All sizes are tokenized, expressed in base unit `u` (u = 24 px @ 3240x2160), and scaled via one UI scale factor.

### Touch-first (10-point multitouch + pen)
- Primary hardware: Surface Book 2 (260 DPI, 10-point capacitive multitouch + active pen with pressure + hover).
- Touch and mouse/pen are co-equal input paths. Never assume hover is available.
- All interactive elements must meet touch target minimums (>=3u / 72px / ~7mm). Preferred: 4u+ (96px / ~9.4mm).
- Adjacent target gap: >=1u (24px / ~2.3mm) to prevent mis-taps.

---

## 2) Typography system (Okuda + Rajdhani + Oxanium)

Fonts live at `/fonts/` in the repo root.

### Font roles
- **Okuda**: display only — rails, section headers, big mode labels, short identifiers.
- **Rajdhani**: UI text — labels, buttons, section titles, tooltips.
- **Oxanium**: numeric/technical readouts — values, units, telemetry, timecodes.

### Style rules
- Prefer uppercase for LCARS labels (or a consistent transform step).
- Slight tracking increase for display labels; keep numeric readouts tight.
- Align values in columns; align units to a stable baseline.

### Type scale (3240x2160 native)
- Rail header: 56–72 px (~2.5–3u)
- Section header: 36–44 px (~1.5–1.75u)
- Label: 28–32 px (~1.15–1.3u)
- Value: 32–40 px (~1.3–1.7u)
- Micro telemetry: 24–28 px (~1u)

---

## 3) Layout schema (modules + primitives)

### Canonical workbench layout
- **Left**: Stack (layers/nodes/history) + thick navigation rail.
- **Center**: World preview viewport (dominant).
- **Right**: Inspector (parameter editing) with collapsible sections and consistent parameter rows.
- **Bottom**: Status strip (mode, selection, perf/telemetry).

Ultrawide behavior (Aspect `expand`): center stays dominant, side modules widen or gain secondary columns (mini-scopes, histogram, node graph). If too narrow, collapse modules into tabbed rails.

### Primitive library (only these are "hard" components)
1. **LcarsRail** — segmented rail (H/V), solid, role-colored.
2. **LcarsEndcap** — half-pill / stepped termination.
3. **LcarsElbow** — 90deg orthogonal join (structural anchor).
4. **GlassPane** — translucent panel surface (blur + tint + grain + bevel via CanvasItem shader + hint_screen_texture).
5. **Chip** — small rounded block for modes/tags/blend types.
6. **Bracket** — thin schematic bracket/tick group for framing.
7. **SplineConnector** — Curve2D-based arc/spline routing element.

Everything else is composition.

### Connector logic (arcs/splines)
Model each module as a rectangle with "ports":
- Ports: N/E/S/W (and optional corner ports NE/NW/SE/SW).
- Each port has: `role_color`, `thickness`, `priority`, `style` (hard elbow vs arc), `dock_offset`, `capacity`.

Routing rules:
- Prefer arcs between related modules (Stack -> Inspector, Inspector -> Viewport overlays).
- Use rails/endcaps as "bus lines," then arcs as "branch lines."
- Connectors have hierarchy: primary (thick, bright), secondary (thin, dim), tertiary (schematic).

See `CONNECTORS.md` and `ROUTER_PSEUDOCODE.md` for full specification.

---

## 4) Interaction model

### States
- **Hover** (mouse/pen only): rim brightens + bevel highlight increases; connector glow subtly intensifies.
- **Active/pressed**: bevel inverts slightly (inset), tint deepens. This is the *primary* feedback for touch — must be unmistakable without a preceding hover.
- **Focus**: target brackets or corner ticks (schematic), not a generic outline.

### Touch gestures
- **Tap**: select / activate (equivalent to click).
- **Long-press** (>=400ms): context action (right-click equivalent). Visual feedback at ~200ms (rim pulse) so user knows the hold is registering.
- **Drag**: parameter scrubbing on sliders/numeric readouts, panel resize, viewport orbit.
- **Two-finger pinch**: viewport zoom.
- **Two-finger pan**: viewport pan.
- **Swipe on rail edge**: collapse/expand side modules.

### No-hover adaptation
- Hover state reveals (tooltips, connector highlight on approach) must have touch-accessible alternatives: tap-to-inspect, or always-visible micro-labels.
- Connector highlighting on module hover -> on touch, highlight on module *tap* (first tap selects + highlights, second tap activates).
- Bevel/rim feedback on Active state must be strong enough to serve as the primary interaction confirmation.

### Godot touch specifics
- `InputEventScreenTouch` for tap/release, `InputEventScreenDrag` for drag gestures.
- `InputEventMouseButton` with `device == -1` can also carry touch events (Godot's mouse emulation). For gesture detection (pinch, multi-touch), use raw `InputEventScreenTouch` with `index` to track fingers (0-9 for 10-point).
- `ProjectSettings.input_devices/pointing/emulate_mouse_from_touch = true` (default) maps single-touch to mouse — most Control nodes work out of the box.
- Multi-touch gestures need custom `_unhandled_input()` handling on the viewport layer.
- Surface Book 2 pen (stylus) sends `InputEventMouseButton`/`InputEventMouseMotion` with pressure — pen hover IS available (unlike finger touch).

---

## 5) Rendering + shader contract (Godot 4)

### CanvasItem shader baseline
All core surfaces (blocks, panes, connectors, brackets) are rendered using CanvasItem shaders (`shader_type canvas_item`) for consistent 2D/UI rendering.

### GlassPane (world-viewport blur)
Glass panes sample the rendered screen (screen-reading shader via `hint_screen_texture` + `SCREEN_UV`) to blur and tint the world behind the HUD.

Rules:
- Apply blur/tint only to pane backgrounds.
- Text/icons render above glass (no blur).
- Blur: use mipmapped `textureLod()` to read a blurred mip level (LOD > 0).
- Grain: subtle noise (blue-noise or hash) to avoid banding and "flat card" vibes.
- Overlapping glass panes need `BackBufferCopy` between them to control what each pane "sees" and avoid disappearance/feedback artifacts.
- Provide a per-pane fallback preset (tint + grain only) for performance/platform safety.

Uniform contract (v1):
- `u_tint_color`: vec3
- `u_alpha`: float
- `u_blur_lod`: float
- `u_grain`: float
- `u_radius_px`: float
- `u_bevel_px`: float
- `u_rim_px`: float
- `u_state`: float (0 normal, 1 hover, 2 active, 3 disabled)

### Bevel language
Bevel is first-class — what keeps LCARS-glass from turning into generic flat UI:
- Compute an SDF for rounded rect.
- Use SDF distance bands to paint:
  - Inner highlight (top-left bias)
  - Inner shadow (bottom-right bias)
  - Rim line (thin, consistent)
  - Optional "edge tint" (very subtle chroma shift)
- All bevel widths and rim thickness scale in logical pixels so they remain visually stable across resolutions.

### Connector rendering (arcs/splines)
Connectors read as "optical cable / power trace," not neon graffiti:
- Core stroke: semi-opaque, role-tinted.
- Edge: 1 px highlight line (bevel illusion).
- Optional animated scanline: low amplitude, slow speed, only on primary connectors.

Implementation options:
- Phase 1: Line2D from baked points (fast prototyping).
- Phase 3+: Ribbon mesh + shader (core fill + rim highlight + subtle shadow).

### Layer ordering (bottom to top)
1. World viewport (base)
2. GlassPane layer(s) sampling world
3. Solid LCARS blocks/rails (opaque, highest contrast)
4. Schematic layer (thin lines, ticks, brackets)
5. Text & icons (always top, always crisp)

---

## 6) System architecture (how the HUD is built)

### Data -> UI pipeline
- TOML schema describes parameters and UI intent.
- Parser produces a Dictionary representation.
- UI builder instantiates Controls in a predictable hierarchy.
- Preview viewport manager drives live preview.

### Required interfaces
- **UiTokens**: token store (unit sizes, radii, palette roles, type scale).
- **UiThemeBinder**: applies fonts + Theme defaults + shader presets.
- **UiModule** base: stable `module_id`, layout hints, connector ports, reserved zones.
- **ConnectorRouter**: deterministic routing engine.

### Debuggability requirements
Provide a toggle overlay showing:
- Module rects
- Port locations + IDs
- Reserved zones
- Connector paths + lane IDs
- Reroute/constraint decisions

---

## 7) Anti-Metro checklist

Every design must pass:
- [ ] Varied radii by component type (rails vs panes vs chips)
- [ ] Endcap language present (half-pill, stepped, bracket)
- [ ] Micro-structure exists (ticks, separators, ID badges, schematic strokes)
- [ ] Bevel + rim respond to interaction states (not flat color swaps)
- [ ] Asymmetrical composition (not uniform grid of identical rectangles)
- [ ] All interactive elements >=3u touch target (72px / ~7mm at 260 DPI)
- [ ] Adjacent targets have >=1u gap (24px / ~2.3mm)
- [ ] Active/pressed state visually sufficient without hover precursor

---

## 8) Build plan & acceptance

### Build phases

| Phase | Deliverables |
|-------|-------------|
| 0 — Tokens | `hud_tokens.tres` (unit sizes, radii, bevels, role colors), 6 palette presets |
| 1 — Primitives | 7 `.tscn` scenes (GlassPane, LcarsRail, LcarsEndcap, LcarsElbow, Chip, Bracket, SplineConnector) |
| 2 — Demo HUD | Workbench scene: Stack + Inspector + Status + 3-5 connectors + 2 overlapping glass panes |
| 3 — Responsiveness | Multi-resolution testing and regression |

### Immediate v1 tasks
1. Define UiTokens and publish v1 token values.
2. Implement the 7 primitives as reusable scenes with stable API/uniform contract.
3. Assemble the Workbench demo scene using only primitives.
4. Integrate connector ports/reserved zones and make routing deterministic.
5. Add debug overlay + layout test scenes.

### Acceptance tests

**Visual:**
- LCARS reads immediately (rails/endcaps/elbows; strong role colors; short labels).
- No tile-OS feel (bevel response visible; connector logic adds structure; controlled asymmetry).

**Readability:**
- Glass auto-adjusts (slightly) to keep labels/values legible over bright/chaotic viewport content.

**Multi-resolution:**
Test: 3240x2160 (native 3:2), 1920x1080 (external 16:9), 2560x1440, 3440x1440 (ultrawide), 3840x2160 (4K 16:9), 1280x720 (small window).
Pass:
- Type scale remains consistent.
- Rim/bevel widths feel constant.
- Connector routing stable and respects reserved zones.
- Reflow avoids overlap and clipping.
- All touch targets remain >=3u physical.

**Performance:**
- Blur is bounded (LOD-based) and can be disabled per pane.

---

## Appendix — Reference targets (design intent, not copying)
- Clean LCARS block zoning + rails/endcaps/elbows, translated into a modern compositing HUD.
- Glass translucency and bevel behavior tuned to feel "instrument panel," not "mobile card."
- Schematic overlays used as framing and emphasis, never as primary information density.
