# SOLIDO — Clean LCARS Glass HUD (Design Brief)

A modular, resolution-flexible, LCARS-inspired sci‑fi HUD for *graphics play/modification*: layering, combination, compositing, parameter editing, and preview, with colored glass translucency, smooth bevels, and connector logic that feels “instrumental” (not Metro/Win8).

---

## 1) Goals & constraints

### Core goals
- **Clean LCARS blocks** as the macro-language: big segmented color rails, rounded endcaps, dense information zoning, short labels, and confident negative space.
- “Colored glass” micro-language: translucent panes over the world viewport (target ~33% opacity baseline), with gentle blur + grain so you read “material,” not “flat card.”
- Connector logic is a first-class system: arcs/splines that visually route between modules, with rules that create satisfying joins and hierarchy.
- Modular from day one: every screen is assembled from reusable primitives + tokens + shader presets, not bespoke textures.

### Hard constraints
- No distortion and no black bars: use a fixed design resolution (3240×2160, 3:2 aspect) matching the primary display (Surface Book 2, 260 DPI). Stretch Aspect `expand` — external monitors and ultrawide gain space rather than letterbox. [web:76]
- UI must remain readable on any viewport content: glass is *tunable* per pane and can auto-thicken (opacity/blur) when contrast drops.
- Avoid "Microsoft 8" cues: do not rely on uniform flat rectangles, uniform spacing, or identical radii everywhere; emphasize bevels, segmented rails, endcaps, and asymmetrical compositions.
- **Touch-first interaction**: primary hardware is Surface Book 2 (3240×2160, 260 DPI, 10-point multitouch + pen). All interactive elements must meet touch target minimums (≥3u / 72px / ~7mm). Touch and mouse/pen are co-equal input paths — never assume hover is available.

---

## 2) Visual system (tokens)

### Layout tokens (base @ 3240×2160, 260 DPI)
Define all sizes as multiples of a unit `u` (recommend `u = 24 px` at native res):
- `gap_1 = 1u`, `gap_2 = 2u`, `gap_3 = 3u`
- Rail thicknesses: `rail_thick = 3u`, `rail_thin = 2u`
- Corner radii: `r_sm = 2u`, `r_md = 3u`, `r_lg = 4u`
- Bevel width: `bev_sm = 0.5u`, `bev_md = 1u`
- Rim/outline: `rim = 2–4 px` (in *logical pixels*, scaled consistently across resolutions; thicker than 1080p to stay crisp at 260 DPI)
- Touch target minimum: `3u` (72px / ~7mm physical). Preferred: `4u` (96px / ~9.4mm).

### Color system (broad palette, strict semantics)
Use a wide palette, but assign each hue family a role. Suggested role families:
- Navigation rails: warm amber/orange family
- Edit/transform: cyan/teal family
- Layer/blend/mask: purple/indigo family
- Diagnostics/telemetry: green family
- Warning/destructive: magenta/red family
- Neutral glass: smoke/graphite with subtle tint

Rules:
- Bright colors belong on *solid LCARS blocks* (rails, endcaps, chips).
- Glass panes are darker and more translucent; they borrow hue via tint, not full saturation.
- Text is mostly near-black on bright blocks and off-white on dark glass; never bright-on-bright.

### Typography & labeling
- Labels: short, uppercase, high tracking (LCARS vibe).
- Values: monospaced or mono-like for numeric stability (sliders, readouts, units).
- Always align numbers in columns; align units to a consistent baseline.

### Anti-Metro heuristics
To keep it from reading like Win8 tiles:
- Vary radii by component type (rails vs panes vs chips).
- Always add an “endcap language” (half-pill ends, stepped ends, bracket ends).
- Introduce micro-structure: ticks, separators, little ID badges, and small schematic strokes.
- Use bevel + rim response to interaction states (hover/active) instead of flat color swaps.

---

## 3) Layout schema & modules

### Macro layout (default “workbench”)
- Center: World viewport (primary visual).
- Left: “Stack” module (layers/graph/lists) with thick navigation rail.
- Right: “Inspector” module (properties) with collapsible sections and numeric readouts.
- Bottom: Status + mode strip (tool state, selection, perf, render mode).

Ultrawide behavior (Aspect `expand`):
- Keep center viewport dominant.
- Let side modules widen or add secondary columns (e.g., mini-scopes, histogram, node graph).
- Preserve minimum readable widths; if space is too small, collapse modules into tabbed rails.

### Primitive library (the only things you hand-author)
1. `LcarsRail` — horizontal/vertical segmented rail (solid).
2. `LcarsEndcap` — half-pill/stepped termination pieces.
3. `LcarsElbow` — classic 90° join (still needed even with arcs, for grounded structure).
4. `GlassPane` — translucent panel surface (blur/tint/grain/bevel).
5. `Chip` — small rounded block for modes/tags/blend types.
6. `Bracket` — thin schematic bracket/tick group for framing.
7. `SplineConnector` — arc/spline routing element (the “creative logic” star).

Everything else is composition.

### Connector logic (arcs/splines)
Model each module as a rectangle with “ports”:
- Ports: N/E/S/W (and optional corner ports NE/NW/SE/SW)
- Each port has: `role_color`, `thickness`, `priority`, `style` (hard elbow vs arc), `dock_offset`, `capacity`

Routing rules:
- Prefer arcs between related modules (Stack → Inspector, Inspector → Viewport overlays).
- Use rails/endcaps as “bus lines,” then arcs as “branch lines.”
- Connectors have hierarchy: primary (thick, bright), secondary (thin, dim), tertiary (schematic).

Spline style spec:
- Default connector = cubic Bézier / Curve2D with two control points.
- Clamp curvature: never exceed a max bend radius that would look like a noodle; keep it “engineered.”
- Add a connector “cap” at endpoints: small endcap, dot, or bracket depending on role.

### Interaction model
- Hover (mouse/pen only): rim brightens + bevel highlight increases; connector glow subtly intensifies.
- Active/pressed: bevel inverts slightly (inset), tint deepens. This is the *primary* feedback for touch — must be unmistakable without a preceding hover.
- Focus: add target brackets or corner ticks (schematic), not a generic outline.

### Touch interaction (Surface Book 2 touchscreen)
Touch is a first-class input path alongside mouse and pen. Godot unifies these via `InputEventScreenTouch`, `InputEventScreenDrag`, and `gui_input` propagation.

**Target sizing:**
- Minimum interactive target: 3u × 3u (72×72 px / ~7×7mm at 260 DPI). This applies to chips, rail segments, collapse toggles, and connector endpoints.
- Preferred target for frequent actions: 4u+ (96px+ / ~9.4mm+).
- Labels/readouts that aren't interactive don't need touch sizing.
- Padding between adjacent targets: ≥1u (24px / ~2.3mm) to prevent mis-taps.
- 10-point multitouch: enables simultaneous two-finger gestures (pinch, pan) plus independent finger interactions elsewhere on screen.

**Gestures:**
- Tap: select / activate (equivalent to click).
- Long-press (≥400ms): context action (equivalent to right-click). Provide visual feedback at ~200ms (rim pulse) so user knows the hold is registering.
- Drag: parameter scrubbing on sliders/numeric readouts, panel resize, viewport orbit.
- Two-finger pinch: viewport zoom.
- Two-finger pan: viewport pan.
- Swipe on rail edge: collapse/expand side modules.

**No-hover adaptation:**
- Hover state reveals (tooltips, connector highlight on approach) must have touch-accessible alternatives: tap-to-inspect, or always-visible micro-labels.
- Connector highlighting on module hover → on touch, highlight on module *tap* (first tap selects + highlights, second tap activates).
- Bevel/rim feedback on Active state must be strong enough to serve as the primary interaction confirmation — touch users never see the Hover ramp-up.

**Godot touch specifics:**
- `InputEventScreenTouch` for tap/release, `InputEventScreenDrag` for drag gestures.
- `InputEventMouseButton` with `device == -1` can also carry touch events (Godot's mouse emulation). For gesture detection (pinch, multi-touch), use raw `InputEventScreenTouch` with `index` to track fingers.
- `ProjectSettings.input_devices/pointing/emulate_mouse_from_touch = true` (default) maps single-touch to mouse — most Control nodes work out of the box.
- Multi-touch gestures need custom `_unhandled_input()` handling on the viewport layer.
- Surface Book 2 pen (stylus) sends `InputEventMouseButton`/`InputEventMouseMotion` with pressure — pen hover IS available (unlike finger touch).

---

## 4) Rendering & shader spec

### Why CanvasItem shaders
All GUI elements and 2D nodes in Godot are rendered as CanvasItems, and CanvasItem shaders are the intended shader type for 2D and GUI rendering. [page:1]

### GlassPane shader (world-blur + tint)
Use screen-reading so panes can sample the world viewport behind them using `hint_screen_texture` and `SCREEN_UV`. [page:2]  
- Base transparency target: `alpha_base ≈ 0.33` (pane-dependent; auto-adjust for readability).
- Blur: use mipmapped `textureLod()` to read a blurred mip level (LOD > 0), with a mipmap-enabled filter mode. [page:2]
- Grain: subtle noise (blue-noise or hash) to avoid banding and “flat card” vibes.

Important: screen-reading has ordering/overlap caveats; if multiple overlapping glass panes sample the screen, you may need `BackBufferCopy` to control what each pane “sees” and avoid unexpected disappearance/feedback artifacts. [page:2]

Uniform contract (example):
- `u_tint_color : vec3`
- `u_alpha : float`
- `u_blur_lod : float`
- `u_grain : float`
- `u_bevel_px : float`
- `u_radius_px : float`
- `u_rim_px : float`
- `u_state : float` (0 normal, 1 hover, 2 active, 3 disabled)

Platform note:
- Keep a “no-blur” fallback preset (tint + grain only) for cases where blur/screen effects are unreliable or too expensive (e.g., WebGL quirks). [web:36]

### Bevel + rim (avoid Metro)
Bevel is what keeps LCARS-glass from turning into generic flat UI:
- Compute an SDF for rounded rect.
- Use SDF distance bands to paint:
  - Inner highlight (top-left bias)
  - Inner shadow (bottom-right bias)
  - Rim line (thin, consistent)
  - Optional “edge tint” (very subtle chroma shift)

Make bevel widths and rim thickness scale in logical pixels so they remain visually stable across resolutions.

### Connector shader/style
Connectors should read as “optical cable / power trace,” not neon graffiti:
- Core stroke: semi-opaque, role-tinted
- Edge: 1 px highlight line (gives bevel illusion)
- Optional animated scanline: low amplitude, slow speed, only on primary connectors

Implementation options:
- Geometry-driven: render spline as a polyline mesh (thick line) and shade in a CanvasItem shader.
- Procedural: draw in a dedicated CanvasItem node that generates UV along the spline and uses shader for bevel/highlight.

### Layer ordering
- World viewport (base)
- GlassPane layer(s) sampling world
- Solid LCARS blocks/rails (opaque, highest contrast)
- Schematic layer (thin lines, ticks, brackets)
- Text & icons (always top, always crisp)

---

## 5) Build plan & tests

### Phase 0 — Tokens + presets
Deliver:
- `tokens/ui_tokens.tres` (or JSON) with unit sizes, radii, bevel widths, and role colors.
- 6 palette presets (Ops Amber, Cyan Tools, Violet Blend, Green Telemetry, Magenta Alert, Neutral Smoke).

### Phase 1 — Primitive scenes
Create and freeze:
- `GlassPane.tscn` (with blur + fallback)
- `LcarsRail.tscn`, `LcarsEndcap.tscn`, `LcarsElbow.tscn`
- `Chip.tscn`, `Bracket.tscn`
- `SplineConnector.tscn` (Curve2D-based routing + rendering)

### Phase 2 — Demo HUD (the acceptance target)
A single “Workbench” scene:
- Left Stack (list + chips)
- Right Inspector (parameter rows + collapses)
- Bottom Status strip
- 3–5 connectors (primary + secondary)
- 2 overlapping glass panes (to validate BackBufferCopy strategy when needed). [page:2]

### Phase 3 — Responsiveness & regression tests
Test sizes:
- 3240×2160 (native, 3:2), 1920×1080 (external 16:9), 2560×1440, 3440×1440 (ultrawide), 3840×2160 (4K 16:9), plus a "small window" case (e.g., 1280×720).
Pass criteria:
- No black bars and no distortion with Aspect `expand`. [web:76]
- Rim thickness and bevel width feel constant (not “fatter” at 4K).
- Text remains crisp and readable over bright/chaotic backgrounds.
- Connector routing remains stable (no overlaps through important content; reroutes around reserved zones).

---

## Appendix — Reference targets (design intent, not copying)
- Clean LCARS block zoning + rails/endcaps, translated into a modern compositing HUD.
- Glass translucency and bevel behavior tuned to feel “instrument panel,” not “mobile card.”
- Schematic overlays used as framing and emphasis, never as primary information density.
