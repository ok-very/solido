# Phase 1 Architectural Blueprint -- 7 HUD Primitive Scenes

**Date:** 2026-02-14
**Author:** HUD Architect (Opus)
**Status:** Complete -- ready for implementation dispatch

---

## Preamble: Phase 0 Dependencies

Phase 1 builds on Phase 0 deliverables (designed in `.claude/skills/hud-architect.md` but not yet on disk). Every primitive assumes these exist at runtime:

- **HudTokens** resource (`hud/tokens/hud_tokens.tres`) -- unit sizes, radii, bevel widths, interaction modifiers
- **HudPalette** resource (`hud/tokens/palette_*.tres`) -- 6 roles x 5 shades each (solid, glass_tint, text_on_solid, text_on_glass, rim)
- **HudRole** enum (`hud/tokens/hud_role.gd`) -- `{ NAV = 0, EDIT = 1, BLEND = 2, TELEMETRY = 3, ALERT = 4, NEUTRAL = 5 }`
- **HudTheme** autoload singleton (`hud/theme/hud_theme.gd`) -- material registry with `register_material(mat, role)` / `unregister_material(mat)`, emits `palette_changed` signal, push model for uniforms

Phase 1 primitives register their materials with HudTheme and receive uniform pushes on palette change. If Phase 0 is not yet built, primitives can still be instantiated with manually-set uniforms for isolated testing -- material registration is additive, not a hard load-time dependency.

**Unit system reminder:** `u = 24px` at native 3240x2160. All exports suffixed `_u` are in unit multiples. Pixel conversions happen in script: `px = value_u * HudTheme.unit_px`.

---

## Shared Registration Pattern (Not a Base Class)

All 7 primitives follow this pattern independently. No class inheritance -- composition over inheritance per the decision framework.

1. Declare `@export var role: int = HudRole.NEUTRAL`
2. In `_ready()`: if shader-based, create per-instance `ShaderMaterial` (set `resource_local_to_scene = true`), call `HudTheme.register_material(material, role)`. If StyleBoxFlat-based, call `HudTheme.register_stylebox_owner(self, role)` (or equivalent)
3. In `_exit_tree()`: unregister from HudTheme
4. Expose `set_role(new_role: int)` that unregisters old, registers new, triggers visual refresh
5. Connect to `HudTheme.palette_changed` signal for non-shader updates (label colors, StyleBox swaps)

---

---

# SECTION A: Scene Tree Blueprints

---

## A1. LcarsRail

```
LcarsRail (Control)
  |-- SegmentContainer (HBoxContainer or VBoxContainer)
  |     |-- Segment_0 (Panel)          # StyleBoxFlat: bg_color, per-corner radii
  |     |-- Segment_1 (Panel)          # StyleBoxFlat: bg_color, per-corner radii
  |     |-- ... (N segments)
  |-- PortMarkers (Control)            # mouse_filter = IGNORE, zero size
        |-- PortN (Marker2D)           # position-only reference points
        |-- PortS (Marker2D)
        |-- PortE (Marker2D)
        |-- PortW (Marker2D)
```

**Rationale for Panel + StyleBoxFlat:** LcarsRail segments are solid, opaque, role-colored rectangles. StyleBoxFlat provides `bg_color`, per-corner `corner_radius`, per-side `border_width`, and `border_color` -- everything a solid rail segment needs. No custom shader required.

**Segmentation:** The `SegmentContainer` is an `HBoxContainer` (horizontal rail) or `VBoxContainer` (vertical rail), switched by script based on `orientation`. Each segment is a `Panel` child with its own `StyleBoxFlat`. Gaps between segments are achieved via the container's `add_theme_constant_override("separation", gap_px)`. When `segment_count == 1`, only one `Segment_0` Panel exists.

**PortMarkers:** `Marker2D` nodes at computed positions on the Control boundary. They carry no visual -- they exist solely as position references for the composition/routing system. The `PortMarkers` parent has `mouse_filter = MOUSE_FILTER_IGNORE`.

---

## A2. LcarsEndcap

```
LcarsEndcap (Control)
  |-- EndcapBody (ColorRect)           # ShaderMaterial: hud_endcap.gdshader
  |-- PortMarkers (Control)
        |-- PortFlat (Marker2D)        # Attachment edge (joins to rail)
        |-- PortRound (Marker2D)       # Rounded end (optional, for connectors)
```

**Rationale for ColorRect + shader:** Endcaps need two variants -- half-pill (one fully rounded end, one flat) and stepped (a notch/shoulder cutout partway along the shape). StyleBoxFlat can do the half-pill (set two corner radii to height/2, two to 0), but it cannot do the stepped variant at all. Since both variants must come from one primitive, a single SDF shader covers both, branching on a `u_style` uniform.

**ColorRect sizing:** The `ColorRect` fills the `Control` parent via anchor layout (full rect). The shader operates in UV space normalized to the ColorRect's dimensions.

---

## A3. LcarsElbow

```
LcarsElbow (Control)
  |-- ElbowBody (ColorRect)            # ShaderMaterial: hud_elbow.gdshader
  |-- PortMarkers (Control)
        |-- PortH (Marker2D)           # End of horizontal arm
        |-- PortV (Marker2D)           # End of vertical arm
```

**Rationale for ColorRect + shader:** An elbow is an L-shaped filled region with a rounded outer corner and a rounded inner cutout. This is a boolean SDF operation: union of two rectangles (the arms) rounded at the outer corner, with a rounded rectangle subtracted at the inner corner. No combination of StyleBoxFlat panels produces the inner curve -- it requires masking or subtraction, which only a shader SDF can express cleanly.

**Rotation:** The `u_rotation` uniform (0=TL, 1=TR, 2=BR, 3=BL) tells the shader which corner the elbow occupies. The shader flips UV coordinates accordingly rather than relying on Godot node rotation, which would complicate port position math.

---

## A4. GlassPane

```
GlassPane (PanelContainer)
  |-- BackBufferCopy                   # copy_mode = COPY_MODE_RECT
  |-- GlassBody (ColorRect)            # ShaderMaterial: hud_glass.gdshader
  |-- ContentMargin (MarginContainer)  # Content children go here
```

**Critical node ordering:** Godot processes CanvasItem children top-to-bottom in tree order. `BackBufferCopy` must precede `GlassBody` so it captures the screen buffer *before* the shader reads it via `hint_screen_texture`. `ContentMargin` comes after `GlassBody` so content (text, chips, controls) renders *on top of* the glass.

**Rationale for PanelContainer root:** GlassPane exists to contain content. `PanelContainer` provides container layout (children fill with configurable theme margins). Its own visual StyleBox is set to empty/transparent -- all rendering is done by the GlassBody shader. This gives us layout-system integration (anchors, size flags, minimum sizes) plus proper content management.

**BackBufferCopy rect sync:** The script keeps `BackBufferCopy.rect` synchronized with the Control's global rect via `_notification(NOTIFICATION_RESIZED)` and `_notification(NOTIFICATION_TRANSFORM_CHANGED)`. `copy_mode = BackBufferCopy.COPY_MODE_RECT` captures only the pane's footprint, not the entire screen.

**Overlap handling:** When two GlassPanes overlap, an *additional* `BackBufferCopy` must be inserted *between* them in the parent scene's tree order. This is a composition concern (Phase 2), not handled by the primitive itself. The primitive's internal `BackBufferCopy` handles self-contained blur of the world behind it.

---

## A5. Chip

```
Chip (Control)
  |-- ChipBody (Panel)                 # StyleBoxFlat: bg_color, corner_radius_all
  |-- ChipLabel (Label)                # Short uppercase text
  |-- LongPressTimer (Timer)           # wait_time = 0.4, one_shot = true
  |-- RimPulseTimer (Timer)            # wait_time = 0.2, one_shot = true
```

**Rationale for Panel + StyleBoxFlat:** Chips are small solid rounded blocks. StyleBoxFlat with uniform `corner_radius_all` handles this with zero shader overhead. Role color via `bg_color`. Border for rim via `border_width_all` + `border_color`.

**Timer nodes:** `LongPressTimer` fires at 400ms for context actions. `RimPulseTimer` fires at 200ms for visual feedback during a press-and-hold (rim brightens to signal "keep holding"). Both are `one_shot = true` Timers, started on touch-down, stopped on touch-up.

**Minimum size:** `custom_minimum_size = Vector2(3 * unit_px, 3 * unit_px)` is enforced in `_ready()`, ensuring the 72x72px touch target minimum at 3240x2160.

---

## A6. Bracket

```
Bracket (Control)
  |-- BracketLines (Node2D)            # Container for Line2D children
        |-- ArmTop (Line2D)            # Top arm of bracket
        |-- ArmBottom (Line2D)         # Bottom arm of bracket
        |-- Spine (Line2D)             # Connecting spine
        |-- Tick_0 (Line2D)            # Optional tick marks (TICK_GROUP style)
        |-- Tick_1 (Line2D)
        |-- ... (N ticks)
```

**Rationale for Line2D:** Brackets are thin schematic strokes -- exactly what Line2D is for. Line2D supports `width` (pixel-based), `default_color`, `antialiased`, `begin_cap_mode` / `end_cap_mode` (LINE_CAP_ROUND, LINE_CAP_BOX, LINE_CAP_NONE), and `joint_mode` (LINE_JOINT_ROUND, LINE_JOINT_SHARP, LINE_JOINT_BEVEL). A bracket is 3-5 line segments forming `[`, `]`, `<`, `>`, or a tick group.

**Why Node2D container:** Line2D is a Node2D, not a Control. `BracketLines` is a Node2D that serves as a drawing container. The root `Bracket` is still a Control (for layout system participation), and `BracketLines` is positioned relative to it. The script computes Line2D point positions based on the Control's size and style parameters.

**No shader needed.** Line2D with `antialiased = true` at 260 DPI produces the crisp thin strokes the schematic layer requires. Color comes from the role palette via `default_color`.

---

## A7. SplineConnector

```
SplineConnector (Control)
  |-- SplinePath (Path2D)              # Holds Curve2D with cubic Bezier points
  |-- ConnectorLine (Line2D)           # Main stroke: baked curve points, thick
  |     material = ShaderMaterial      # hud_connector.gdshader (scanline, rim)
  |-- RimLine (Line2D)                 # 1px offset highlight (bevel illusion)
  |-- StartCap (Control)              # Cap glyph at source port (DOT/ENDCAP/BRACKET/NONE)
  |-- EndCap (Control)                # Cap glyph at dest port (DOT/ENDCAP/BRACKET/NONE)
  |-- HitArea (Control)               # Invisible, inflated bounding box for touch
```

**Render path:** `SplinePath.curve` (a `Curve2D` resource) holds the Bezier control points set by the Router. The script calls `Curve2D.get_baked_points()` to get a `PackedVector2Array`, then assigns it to `ConnectorLine.points`. `RimLine` gets the same points, offset by 1px perpendicular to the tangent at each sample, creating the single-sided bevel highlight.

**Rationale for Line2D over ribbon mesh:** Line2D is the fast-prototyping path (Option A from the routing pseudocode). It supports `width_curve` (a `Curve` resource mapping 0..1 along the line to a width multiplier) for thickness variation near endpoints, `begin_cap_mode` / `end_cap_mode` for round caps, and `antialiased` rendering. A `ShaderMaterial` on `ConnectorLine` adds the animated scanline effect for primary connectors without changing the geometry approach.

**Upgrade path to ribbon mesh:** Same `Curve2D` data, different renderer. If Line2D fidelity at 260 DPI is insufficient (stairstepping, aliasing at tight curves, bevel quality), Phase 3 can replace the two Line2D nodes with a custom `_draw()` ribbon mesh. The script API and Curve2D data model remain unchanged.

**HitArea:** An invisible `Control` sized to the bounding box of the spline plus `1.5u` padding on each side. This ensures the touch target is at least 3u wide even for thin tertiary connectors. `mouse_filter = MOUSE_FILTER_STOP`. The script checks whether touch coordinates fall within `clearance_px` of any baked polyline segment before accepting the input.

**Cap nodes:** `StartCap` and `EndCap` are small `Control` nodes positioned at the first and last baked points. They can be empty (NONE), contain a small circle (DOT -- drawn via `_draw()`), contain an LcarsEndcap instance (ENDCAP), or contain a Bracket instance (BRACKET). Cap style is set via export.

---

---

# SECTION B: Script Architecture

---

## B1. `lcars_rail.gd`

```
class_name LcarsRail extends Control

# === EXPORTS ===
@export var role: int = 0                    # HudRole enum value
@export var orientation: int = 0             # 0 = HORIZONTAL, 1 = VERTICAL
@export var thickness_u: float = 3.0         # Rail thickness in u-units
@export var segment_count: int = 1           # 1 = solid bar, >1 = segmented
@export var segment_gap_u: float = 0.25      # Gap between segments
@export var segment_ratios: Array[float] = []  # Relative widths; empty = equal distribution
@export var corner_radius_u: float = 0.0     # 0 = sharp corners

# === SIGNALS ===
signal port_positions_changed(ports: Dictionary)
signal segment_pressed(index: int)
signal segment_long_pressed(index: int)

# === INTERNAL STATE ===
# _segments: Array[Panel]               -- created/destroyed on segment_count change
# _stylebox_normal: StyleBoxFlat        -- template for normal state
# _stylebox_active: StyleBoxFlat        -- template for active/pressed state
# _press_timer: Timer                   -- for long-press detection per segment
# _active_segment: int = -1             -- currently pressed segment index

# === PUBLIC API ===
func get_port_position(side: String) -> Vector2
    # Returns global position of port on given side (N/S/E/W)
    # Computed from Control rect + orientation

func get_port_positions() -> Dictionary
    # Returns { "N": Vector2, "E": Vector2, "S": Vector2, "W": Vector2 }

func set_role(new_role: int) -> void
    # Unregisters old, registers new with HudTheme
    # Updates all segment StyleBox colors from palette

func get_segment_rects() -> Array[Rect2]
    # Returns global Rect2 for each segment Panel
    # Used by composition scenes for alignment queries

func get_thickness_px() -> float
    # Returns thickness_u * HudTheme.unit_px

# === LIFECYCLE ===
# _ready():
#   - Read unit_px from HudTheme
#   - Enforce custom_minimum_size based on thickness_u (3u on short axis for touch)
#   - Build segment Panels into SegmentContainer
#   - Apply initial role colors from palette
#   - Connect HudTheme.palette_changed
#   - Connect each segment Panel's gui_input for touch handling

# _notification(NOTIFICATION_RESIZED):
#   - Recompute port marker positions
#   - Emit port_positions_changed

# _on_segment_gui_input(event, index):
#   - Handle tap / long-press per segment
#   - Apply active StyleBox on press, restore on release
```

---

## B2. `lcars_endcap.gd`

```
class_name LcarsEndcap extends Control

# === EXPORTS ===
@export var role: int = 0                    # HudRole enum value
@export var style: int = 0                   # 0 = HALF_PILL, 1 = STEPPED
@export var direction: int = 0               # 0 = LEFT, 1 = RIGHT, 2 = UP, 3 = DOWN
                                             # Direction the rounded/pill end faces
@export var thickness_u: float = 3.0         # Must match attached rail
@export var length_u: float = 4.0            # Length of endcap along its axis
@export var pill_radius_u: float = 0.0       # 0 = auto (thickness/2 for full pill)
@export var step_depth_u: float = 1.0        # Stepped variant: notch depth
@export var step_offset_u: float = 1.0       # Stepped variant: notch position from pill end

# === SIGNALS ===
signal port_positions_changed(ports: Dictionary)

# === INTERNAL STATE ===
# _material: ShaderMaterial               -- per-instance, resource_local_to_scene
# _shader: preloaded hud_endcap.gdshader

# === PUBLIC API ===
func get_port_position(side: String) -> Vector2
    # Flat edge is the attachment side (opposite of direction)
    # E.g., direction=LEFT means pill faces left, flat edge is on the right

func get_attachment_edge() -> Rect2
    # Returns the Rect2 of the flat edge where this endcap joins a rail
    # Width = thickness, height = 0 (edge line), in global coordinates

func set_role(new_role: int) -> void
    # Unregisters material, re-registers with new role
    # HudTheme pushes new u_tint_color, u_rim_color, etc.

# === LIFECYCLE ===
# _ready():
#   - Create ShaderMaterial from hud_endcap.gdshader (resource_local_to_scene = true)
#   - Set initial uniforms: u_style, u_direction, u_radius_px, u_step_depth_px, u_step_offset_px
#   - Assign to EndcapBody.material
#   - Register with HudTheme
#   - Compute custom_minimum_size from thickness_u and length_u

# _notification(NOTIFICATION_RESIZED):
#   - Update u_radius_px (if auto, recalc from new thickness)
#   - Update port marker positions
#   - Emit port_positions_changed
```

---

## B3. `lcars_elbow.gd`

```
class_name LcarsElbow extends Control

# === EXPORTS ===
@export var role: int = 0                    # HudRole enum value
@export var rotation_index: int = 0          # 0 = TL, 1 = TR, 2 = BR, 3 = BL
@export var outer_radius_u: float = 4.0      # Outer curve radius
@export var inner_radius_u: float = 2.0      # Inner cutout radius
@export var arm_h_thickness_u: float = 3.0   # Horizontal arm height
@export var arm_v_thickness_u: float = 3.0   # Vertical arm width
@export var arm_h_length_u: float = 8.0      # Horizontal arm extends from curve
@export var arm_v_length_u: float = 6.0      # Vertical arm extends from curve

# === SIGNALS ===
signal port_positions_changed(ports: Dictionary)

# === INTERNAL STATE ===
# _material: ShaderMaterial
# _shader: preloaded hud_elbow.gdshader

# === PUBLIC API ===
func get_port_position(side: String) -> Vector2
    # PortH: end of horizontal arm
    # PortV: end of vertical arm
    # Other sides return center of the elbow's bounding box

func get_h_attachment_edge() -> Rect2
    # Rect2 at the open end of the horizontal arm
    # For snapping a horizontal LcarsRail

func get_v_attachment_edge() -> Rect2
    # Rect2 at the open end of the vertical arm
    # For snapping a vertical LcarsRail

func set_role(new_role: int) -> void

# === LIFECYCLE ===
# _ready():
#   - Create ShaderMaterial from hud_elbow.gdshader (resource_local_to_scene = true)
#   - Set uniforms: u_outer_radius_px, u_inner_radius_px, u_arm_h_px, u_arm_v_px, u_rotation
#   - Compute custom_minimum_size:
#     For TL: width = outer_radius + arm_h_length, height = outer_radius + arm_v_length
#     (adjusted per rotation_index)
#   - Register with HudTheme

# _notification(NOTIFICATION_RESIZED):
#   - Recalculate pixel-based uniforms from u-unit exports
#   - Update port marker positions
#   - Emit port_positions_changed
```

---

## B4. `glass_pane.gd`

```
class_name GlassPane extends PanelContainer

# === EXPORTS ===
@export var role: int = 5                    # Default NEUTRAL
@export var blur_lod: float = 2.0            # Mip level for textureLod blur (0 = sharp)
@export var alpha: float = 0.33              # Base glass transparency
@export var grain: float = 0.03              # Noise overlay intensity
@export var bevel_u: float = 1.0             # Bevel width in u-units
@export var radius_u: float = 2.0            # Corner radius in u-units
@export var rim_px: float = 3.0              # Rim thickness in logical pixels
@export var auto_contrast: bool = true       # Auto-thicken opacity when background luminance is close
@export var blur_enabled: bool = true        # Toggle no-blur fallback

# === SIGNALS ===
signal port_positions_changed(ports: Dictionary)
signal content_margin_ready(margin_node: MarginContainer)

# === INTERNAL STATE ===
# _material: ShaderMaterial
# _shader: preloaded hud_glass.gdshader
# _back_buffer: BackBufferCopy (child node reference)
# _glass_body: ColorRect (child node reference)
# _content_margin: MarginContainer (child node reference)

# === PUBLIC API ===
func get_port_position(side: String) -> Vector2

func get_port_positions() -> Dictionary

func get_content_container() -> MarginContainer
    # Returns the MarginContainer where composed content should be added

func set_role(new_role: int) -> void

func set_blur_enabled(enabled: bool) -> void
    # Toggles between full shader (blur + tint + grain + bevel) and
    # no-blur fallback (tint + grain + bevel only, u_blur_lod = 0)

func set_alpha(value: float) -> void
    # Updates u_alpha uniform directly

func set_grain(value: float) -> void
    # Updates u_grain uniform directly

# === LIFECYCLE ===
# _ready():
#   - Create ShaderMaterial from hud_glass.gdshader (resource_local_to_scene = true)
#   - Set initial uniforms from exports (u_blur_lod, u_alpha, u_grain, u_bevel_px, etc.)
#   - Assign material to GlassBody (ColorRect)
#   - Set PanelContainer's own StyleBox to empty (StyleBoxEmpty.new())
#   - Set mouse_filter = MOUSE_FILTER_PASS (events pass through to content)
#   - Register material with HudTheme
#   - Emit content_margin_ready(_content_margin)

# _notification(NOTIFICATION_RESIZED):
#   - Update BackBufferCopy.rect to match global rect
#   - Update u_radius_px and u_bevel_px from u-unit exports
#   - Update port marker positions
#   - Emit port_positions_changed

# _notification(NOTIFICATION_TRANSFORM_CHANGED):
#   - Update BackBufferCopy.rect (position may have changed without resize)
```

---

## B5. `chip.gd`

```
class_name Chip extends Control

# === EXPORTS ===
@export var role: int = 0                    # HudRole enum value
@export var label_text: String = ""          # Short uppercase text
@export var interactive: bool = true         # false = display-only, no touch response
@export var toggled: bool = false            # Toggle state for mode chips
@export var radius_u: float = 1.0            # Corner radius in u-units
@export var padding_u: float = 0.5           # Internal padding around label

# === SIGNALS ===
signal pressed()
signal toggled_changed(is_on: bool)
signal long_pressed()

# === INTERNAL STATE ===
# _stylebox_normal: StyleBoxFlat
# _stylebox_hover: StyleBoxFlat
# _stylebox_active: StyleBoxFlat
# _stylebox_toggled: StyleBoxFlat
# _is_pressing: bool = false
# _press_position: Vector2
# _chip_body: Panel
# _chip_label: Label
# _long_press_timer: Timer
# _rim_pulse_timer: Timer

# === PUBLIC API ===
func set_role(new_role: int) -> void
    # Rebuilds all 4 StyleBoxFlat variants with new palette colors
    # Normal: solid role color, no border emphasis
    # Hover: solid role color, border_width += 1px, border_color = rim color
    # Active: darkened tint, bevel inversion (border_width increases, color shifts)
    # Toggled: sustained bright border (rim color at full)

func set_label(text: String) -> void
    # Updates ChipLabel.text, recalculates minimum size

func set_toggled(state: bool) -> void
    # Sets toggled state, applies toggled StyleBox if true

func get_toggled() -> bool

# === LIFECYCLE ===
# _ready():
#   - Enforce custom_minimum_size = Vector2(3u, 3u)
#   - Build StyleBoxFlat variants from current palette
#   - Apply normal StyleBox to ChipBody
#   - Set ChipLabel.text = label_text
#   - Set mouse_filter based on interactive flag:
#     interactive = true  -> MOUSE_FILTER_STOP
#     interactive = false -> MOUSE_FILTER_IGNORE
#   - Connect HudTheme.palette_changed

# _gui_input(event: InputEvent):
#   - Guard: if not interactive, return
#
#   - InputEventMouseButton pressed + MOUSE_BUTTON_LEFT:
#     - _is_pressing = true
#     - _press_position = event.position
#     - Apply _stylebox_active to ChipBody
#     - Start _rim_pulse_timer (0.2s)
#     - Start _long_press_timer (0.4s)
#
#   - InputEventMouseButton released + MOUSE_BUTTON_LEFT:
#     - If _is_pressing:
#       - Stop both timers
#       - If _long_press_timer had NOT fired:
#         - This is a tap -> emit pressed()
#         - If toggled mode: flip toggled, emit toggled_changed
#       - Restore appropriate StyleBox (normal or toggled)
#     - _is_pressing = false
#
#   - InputEventMouseMotion (while pressing):
#     - If distance from _press_position > slop threshold (1u):
#       - Cancel press (stop timers, restore StyleBox)
#       - _is_pressing = false
#       - (Allows drag gestures to override chip press)
#
# _on_rim_pulse_timer_timeout():
#   - Visual: brighten border_color briefly (rim pulse feedback)
#   - Signals to user that long-press is registering
#
# _on_long_press_timer_timeout():
#   - Emit long_pressed()
#   - Visual: apply distinct "context" StyleBox (e.g., alert rim)
#   - _is_pressing remains true (release will NOT emit pressed)
```

---

## B6. `bracket.gd`

```
class_name Bracket extends Control

# === EXPORTS ===
@export var role: int = 5                    # Default NEUTRAL
@export var style: int = 0                   # 0 = SQUARE_BRACKET, 1 = ANGLE_BRACKET, 2 = TICK_GROUP
@export var orientation: int = 0             # 0 = LEFT, 1 = RIGHT, 2 = TOP, 3 = BOTTOM
@export var arm_length_u: float = 2.0        # Length of bracket arms
@export var stroke_width_px: float = 2.0     # Line thickness in logical pixels
@export var tick_count: int = 0              # TICK_GROUP only: number of ticks along spine
@export var tick_spacing_u: float = 1.0      # TICK_GROUP only: spacing between ticks
@export var tick_length_u: float = 0.5       # TICK_GROUP only: length of each tick mark

# === SIGNALS ===
# (none -- brackets are non-interactive)

# === PUBLIC API ===
func set_role(new_role: int) -> void
    # Updates default_color on all Line2D children from palette

func get_span() -> float
    # Returns total length along the bracket's primary axis (px)
    # For SQUARE/ANGLE: distance between arm tips
    # For TICK_GROUP: (tick_count - 1) * tick_spacing_u * unit_px

func rebuild_geometry() -> void
    # Recalculates all Line2D point arrays based on current exports
    # Called on export change and resize

# === LIFECYCLE ===
# _ready():
#   - Set mouse_filter = MOUSE_FILTER_IGNORE (non-interactive)
#   - Build initial Line2D geometry based on style
#   - Set Line2D properties: width = stroke_width_px, antialiased = true
#   - Set begin_cap_mode = LINE_CAP_ROUND, end_cap_mode = LINE_CAP_ROUND
#     (for SQUARE_BRACKET and TICK_GROUP)
#   - For ANGLE_BRACKET: end_cap_mode = LINE_CAP_NONE (pointed)
#   - Apply role color from palette to all Line2D default_color
#   - Connect HudTheme.palette_changed

# _notification(NOTIFICATION_RESIZED):
#   - rebuild_geometry()

# Geometry construction per style:
#
# SQUARE_BRACKET (orientation=LEFT):
#   ArmTop:    [(arm_length, 0), (0, 0)]
#   Spine:     [(0, 0), (0, height)]
#   ArmBottom: [(0, height), (arm_length, height)]
#
# ANGLE_BRACKET (orientation=LEFT):
#   ArmTop:    [(arm_length, 0), (0, height/2)]
#   ArmBottom: [(0, height/2), (arm_length, height)]
#
# TICK_GROUP (orientation=LEFT):
#   Spine:     [(0, 0), (0, total_tick_span)]
#   Tick_N:    [(0, N*spacing), (tick_length, N*spacing)]
#
# Other orientations: mirror/rotate point coordinates
```

---

## B7. `spline_connector.gd`

```
class_name SplineConnector extends Control

# === EXPORTS ===
@export var role: int = 0                    # HudRole enum value
@export var importance: int = 50             # Determines visual class (primary/secondary/tertiary)
@export var start_cap_style: int = 0         # 0=NONE, 1=DOT, 2=ENDCAP, 3=BRACKET
@export var end_cap_style: int = 0           # 0=NONE, 1=DOT, 2=ENDCAP, 3=BRACKET
@export var scanline_speed: float = 0.0      # >0 enables animated scanline (primary only)
@export var bake_interval_px: float = 8.0    # Distance between baked curve samples

# === SIGNALS ===
signal connector_tapped(connector: SplineConnector)
signal connector_long_pressed(connector: SplineConnector)
signal connector_hovered(connector: SplineConnector)
signal connector_unhovered(connector: SplineConnector)

# === INTERNAL STATE ===
# _material: ShaderMaterial                -- on ConnectorLine
# _curve: Curve2D                          -- held by SplinePath
# _baked_points: PackedVector2Array        -- cached after bake
# _connector_line: Line2D
# _rim_line: Line2D
# _hit_area: Control
# _start_cap: Control
# _end_cap: Control
# _long_press_timer: Timer
# _is_pressing: bool = false

# === PUBLIC API (called by Router, not user) ===
func set_curve_points(p0: Vector2, p1: Vector2, p2: Vector2, p3: Vector2) -> void
    # Clears Curve2D, adds p0 with out-handle (p1-p0),
    # adds p3 with in-handle (p2-p3)
    # Bakes, updates Line2D points, repositions caps and hit area

func set_points_from_bake(points: PackedVector2Array) -> void
    # Directly sets pre-baked points (bypasses Curve2D)
    # Used when Router has already baked (e.g., BUS_BRANCH composite)

func set_bus_segment(entry: Vector2, exit: Vector2) -> void
    # Sets a straight-line segment for the bus portion of BUS_BRANCH
    # Renders as a thick straight connector (rail-like)

func get_baked_points() -> PackedVector2Array
    # Returns the cached baked polyline

func set_role(new_role: int) -> void

func set_importance(value: int) -> void
    # Recalculates visual class:
    #   >= 80: width = 2.5u, alpha = 0.8, scanline enabled (if speed > 0)
    #   50-79: width = 1.5u, alpha = 0.6, scanline disabled
    #   < 50:  width = 0.75u, alpha = 0.4, scanline disabled
    # Updates ConnectorLine.width, RimLine.width, shader uniforms

func get_start_position() -> Vector2
    # First baked point (source port location)

func get_end_position() -> Vector2
    # Last baked point (dest port location)

func get_bounding_rect() -> Rect2
    # Bounding box of all baked points + padding

# === LIFECYCLE ===
# _ready():
#   - Create ShaderMaterial from hud_connector.gdshader (resource_local_to_scene = true)
#   - Assign to ConnectorLine.material
#   - Set ConnectorLine: antialiased = true, begin_cap_mode = LINE_CAP_ROUND,
#     end_cap_mode = LINE_CAP_ROUND, joint_mode = LINE_JOINT_ROUND
#   - Set RimLine: antialiased = true, width = 1.0 (px)
#   - Apply initial importance-based class
#   - Register material with HudTheme

# _on_hit_area_gui_input(event):
#   - Verify touch/click is actually near the polyline (distance check)
#   - Handle tap (emit connector_tapped) and long-press (emit connector_long_pressed)
#   - On mouse enter (pen/mouse): emit connector_hovered
#   - On mouse exit: emit connector_unhovered

# _bake_and_render():
#   - Bake SplinePath.curve -> _baked_points
#   - Assign to ConnectorLine.points
#   - Compute rim offset points (1px perpendicular offset at each sample)
#   - Assign to RimLine.points
#   - Resize root Control to bounding rect of points
#   - Position/resize HitArea to bounding rect + 1.5u padding
#   - Position StartCap at first point, EndCap at last point
#   - Instantiate cap glyphs based on start_cap_style / end_cap_style

# _compute_rim_offset(points: PackedVector2Array) -> PackedVector2Array:
#   - For each point, compute tangent from neighbors
#   - Perpendicular = tangent.orthogonal().normalized()
#   - Offset point = original + perpendicular * 1.0
#   - Returns offset array (same length as input)
```

---

---

# SECTION C: Shader Requirements

---

## C1. Which Primitives Need Custom Shaders

| Primitive | Custom Shader? | Rendering Approach | Why |
|-----------|---------------|-------------------|-----|
| LcarsRail | NO | StyleBoxFlat on Panel | Solid colored rectangles. StyleBoxFlat handles bg_color, per-corner radii, borders |
| LcarsEndcap | YES | SDF on ColorRect | Half-pill + stepped variants need SDF. StyleBoxFlat cannot do stepped cutout |
| LcarsElbow | YES | SDF on ColorRect | L-shape with inner/outer radii needs SDF boolean subtract |
| GlassPane | YES | Screen-read + SDF on ColorRect | Blur (textureLod + hint_screen_texture), tint, grain, bevel all require shader |
| Chip | NO | StyleBoxFlat on Panel | Simple rounded rects. Interaction states via StyleBox property swaps |
| Bracket | NO | Line2D native rendering | Thin schematic strokes. Line2D with antialiased = true is sufficient |
| SplineConnector | YES | Shader on Line2D material | Scanline animation, core/rim rendering on the polyline |

**Total: 4 custom shaders needed.**

---

## C2. Uniform Contracts Per Shader

All uniforms use `u_` prefix per established convention. Standard uniforms pushed by HudTheme are marked with [PUSH]. Primitive-specific uniforms are set by the primitive's script.

### `hud_endcap.gdshader`

```
shader_type canvas_item;

// --- HudTheme-pushed uniforms [PUSH] ---
uniform vec3 u_tint_color : source_color;        // Role solid color
uniform vec3 u_rim_color : source_color;          // Role rim color
uniform float u_alpha;                            // Base opacity (1.0 for solid endcaps)
uniform float u_rim_px;                           // Rim thickness in pixels
uniform float u_bevel_px;                         // Bevel width in pixels
uniform float u_state;                            // 0=normal, 1=hover, 2=active, 3=disabled
uniform float u_bevel_scale;                      // Interaction modifier (1.0 normal, 1.3 hover)
uniform float u_rim_alpha;                        // Interaction modifier (1.0 normal, 1.5 hover)

// --- Primitive-specific uniforms ---
uniform float u_radius_px;                        // Pill radius (auto = node_height/2)
uniform int u_style;                              // 0 = HALF_PILL, 1 = STEPPED
uniform int u_direction;                          // 0=LEFT, 1=RIGHT, 2=UP, 3=DOWN
uniform float u_step_depth_px;                    // Stepped variant: notch depth
uniform float u_step_offset_px;                   // Stepped variant: notch position
```

### `hud_elbow.gdshader`

```
shader_type canvas_item;

// --- HudTheme-pushed uniforms [PUSH] ---
uniform vec3 u_tint_color : source_color;
uniform vec3 u_rim_color : source_color;
uniform float u_alpha;
uniform float u_rim_px;
uniform float u_bevel_px;
uniform float u_state;
uniform float u_bevel_scale;
uniform float u_rim_alpha;

// --- Primitive-specific uniforms ---
uniform float u_outer_radius_px;                  // Outer curve radius
uniform float u_inner_radius_px;                  // Inner cutout radius
uniform float u_arm_h_px;                         // Horizontal arm thickness (px)
uniform float u_arm_v_px;                         // Vertical arm thickness (px)
uniform int u_rotation;                           // 0=TL, 1=TR, 2=BR, 3=BL
```

### `hud_glass.gdshader`

```
shader_type canvas_item;

// --- Screen texture sampling ---
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

// --- HudTheme-pushed uniforms [PUSH] ---
uniform vec3 u_tint_color : source_color;         // Role solid color (unused for glass body)
uniform vec3 u_glass_tint : source_color;          // Glass tint color (darker, more muted)
uniform vec3 u_rim_color : source_color;
uniform float u_alpha;                             // Base glass transparency (~0.33)
uniform float u_rim_px;                            // Rim thickness
uniform float u_bevel_px;                          // Bevel width
uniform float u_state;                             // 0=normal, 1=hover, 2=active, 3=disabled
uniform float u_bevel_scale;
uniform float u_rim_alpha;

// --- Primitive-specific uniforms ---
uniform float u_blur_lod;                          // textureLod mip level (0=sharp, 2-3=blurry)
uniform float u_grain;                             // Noise intensity (0.0-0.1 typical)
uniform float u_radius_px;                         // Corner radius
uniform float u_contrast_min;                      // Auto-contrast threshold (0.0 = disabled)
```

### `hud_connector.gdshader`

```
shader_type canvas_item;

// --- HudTheme-pushed uniforms [PUSH] ---
uniform vec3 u_tint_color : source_color;          // Role color for core stroke
uniform vec3 u_rim_color : source_color;            // Highlight color
uniform float u_alpha;                              // Core stroke alpha
uniform float u_state;                              // 0=normal, 1=hover, 2=active
uniform float u_rim_alpha;

// --- Primitive-specific uniforms ---
uniform float u_stroke_width;                       // Core width (set from importance class)
uniform float u_rim_px;                             // Rim highlight width
uniform float u_scanline_speed;                     // 0 = no animation, >0 = px/sec scroll
uniform float u_scanline_alpha;                     // Scanline overlay intensity
```

**Note on Line2D UV mapping:** When a `ShaderMaterial` is applied to a Line2D, Godot maps `UV.x` along the polyline length (0.0 at start, 1.0 at end) and `UV.y` across the width (0.0 at one edge, 1.0 at the other). The connector shader uses `UV.x` for the scanline sweep (TIME-based scroll) and `UV.y` for the rim highlight (thin bright band near one edge).

---

## C3. Shared Shader Includes

### `hud_sdf.gdshaderinc`

Shared SDF utility functions. Used by: `hud_endcap.gdshader`, `hud_elbow.gdshader`, `hud_glass.gdshader`.

**Functions provided:**

```
float sdf_box(vec2 p, vec2 half_size)
    // Axis-aligned box SDF

float sdf_rounded_rect(vec2 p, vec2 half_size, vec4 radii)
    // Rounded rectangle SDF with per-corner radii
    // radii = vec4(top_left, top_right, bottom_right, bottom_left)

float sdf_pill(vec2 p, vec2 half_size, float radius, int direction)
    // Half-pill: one end rounded (radius = half_size.y for full), one flat
    // direction controls which end is rounded

float sdf_subtract(float d1, float d2)
    // Boolean subtraction: d1 AND NOT d2
    // Returns max(d1, -d2)

float sdf_union(float d1, float d2)
    // Boolean union: min(d1, d2)

vec3 sdf_bevel(float dist, float bevel_width, vec2 uv, vec2 light_dir)
    // Returns vec3(highlight, shadow, rim) intensities
    // highlight: inner bright band on light_dir side
    // shadow: inner dark band on opposite side
    // rim: thin bright line at dist == 0

float sdf_rim(float dist, float rim_width)
    // Returns rim intensity: 1.0 when abs(dist) < rim_width, 0.0 otherwise
    // Smooth falloff via smoothstep

vec4 apply_bevel_to_color(vec4 base_color, vec3 bevel, float bevel_scale, float rim_color_mix, vec3 rim_color)
    // Composites bevel highlight/shadow/rim onto base color
    // bevel_scale: interaction modifier (1.0 normal, 1.3 hover, 0.7 active)
```

### `hud_noise.gdshaderinc`

Noise utility functions. Used by: `hud_glass.gdshader`.

**Functions provided:**

```
float hash21(vec2 p)
    // Fast 2D -> 1D hash (no texture lookup)

float blue_noise_approx(vec2 uv, float time)
    // Approximated blue noise using interleaved hash layers
    // time parameter for temporal variation (avoids static grain pattern)

vec4 grain_overlay(vec4 base_color, float noise_value, float intensity)
    // Mixes noise into base color
    // Additive blend: base + (noise - 0.5) * intensity
    // Preserves alpha channel
```

---

---

# SECTION D: Touch Interaction

---

## D1. LcarsRail -- Interactive (Segments)

**Classification:** Interactive. Each segment is a separate touch target.

**Touch target sizing:**
- Short axis of each segment must be >= 3u (72px). The script enforces this: if `thickness_u < 3.0`, `custom_minimum_size` on the short axis is clamped to `3u`.
- Long axis has no minimum (segments can be any length), but adjacent segment gaps must be >= 1u (24px) between their Panel edges. The `segment_gap_u` export enforces this (minimum value clamped to 0.25u; the gap plus StyleBoxFlat borders create sufficient separation).

**Input path:**
1. Touch event -> Godot gui_input propagation -> individual segment Panel (mouse_filter = STOP)
2. Script's `_on_segment_gui_input(event, index)` receives it
3. Tap (press + release within 400ms, within 1u slop) -> `segment_pressed.emit(index)`
4. Long-press (400ms timer fires) -> `segment_long_pressed.emit(index)`

**State feedback:**
- Normal: StyleBoxFlat with role solid color, no border emphasis
- Active/Pressed (touch-down): StyleBoxFlat with darkened bg_color (tint deepened), border_width_all += 2px (bevel inversion simulation), border_color = rim color
- Hover (mouse/pen only): StyleBoxFlat with slightly brighter bg_color, border_width_all += 1px, border_color = rim color at 50% alpha

**No-hover compliance:** Active state is visually distinct from normal without any preceding hover. Touch users see: normal -> active (instant) -> normal (release).

**Drag gesture:** Swipe along the rail edge (detected as `InputEventMouseMotion` with button pressed, movement primarily along the rail axis) emits a `rail_swiped(direction)` signal for collapse/expand of adjacent modules. This is detected by the rail, not individual segments.

---

## D2. LcarsEndcap -- Non-Interactive

**Classification:** Non-interactive structural element.

**mouse_filter:** `MOUSE_FILTER_IGNORE` on root Control. All input passes through.

**Rationale:** Endcaps are visual terminations of rails. They carry no interactive function. Tapping an endcap taps whatever is behind it (glass pane, viewport, nothing).

---

## D3. LcarsElbow -- Non-Interactive

**Classification:** Non-interactive structural element.

**mouse_filter:** `MOUSE_FILTER_IGNORE` on root Control.

**Rationale:** Elbows are structural joins connecting perpendicular rails. Like endcaps, they carry no interactive function. The corner area of an elbow is not a meaningful tap target.

---

## D4. GlassPane -- Pass-Through

**Classification:** Pass-through (not interactive itself, but content inside is).

**mouse_filter:** `MOUSE_FILTER_PASS` on root PanelContainer. Events propagate to children (the `ContentMargin` and its contents), which have their own mouse_filter settings.

**Rationale:** The glass pane is a visual surface. Users interact with *content inside* the pane (chips, sliders, text fields), not with the glass itself. `MOUSE_FILTER_PASS` lets the PanelContainer participate in event routing (so children receive events) without the pane consuming them.

**Touch note:** No special touch handling on the pane itself. Multi-touch gestures (pinch-to-zoom on viewport) pass through the pane to the viewport below, since `MOUSE_FILTER_PASS` does not consume events.

---

## D5. Chip -- Interactive (Primary Touch Target)

**Classification:** Interactive. Primary touch target for mode selection, tag toggling, blend type selection.

**Touch target sizing:**
- `custom_minimum_size = Vector2(3u, 3u)` = `Vector2(72, 72)` at native res.
- Preferred size for frequently-used chips: 4u x 3u or larger.
- Adjacent chips must have >= 1u (24px) gap. Enforced by the composing container's `add_theme_constant_override("separation", unit_px)`.

**Input path (full trace):**
1. Touch-down -> Godot emulates mouse via `emulate_mouse_from_touch` -> `_gui_input(InputEventMouseButton)` on Chip (mouse_filter = STOP)
2. Immediately: apply `_stylebox_active`, start `_rim_pulse_timer` (0.2s), start `_long_press_timer` (0.4s)
3. At 200ms: `_rim_pulse_timer` fires -> border_color brightens momentarily (visual "still holding" signal)
4. At 400ms: `_long_press_timer` fires -> emit `long_pressed()`, apply context StyleBox
5. On release before 400ms: stop timers, emit `pressed()`, if toggle mode flip state, restore StyleBox
6. On drag away (> 1u from press point): cancel press, restore StyleBox, allow drag gesture to propagate

**State feedback:**
- Normal: Role solid bg_color, corner_radius from token, 1px border at role rim color
- Hover (mouse/pen only): Brighter bg_color, border_width += 1px
- Active/Pressed: Darkened bg_color (inset feel), border_width = 3px, border_color = bright rim
- Toggled (sustained): Border stays at 2px with bright rim color; bg_color has slight saturation boost
- Long-press context: Border color shifts to ALERT rim color (magenta/red), signaling context action active

**No-hover compliance:** Active state (darkened + thick border) is unmistakable from normal state. Touch users see immediate visual feedback on contact.

---

## D6. Bracket -- Non-Interactive

**Classification:** Non-interactive structural/decorative element.

**mouse_filter:** `MOUSE_FILTER_IGNORE` on root Control.

**Rationale:** Brackets are thin schematic strokes for visual framing. They have no interactive function. They should never intercept touch events.

---

## D7. SplineConnector -- Interactive (Tap-to-Select)

**Classification:** Interactive, but with a non-standard hit detection model (inflated bounding box + polyline proximity check).

**Touch target sizing:**
- The `HitArea` Control is sized to the bounding box of the baked polyline + 1.5u padding on each side.
- Even for tertiary connectors (0.75u visual width), the hit area is >= 3u wide because of the padding.
- The actual hit test is a distance check: touch point must be within 1.5u of any segment in the baked polyline.

**Input path:**
1. Touch-down on `HitArea` (mouse_filter = STOP) -> `_on_hit_area_gui_input(event)`
2. Script computes minimum distance from touch point to all polyline segments
3. If distance <= `1.5u * unit_px` (hit confirmed):
   - First tap: emit `connector_tapped(self)` -- highlights the net
   - Second tap (if already highlighted): activates the connection
   - Long-press (400ms): emit `connector_long_pressed(self)` -- context action
4. If distance > threshold: consume event but do nothing (prevents pass-through to pane below while within bounding box)

**State feedback:**
- Normal: Core stroke at role color, rim highlight at low alpha
- Hover (mouse/pen): Alpha boost on core + rim, width increase by 0.5u
- Tapped/Selected (first tap): Alpha boost, rim brightens, optional endpoint bracket ticks appear
- Active (second tap): Net pulses (alpha oscillation 0.1-0.2 amplitude via scanline shader)

**No-hover compliance:** First tap performs both selection AND highlight simultaneously. Touch users see: normal -> selected+highlighted (on tap) -> activated (on second tap). Mouse users get hover as an enhancement before tap.

**Multi-touch note:** Connectors can be tapped while another finger is performing a two-finger viewport gesture. The `HitArea` consumes the event independently via `gui_input`, which does not interfere with `_unhandled_input()` multi-touch handling on the viewport layer.

---

---

# SECTION E: Composition API

---

## E1. Rail + Endcap Attachment

Endcaps attach to the flat end of a rail. Two composition modes:

### Container-based composition (preferred for linear layouts):
```
HBoxContainer
  |-- LcarsEndcap (direction=RIGHT)     # Pill faces left, flat edge on right
  |-- LcarsRail (orientation=HORIZONTAL)
  |-- LcarsEndcap (direction=LEFT)      # Pill faces right, flat edge on left
```
The container handles alignment. Endcap `thickness_u` must match rail `thickness_u` -- this is the composer's responsibility (convention, not enforced by code).

### Code-driven composition (for non-linear layouts):
```
# Position endcap at rail's east port
var edge = rail.get_port_position("E")
endcap.position = edge
endcap.thickness_u = rail.thickness_u
endcap.direction = 0  # LEFT -- pill faces away from rail
```
The `get_attachment_edge()` method on LcarsEndcap returns a `Rect2` describing the flat edge, useful for snapping validation.

### Vertical variant:
```
VBoxContainer
  |-- LcarsEndcap (direction=DOWN)
  |-- LcarsRail (orientation=VERTICAL)
  |-- LcarsEndcap (direction=UP)
```

---

## E2. Rail + Elbow Attachment

Elbows join two perpendicular rails. The elbow sits at the junction, and rails extend from its arm ends.

### Typical composition (top-left corner):
```
Control (manual positioning)
  |-- LcarsElbow (rotation_index=TL)
  |     arm_h extends right, arm_v extends down
  |-- LcarsRail (horizontal)
  |     position = elbow.get_h_attachment_edge().position
  |-- LcarsRail (vertical)
  |     position = elbow.get_v_attachment_edge().position
```

The elbow exposes:
- `get_h_attachment_edge() -> Rect2` -- the open end of the horizontal arm
- `get_v_attachment_edge() -> Rect2` -- the open end of the vertical arm

Rails snap to these edges. Thickness matching is the composer's responsibility: `rail.thickness_u == elbow.arm_h_thickness_u` for horizontal, `rail.thickness_u == elbow.arm_v_thickness_u` for vertical.

### All four rotation variants:
```
TL (0): H-arm right, V-arm down   -- top-left corner of a frame
TR (1): H-arm left,  V-arm down   -- top-right corner
BR (2): H-arm left,  V-arm up     -- bottom-right corner
BL (3): H-arm right, V-arm up     -- bottom-left corner
```

---

## E3. Elbow + Endcap at Frame Corners

A common LCARS pattern: elbow at corner with endcaps terminating the arms that don't continue as rails.

```
Control (top-left frame corner)
  |-- LcarsElbow (rotation_index=TL)
  |-- LcarsRail (horizontal, extends right from elbow H-arm)
  |-- LcarsEndcap (direction=DOWN)
  |     Positioned at elbow V-arm end if no vertical rail continues
```

---

## E4. GlassPane Relative to Rails

Glass panes sit *behind* rails in z-order. The layer ordering from the design doc:

```
HUDLayer (CanvasLayer)
  |-- GlassLayer (Control, z_index = 0)
  |     |-- StackGlass (GlassPane)        # Left panel glass
  |     |-- BackBufferCopy                 # Required between overlapping panes
  |     |-- InspectorGlass (GlassPane)    # Right panel glass
  |
  |-- StructureLayer (Control, z_index = 1)
  |     |-- TopLeftElbow (LcarsElbow)
  |     |-- TopRail (LcarsRail)
  |     |-- LeftRail (LcarsRail)
  |     |-- TopRightElbow (LcarsElbow)
  |     |-- ... endcaps
  |
  |-- SchematicLayer (Control, z_index = 2)
  |     |-- FrameBracket_0 (Bracket)
  |     |-- Connector_0 (SplineConnector)
  |     |-- Connector_1 (SplineConnector)
  |
  |-- TextLayer (Control, z_index = 3)
        |-- Labels, readouts, icons
```

Glass pane bounds typically overlap or abut the rail structure. Rails *frame* the pane visually but are not children of the pane. No data coupling between glass and rails -- they are independently positioned by the composing scene's layout.

### BackBufferCopy between overlapping panes

When two GlassPane instances overlap, a `BackBufferCopy` node must be inserted between them in the GlassLayer:

```
GlassLayer
  |-- PaneA (GlassPane)           # Reads SCREEN_TEXTURE -> sees world
  |-- BackBufferCopy               # Captures rendered PaneA into back buffer
  |     copy_mode = COPY_MODE_RECT
  |     rect = union(PaneA.rect, PaneB.rect)
  |-- PaneB (GlassPane)           # Reads SCREEN_TEXTURE -> sees world + PaneA
```

Without this intermediate BackBufferCopy, PaneB would read the same screen texture as PaneA (before PaneA rendered), causing PaneA to appear invisible where PaneB overlaps it.

For non-overlapping panes, no intermediate BackBufferCopy is needed -- each pane's internal BackBufferCopy is sufficient.

---

## E5. SplineConnector Port Position Reading

**SplineConnector does NOT reference other primitives directly.** The decoupling principle:

1. Every composed module (not raw primitives, but composed assemblies from Phase 2+) implements a **port interface**: `get_port_positions() -> Dictionary` returning `{ "side_name": Vector2 }` in global coordinates.

2. Raw primitives provide `get_port_position(side: String) -> Vector2` for their own boundary ports. Composed modules aggregate these.

3. The **Router** (Phase 2+ system, not part of Phase 1) collects all module port data, runs the routing algorithm from the routing pseudocode doc, and creates SplineConnector instances:

```
# Router pseudocode (Phase 2):
var p0 = source_module.get_port_position("E")
var p3 = dest_module.get_port_position("W")
var d = p0.distance_to(p3)
var t = clamp(d * 0.25, 3 * unit_px, 16 * unit_px)
var p1 = p0 + Vector2(1, 0) * t   # East-facing normal
var p2 = p3 + Vector2(-1, 0) * t  # West-facing approach

var connector = SplineConnector.new()
connector.set_curve_points(p0, p1, p2, p3)
connector.role = net_spec.role
connector.importance = net_spec.importance
schematic_layer.add_child(connector)
```

4. SplineConnector is a **dumb renderer** -- it draws what it's told. It does not know about source/destination modules, routing decisions, or port semantics.

---

## E6. Chip Inside GlassPane

Chips are content children of a GlassPane. They sit in the `ContentMargin`:

```
GlassPane (StackGlass)
  |-- BackBufferCopy
  |-- GlassBody
  |-- ContentMargin (MarginContainer)
        |-- ChipRow (HBoxContainer)
              |-- Chip (role=BLEND, label="SCREEN")
              |-- Chip (role=BLEND, label="MULTIPLY")
              |-- Chip (role=BLEND, label="OVERLAY")
```

The glass pane's `get_content_container()` returns `ContentMargin`. Composers add children there. The chips render on top of the glass because `ContentMargin` comes after `GlassBody` in tree order.

---

## E7. Bracket Framing a Module

Brackets sit in the SchematicLayer, positioned to frame a module boundary:

```
SchematicLayer
  |-- InspectorFrame_L (Bracket, style=SQUARE, orientation=LEFT)
  |     position = inspector_glass.position + Vector2(-2u, 1u)
  |     custom_minimum_size.y = inspector_glass.size.y - 2u
  |-- InspectorFrame_R (Bracket, style=SQUARE, orientation=RIGHT)
  |     position = inspector_glass.position + Vector2(inspector_glass.size.x + 0.5u, 1u)
  |     custom_minimum_size.y = inspector_glass.size.y - 2u
```

Brackets don't attach to other primitives programmatically. They are positioned relative to the modules they frame, either via manual positioning or via script that reads module rects and places brackets accordingly.

---

---

# SECTION F: Shader File Manifest

---

| File Path | Type | Purpose | Consumed By |
|-----------|------|---------|-------------|
| `hud/shaders/hud_sdf.gdshaderinc` | Shader include | SDF functions: rounded_rect, pill, box, subtract, union, bevel, rim | hud_endcap, hud_elbow, hud_glass |
| `hud/shaders/hud_noise.gdshaderinc` | Shader include | Noise functions: hash21, blue_noise_approx, grain_overlay | hud_glass |
| `hud/shaders/hud_endcap.gdshader` | CanvasItem shader | Half-pill and stepped endcap SDF rendering + bevel + rim | LcarsEndcap ColorRect |
| `hud/shaders/hud_elbow.gdshader` | CanvasItem shader | L-shape elbow SDF with boolean subtract + bevel + rim | LcarsElbow ColorRect |
| `hud/shaders/hud_glass.gdshader` | CanvasItem shader | Screen-read blur + tint + grain + SDF bevel + rim + no-blur fallback | GlassPane ColorRect |
| `hud/shaders/hud_connector.gdshader` | CanvasItem shader | Core stroke color + rim highlight + scanline animation | SplineConnector Line2D |

**Total: 2 `.gdshaderinc` includes + 4 `.gdshader` files = 6 shader files.**

**Primitives without custom shaders:** LcarsRail (StyleBoxFlat), Chip (StyleBoxFlat), Bracket (Line2D native). These three use no shader files.

---

---

# SECTION G: Complete File Manifest

---

## All Phase 1 files

| # | File Path | Type | Purpose |
|---|-----------|------|---------|
| **Shader Includes** | | | |
| 1 | `hud/shaders/hud_sdf.gdshaderinc` | Shader include | Shared SDF utilities (rounded_rect, pill, subtract, bevel, rim) |
| 2 | `hud/shaders/hud_noise.gdshaderinc` | Shader include | Shared noise utilities (hash, blue noise, grain overlay) |
| **Shaders** | | | |
| 3 | `hud/shaders/hud_endcap.gdshader` | CanvasItem shader | Endcap rendering (half-pill + stepped variants) |
| 4 | `hud/shaders/hud_elbow.gdshader` | CanvasItem shader | Elbow rendering (L-shape SDF boolean) |
| 5 | `hud/shaders/hud_glass.gdshader` | CanvasItem shader | Glass pane (blur + tint + grain + bevel) |
| 6 | `hud/shaders/hud_connector.gdshader` | CanvasItem shader | Connector stroke + rim + scanline |
| **Scenes** | | | |
| 7 | `hud/primitives/lcars_rail.tscn` | Scene | LcarsRail: segmented solid rail |
| 8 | `hud/primitives/lcars_endcap.tscn` | Scene | LcarsEndcap: half-pill/stepped terminator |
| 9 | `hud/primitives/lcars_elbow.tscn` | Scene | LcarsElbow: 90-degree corner join |
| 10 | `hud/primitives/glass_pane.tscn` | Scene | GlassPane: translucent blur panel |
| 11 | `hud/primitives/chip.tscn` | Scene | Chip: interactive rounded tag block |
| 12 | `hud/primitives/bracket.tscn` | Scene | Bracket: schematic stroke group |
| 13 | `hud/primitives/spline_connector.tscn` | Scene | SplineConnector: Curve2D-based spline renderer |
| **Scripts** | | | |
| 14 | `hud/primitives/lcars_rail.gd` | GDScript | Rail segmentation, port positions, touch handling |
| 15 | `hud/primitives/lcars_endcap.gd` | GDScript | Endcap shader setup, attachment edge, direction |
| 16 | `hud/primitives/lcars_elbow.gd` | GDScript | Elbow arm config, rotation, attachment edges |
| 17 | `hud/primitives/glass_pane.gd` | GDScript | Glass lifecycle, BackBufferCopy sync, content access |
| 18 | `hud/primitives/chip.gd` | GDScript | Chip tap/long-press/toggle interaction |
| 19 | `hud/primitives/bracket.gd` | GDScript | Bracket Line2D geometry generation |
| 20 | `hud/primitives/spline_connector.gd` | GDScript | Curve management, baking, Line2D feed, importance classes |

**Total: 20 files (6 shader + 7 scene + 7 script)**

---

## Directory tree after Phase 1

```
hud/
|-- tokens/                              # Phase 0 (prerequisite, not Phase 1)
|   |-- hud_tokens.gd
|   |-- hud_tokens.tres
|   |-- hud_palette.gd
|   |-- hud_role.gd
|   |-- palette_ops_amber.tres
|   |-- palette_cyan_tools.tres
|   |-- palette_violet_blend.tres
|   |-- palette_green_telemetry.tres
|   |-- palette_magenta_alert.tres
|   |-- palette_neutral_smoke.tres
|-- theme/                               # Phase 0 (prerequisite, not Phase 1)
|   |-- hud_theme.gd
|-- shaders/                             # Phase 1
|   |-- hud_sdf.gdshaderinc
|   |-- hud_noise.gdshaderinc
|   |-- hud_endcap.gdshader
|   |-- hud_elbow.gdshader
|   |-- hud_glass.gdshader
|   |-- hud_connector.gdshader
|-- primitives/                          # Phase 1
    |-- lcars_rail.tscn
    |-- lcars_rail.gd
    |-- lcars_endcap.tscn
    |-- lcars_endcap.gd
    |-- lcars_elbow.tscn
    |-- lcars_elbow.gd
    |-- glass_pane.tscn
    |-- glass_pane.gd
    |-- chip.tscn
    |-- chip.gd
    |-- bracket.tscn
    |-- bracket.gd
    |-- spline_connector.tscn
    |-- spline_connector.gd
```

---

---

# SECTION H: Decision Log

---

## Decision 1: Control vs Node2D as Primitive Base Class

**Chosen:** `Control` for all 7 primitives.

**Alternatives considered:**
- **Node2D for all:** Full render freedom via Transform2D. Position/draw without layout constraints. Better for freeform visuals.
- **Node2D for SplineConnector only, Control for rest:** Mixed paradigm. Connectors are freeform; structural primitives are layout-driven.
- **Control for all:** Layout system integration (anchors, size_flags, custom_minimum_size, containers). Theme integration. Native `gui_input` for touch/mouse.

**Rationale:** The HUD is a layout-driven UI system. Primitives live inside `HBoxContainer`, `VBoxContainer`, `MarginContainer` -- they need to resize, anchor, and flow with the layout. The canonical LCARS image shows structured zoning, not freeform art. `Control` provides layout participation, `custom_minimum_size` for touch target enforcement, `mouse_filter` for input routing, and `gui_input` for touch handling -- all essential.

SplineConnector is the one primitive where Node2D might have been simpler (freeform curves do not need layout). But keeping all primitives as Control maintains a uniform composition contract: every primitive can live in any container, respond to `get_port_position()`, and participate in the same sizing/anchoring system. The connector's Control is sized to its bounding box, which works fine.

**Risk:** Control's rect-based sizing means the SplineConnector's bounding box consumes layout space even where the curve does not visually fill it. Mitigated by placing connectors in the SchematicLayer with manual positioning, not in layout containers.

---

## Decision 2: Line2D vs Ribbon Mesh for SplineConnector

**Chosen:** Line2D (Option A from routing pseudocode).

**Alternatives considered:**
- **Ribbon mesh + CanvasItem shader (Option B):** Build a quad strip along the polyline with per-vertex normals. UV-map along length. Full shader control for bevel, core/rim, scanline, glow. Higher visual fidelity. More complex to implement.
- **Custom `_draw()` method:** Draw directly in the Control's `_draw()` callback using `draw_polyline()`. Simpler than ribbon mesh but loses Line2D's built-in cap/joint handling.
- **Line2D with ShaderMaterial:** Built-in node with width_curve, cap modes, joint modes, antialiased. Apply a ShaderMaterial for scanline/rim effects. Moderate visual fidelity.

**Rationale:** Phase 1 establishes the primitive API and proves the composition model. Visual polish is Phase 3. Line2D is fast to implement, visually adequate for proving routing, and upgradeable. The `Curve2D` data that feeds it is the same data that would feed a ribbon mesh -- the upgrade path changes the *renderer*, not the *data model* or *public API*.

Line2D's `width_curve` (a `Curve` resource mapping 0..1 along the line to a width multiplier) enables thickness variation near endpoints. `begin_cap_mode = LINE_CAP_ROUND` and `end_cap_mode = LINE_CAP_ROUND` handle smooth caps. A `ShaderMaterial` applied to the Line2D's `material` property provides the scanline effect.

**Upgrade trigger:** If Line2D at 260 DPI shows visible stairstepping at tight curves, aliasing on the rim highlight, or insufficient bevel quality, upgrade to ribbon mesh in Phase 3. The threshold is a visual review during Phase 2 demo.

---

## Decision 3: Shared SDF Shader Include vs Per-Primitive Shaders

**Chosen:** Shared `hud_sdf.gdshaderinc` referenced via `#include` by per-primitive `.gdshader` files.

**Alternatives considered:**
- **All-in-one per-primitive shaders:** Each shader contains its own SDF functions. No dependencies between files. Code duplication across 3 shaders (endcap, elbow, glass).
- **Single mega-shader for all primitives:** One `.gdshader` handling all primitives via uniform-driven branching. Complex, hard to maintain, wastes GPU cycles on unused branches.
- **Shared `.gdshaderinc` includes + per-primitive `.gdshader` files:** SDF utilities in one include file. Each shader `#include`s it and adds primitive-specific logic.

**Rationale:** `sdf_rounded_rect`, `sdf_bevel`, `sdf_rim`, and `apply_bevel_to_color` are identical across endcap, elbow, and glass. Duplicating them creates maintenance burden and risks divergence (fix bevel in one, forget the others). Godot supports `#include "res://path/to/file.gdshaderinc"` in `.gdshader` files. The include provides the shared SDF vocabulary; each shader adds its specific geometry (half-pill for endcap, L-shape for elbow, screen-read for glass).

`hud_noise.gdshaderinc` is only used by glass, but is still separated to keep the glass shader focused on composition logic rather than containing raw noise functions inline.

---

## Decision 4: Elbow-Rail Connection -- Scene Composition vs Code-Driven

**Chosen:** Hybrid -- layout containers for linear flow, code-driven positioning at junctions, attachment-edge methods for validation.

**Alternatives considered:**
- **Pure scene composition:** Elbows and rails in containers. Godot layout handles alignment. No code needed for positioning.
- **Pure code-driven:** Script reads attachment edges and sets positions. Full control, but fragile to resize and hard to inspect in editor.
- **Hybrid:** Containers for linear rail+endcap sequences. Manual positioning for elbow junctions (since an L-shape cannot live in a single linear container). Attachment-edge methods provide the data for manual placement.

**Rationale:** An elbow joins a horizontal rail to a vertical rail -- it is a junction point, not a linear sequence. It cannot live in a single `HBoxContainer` or `VBoxContainer`. The elbow must be positioned at the corner, and rails extend from its arms in perpendicular directions.

The hybrid approach: use containers for the linear parts (`HBox` for horizontal rail + endcaps, `VBox` for vertical rail + endcaps), and position the elbow at the junction via script using `get_h_attachment_edge()` and `get_v_attachment_edge()`. These methods return `Rect2` values that the composer uses to snap rails to the correct positions.

Phase 2 (Demo HUD) will establish the exact composition patterns and may introduce helper scripts or scenes that automate common frame layouts (e.g., `LcarsFrame.tscn` that wires up 4 elbows + 4 rails + endcaps).

---

## Decision 5: StyleBoxFlat vs Shader for LcarsRail and Chip

**Chosen:** StyleBoxFlat for both, with documented upgrade path to shader overlay.

**Alternatives considered:**
- **Shader for everything:** Uniform rendering approach. Every primitive gets SDF bevel/rim. Visual consistency guaranteed.
- **StyleBoxFlat where sufficient:** Use native Godot rendering for simple shapes. Add shader only when StyleBoxFlat cannot express the visual.

**Rationale:** LcarsRail segments are solid colored rectangles with optional rounded corners and borders. StyleBoxFlat handles this with `bg_color`, `corner_radius_*`, `border_width_*`, `border_color`. No shader overhead, and the segments are inspectable/editable in the Godot editor.

Chips are small rounded rectangles. StyleBoxFlat with uniform `corner_radius_all` handles them identically.

The anti-Metro checklist requires "bevel + rim respond to interaction states." For Phase 1, rails and chips simulate this via StyleBoxFlat property swaps: border_width increases on hover/active, border_color shifts to rim color, bg_color darkens on press. This is not true SDF bevel (no light-direction highlight/shadow), but it produces a visible state change.

**Upgrade path (Phase 2 review):** If the visual review reveals that StyleBoxFlat state changes look too flat compared to the shader-rendered endcaps/elbows/glass, add a `ShaderOverlay` child (a `ColorRect` with a thin SDF bevel shader) to rails and chips. This is an additive change: add one child node, add one shader file (`hud_solid_bevel.gdshader`), no other changes.

---

## Decision 6: Per-Instance Materials vs Shared Materials

**Chosen:** Per-instance materials (`resource_local_to_scene = true`).

**This is a settled Phase 0 decision, not re-derived here.** Documenting for completeness:

Per-instance materials allow two primitives of the same role to have different `u_state` values simultaneously (one hovered, one normal). The push model's per-frame cost is zero -- pushes happen only on palette change or interaction state change. Memory cost is negligible (ShaderMaterial objects are lightweight).

The alternative (shared materials per role) would prevent per-instance state variation and require a different approach to interaction states (e.g., shader branching on instance data). Rejected.

---

## Decision 7: Bracket Rendering -- Line2D vs `_draw()` vs Shader

**Chosen:** Line2D child nodes.

**Alternatives considered:**
- **Line2D:** Built-in node. Antialiased. Cap modes. Width. Color. Points editable in editor. Multiple Line2D children for complex bracket shapes.
- **`_draw()` override:** Custom drawing via `draw_line()`, `draw_polyline()` in the Control's `_draw()` method. More flexible point management but not inspectable in editor.
- **Shader:** SDF-based bracket rendering on a ColorRect. Overkill for thin lines with no fill, no bevel, no transparency.

**Rationale:** Brackets are thin schematic strokes -- 2-4px lines forming shapes like `[`, `]`, `<`, `>` with optional ticks. Line2D is purpose-built for this. It is inspectable in the editor (points visible and editable), supports `antialiased = true` for crisp rendering at 260 DPI, and `default_color` is directly settable from the role palette.

Multiple Line2D children (ArmTop, Spine, ArmBottom, ticks) keep the geometry organized and independently styleable. A `_draw()` approach would consolidate all line drawing into one method, losing per-arm inspectability.

---

## Decision 8: GlassPane Root Node -- Panel vs PanelContainer vs Control

**Chosen:** `PanelContainer` root.

**Alternatives considered:**
- **Panel:** Draws a StyleBox background. Visual conflict with the shader-driven glass rendering (two layers drawing backgrounds).
- **PanelContainer:** Container that draws a StyleBox AND manages child layout with margins. Setting its StyleBox to empty eliminates the visual conflict while retaining container behavior.
- **Plain Control:** No built-in child layout. Would need a manual MarginContainer child for content inset, plus explicit sizing logic.

**Rationale:** GlassPane exists to *contain* content (text, chips, controls). PanelContainer provides:
1. Container behavior -- children fill the panel with configurable margins (theme-derived or override)
2. Layout system participation -- anchors, size_flags, custom_minimum_size all work
3. Automatic margin management -- add_theme_constant_override("margin_*") for content inset

We disable its visual rendering by setting its StyleBox to `StyleBoxEmpty.new()`. All visual rendering is done by the `GlassBody` ColorRect child with its screen-reading shader.

Plain Control would work but requires manually adding and wiring a MarginContainer child, which PanelContainer already provides built-in.

---

## Decision 9: SplineConnector Hit Detection -- Inflated Bounding Box + Distance Check

**Chosen:** Invisible `HitArea` Control sized to bounding box + 1.5u padding, with polyline proximity check in the input handler.

**Alternatives considered:**
- **Line2D built-in input:** Line2D does not natively support gui_input. It is a Node2D, not a Control. Even if wrapped in a Control, the hit testing would be rect-based, not polyline-based.
- **Area2D with CollisionPolygon2D:** Generate a collision polygon from the inflated polyline. Accurate but heavyweight for UI hit detection, and Area2D does not integrate with the Control gui_input pipeline.
- **Custom Control with rect-based hit + polyline distance check:** Use a Control-sized bounding box for coarse hit detection (Godot's built-in rect check), then refine with a polyline distance check in the input handler.

**Rationale:** The chosen approach leverages Godot's gui_input propagation (which works with Control rects) for coarse filtering, then adds a precise distance check for hit confirmation. This avoids the overhead of physics-based collision while ensuring thin connectors are still touchable within the 3u minimum target.

The 1.5u padding on each side of the bounding box ensures the hit area extends well beyond the visual stroke, meeting the touch target requirement even for thin tertiary connectors (0.75u visual width + 1.5u padding on each side = 3.75u total hit width).

---

## Decision 10: PortMarkers -- Marker2D vs Dictionary-Only

**Chosen:** `Marker2D` nodes for port positions, exposed via `get_port_position()` API.

**Alternatives considered:**
- **Dictionary-only:** Compute port positions on demand from the Control's rect. No scene nodes. Positions are ephemeral.
- **Marker2D nodes:** Persistent scene nodes at port positions. Visible in editor. Positions update on resize.

**Rationale:** Marker2D nodes provide:
1. **Editor visibility:** Port positions are visible when selecting the primitive in the Godot editor. Aids manual composition and debugging.
2. **Global position tracking:** `Marker2D.global_position` automatically accounts for parent transforms. No manual `to_global()` conversion needed.
3. **Signal-based updates:** The primitive can watch for `NOTIFICATION_RESIZED` and reposition markers, then emit `port_positions_changed` for any listeners.

The overhead is minimal (Marker2D is one of the lightest Godot nodes). For a system with at most hundreds of primitives on screen, this is negligible.

---

---

# SECTION I: Phase 1 Build Order

---

## Dependency Graph

```
                   +---> hud_endcap.gdshader ---> LcarsEndcap (.tscn + .gd)
                   |
hud_sdf.gdshaderinc --+---> hud_elbow.gdshader  ---> LcarsElbow (.tscn + .gd)
                   |
                   +---> hud_glass.gdshader   ---> GlassPane (.tscn + .gd)
                   |
hud_noise.gdshaderinc -+

hud_connector.gdshader ---------> SplineConnector (.tscn + .gd)

(no shader deps) ---> LcarsRail (.tscn + .gd)
(no shader deps) ---> Chip (.tscn + .gd)
(no shader deps) ---> Bracket (.tscn + .gd)
```

Key dependencies:
- `hud_sdf.gdshaderinc` must exist before `hud_endcap.gdshader`, `hud_elbow.gdshader`, or `hud_glass.gdshader` can be compiled
- `hud_noise.gdshaderinc` must exist before `hud_glass.gdshader` can be compiled
- `hud_connector.gdshader` has NO shared include dependencies (standalone)
- LcarsRail, Chip, and Bracket have ZERO shader dependencies
- All primitives depend on Phase 0 (HudRole, HudTokens, HudPalette, HudTheme) for runtime registration

---

## Build Steps

### Step 0: Phase 0 Stubs (BLOCKING PREREQUISITE)

Before any Phase 1 work, the following must exist (at minimum as stubs with hardcoded values):

- `hud/tokens/hud_role.gd` -- enum with 6 values
- `hud/tokens/hud_tokens.gd` -- Resource class with unit_px, radii, bevel widths (can return defaults)
- `hud/tokens/hud_tokens.tres` -- Default instance
- `hud/tokens/hud_palette.gd` -- Resource class with role-to-color lookups (can return hardcoded Ops Amber)
- `hud/tokens/palette_ops_amber.tres` -- At least one palette
- `hud/theme/hud_theme.gd` -- Autoload with register_material/unregister_material stubs, palette_changed signal

These can be minimal stubs. The real implementation happens in Phase 0 proper, but Phase 1 primitives need the interfaces to compile and register.

### Step 1: Non-Shader Primitives (PARALLEL)

**Can be built simultaneously by independent devs. No shader dependencies.**

| Primitive | Dev | Deliverables | Estimated Complexity |
|-----------|-----|-------------|---------------------|
| LcarsRail | godot-dev | `lcars_rail.tscn`, `lcars_rail.gd` | Medium (segmentation logic, dynamic child management) |
| Chip | godot-dev | `chip.tscn`, `chip.gd` | Medium (touch interaction, timer-based long-press, state machine) |
| Bracket | godot-dev | `bracket.tscn`, `bracket.gd` | Low (Line2D geometry, 3 style variants) |

**Validation gate:** Each primitive can be instantiated in a test scene, role-colored via hardcoded palette values, resized, and interacted with (for Rail and Chip).

### Step 2: Shader Includes

**Must complete before Step 3. Can be built in parallel with Step 1.**

| File | Dev | Deliverables | Estimated Complexity |
|------|-----|-------------|---------------------|
| `hud_sdf.gdshaderinc` | shader-writer | SDF functions (rounded_rect, pill, box, subtract, bevel, rim) | Medium-High (correct SDF math, bevel light model) |
| `hud_noise.gdshaderinc` | shader-writer | Noise functions (hash, blue_noise_approx, grain_overlay) | Low (standard hash functions) |

**Validation gate:** Include files compile without errors when `#include`d in a minimal test shader.

### Step 3: SDF-Based Solid Primitives (PARALLEL)

**Depends on Step 2 (hud_sdf.gdshaderinc). Can be built simultaneously.**

| Primitive | Dev | Deliverables | Estimated Complexity |
|-----------|-----|-------------|---------------------|
| LcarsEndcap | shader-writer + godot-dev | `hud_endcap.gdshader`, `lcars_endcap.tscn`, `lcars_endcap.gd` | Medium (SDF half-pill + stepped variant, direction flipping) |
| LcarsElbow | shader-writer + godot-dev | `hud_elbow.gdshader`, `lcars_elbow.tscn`, `lcars_elbow.gd` | Medium (SDF boolean subtract, 4 rotation variants) |

**Validation gate:** Each renders correctly in all orientations/rotations, bevel/rim visible, role color applied via HudTheme.

### Step 4: GlassPane

**Depends on Step 2 (hud_sdf.gdshaderinc + hud_noise.gdshaderinc). Build after Step 3 to benefit from proven SDF include.**

| Primitive | Dev | Deliverables | Estimated Complexity |
|-----------|-----|-------------|---------------------|
| GlassPane | shader-writer + godot-dev | `hud_glass.gdshader`, `glass_pane.tscn`, `glass_pane.gd` | HIGH (screen-read via hint_screen_texture, textureLod blur, BackBufferCopy sync, grain, SDF bevel, no-blur fallback, content container) |

**Validation gate:** Glass pane renders over 3D viewport with visible blur, tint, grain. Bevel/rim visible. Content children render on top. BackBufferCopy rect stays synchronized with pane position/size. No-blur fallback works when toggled.

**This is the most complex primitive.** Screen-reading shaders have ordering subtleties. Build it after the simpler SDF primitives validate the include files.

### Step 5: SplineConnector

**Depends only on `hud_connector.gdshader` (no shared includes). Built last because:**
1. It has the most complex public API (curve management, baking, importance classes, cap instantiation, hit detection)
2. It benefits from seeing how other primitives' `get_port_position()` APIs settled
3. The Router (Phase 2) is the primary consumer -- Phase 1 only needs the primitive to accept curve data and render it

| Primitive | Dev | Deliverables | Estimated Complexity |
|-----------|-----|-------------|---------------------|
| SplineConnector | shader-writer + godot-dev | `hud_connector.gdshader`, `spline_connector.tscn`, `spline_connector.gd` | HIGH (Curve2D management, baking, Line2D feed, rim offset computation, hit detection, importance-class switching, cap instantiation) |

**Validation gate:** Connector renders a cubic Bezier between two points with correct thickness, rim highlight, and (for primary importance) scanline animation. Tap detection works on the inflated hit area. Width changes on importance change.

---

## Summary Timeline

```
        Step 0           Step 1 (parallel)         Step 2 (parallel with 1)
    Phase 0 stubs    LcarsRail | Chip | Bracket    hud_sdf.inc | hud_noise.inc
         |                |                              |
         v                v                              v
                     Step 3 (parallel, after Step 2)
                   LcarsEndcap | LcarsElbow
                          |
                          v
                     Step 4 (after Step 3)
                       GlassPane
                          |
                          v
                     Step 5 (after Step 4 or parallel)
                     SplineConnector
```

Steps 1 and 2 overlap freely -- non-shader primitives do not wait for shader includes.
Step 5 (SplineConnector) can technically overlap with Step 4 (GlassPane) since they have no shared shader dependencies, but building GlassPane first is recommended to validate the full SDF include chain before adding connector complexity.

---

---

# Appendix A: Anti-Metro Compliance Checklist

---

Verifying Phase 1 primitives satisfy the anti-Metro requirements from the design doc:

- [x] **Varied radii by component type:**
  - Rails: `corner_radius_u = 0` or `r_sm (2u)` -- blocky
  - Endcaps: `radius = thickness/2` (full pill) or stepped -- organic
  - Elbows: `outer = r_lg (4u)`, `inner = r_md (3u)` -- large sweeping curves
  - GlassPane: `radius_u = r_md (3u)` -- medium rounds
  - Chips: `radius_u = r_sm (2u)` -- small rounds
  - All different. No uniform radius across the system.

- [x] **Endcap language present:** LcarsEndcap provides half-pill and stepped variants. Two distinct termination profiles. Connector caps add DOT, ENDCAP, and BRACKET cap styles.

- [x] **Micro-structure exists:** Bracket primitive provides ticks, separators, schematic strokes. Rail segmentation creates visual subdivision within rails. Connector rim highlights add micro-detail to splines.

- [x] **Bevel + rim respond to interaction states:**
  - Shader-based primitives: `u_state` uniform drives bevel_scale and rim_alpha modifiers
  - StyleBoxFlat primitives: border_width and border_color change on hover/active
  - Not flat color swaps -- dimensional response via bevel/rim

- [x] **Asymmetrical composition:** Not enforced at primitive level (this is Phase 2 composition concern), but primitives *support* asymmetry: elbow arms have independent lengths/thicknesses, rail segment_ratios allow non-equal segments, bracket tick groups are inherently non-symmetric

- [x] **All interactive elements >= 3u touch target:**
  - Chip: `custom_minimum_size = Vector2(3u, 3u)` = 72x72px
  - Rail segments: thickness clamped to >= 3u on short axis
  - SplineConnector: HitArea padded to >= 3u width
  - Non-interactive primitives (Endcap, Elbow, Bracket): not applicable

- [x] **Adjacent targets have >= 1u gap:**
  - Rail segment gaps: `segment_gap_u >= 0.25u` minimum (visual), but the gap between interactive segment Panels is enforced by container separation
  - Chip rows: container separation = 1u
  - Enforced by composing layouts, not primitives themselves

- [x] **Active/pressed state visually sufficient without hover:**
  - Chip: darkened bg + thick border on press (instant, no hover needed)
  - Rail segments: darkened bg + border emphasis on press
  - Connectors: alpha boost + rim brighten on first tap
  - All produce unmistakable visual change from normal state

---

---

# Appendix B: Project Settings Changes (Deferred to Phase 0 Implementation)

---

The current `project.godot` has viewport 1920x1080 with `canvas_items` stretch mode. The HUD design targets:

```
display/window/size/viewport_width = 3240
display/window/size/viewport_height = 2160
display/window/stretch/mode = "canvas_items"
display/window/stretch/aspect = "expand"
```

This change should be made during Phase 0 implementation, not during Phase 1 primitive authoring. Primitives are designed resolution-independently via the `u` unit system (all sizes computed from `HudTheme.unit_px`), so they work at any viewport size. But design-time visual validation should happen at the target resolution.

---

---

# Appendix C: Render Path Traces (Decision Framework Requirement)

For each visual primitive, the complete data-to-screen path:

---

**LcarsRail:**
`role (int)` -> `HudTheme.get_solid_color(role)` -> `StyleBoxFlat.bg_color` on each segment Panel -> Godot Panel draws StyleBoxFlat -> screen pixel

**LcarsEndcap:**
`role (int)` -> `HudTheme.register_material(mat, role)` -> HudTheme pushes `u_tint_color`, `u_rim_color` to ShaderMaterial -> `hud_endcap.gdshader` fragment() reads uniforms, computes SDF (pill or stepped), applies bevel/rim -> ColorRect renders -> screen pixel

**LcarsElbow:**
`role (int)` -> `HudTheme.register_material(mat, role)` -> HudTheme pushes uniforms -> `hud_elbow.gdshader` fragment() computes L-shape SDF (union of arms - inner cutout), applies bevel/rim -> ColorRect renders -> screen pixel

**GlassPane:**
`role (int)` -> HudTheme pushes `u_glass_tint`, `u_rim_color` etc. -> `hud_glass.gdshader` fragment():
1. `textureLod(SCREEN_TEXTURE, SCREEN_UV, u_blur_lod)` reads blurred world
2. Mix with `u_glass_tint` at `u_alpha`
3. Add grain via `grain_overlay()` from hud_noise.gdshaderinc
4. Compute SDF rounded rect for bevel/rim
5. Composite bevel onto tinted glass
-> ColorRect renders -> screen pixel
(BackBufferCopy captures screen before this shader reads it)

**Chip:**
`role (int)` -> `HudTheme.get_solid_color(role)` -> `StyleBoxFlat.bg_color`, `StyleBoxFlat.border_color` -> Godot Panel draws StyleBoxFlat -> Label draws text on top -> screen pixel

**Bracket:**
`role (int)` -> `HudTheme.get_rim_color(role)` or schematic color -> `Line2D.default_color` -> Godot Line2D renders antialiased polyline -> screen pixel

**SplineConnector:**
`Curve2D control points` -> `Curve2D.get_baked_points()` -> `Line2D.points` assignment -> `hud_connector.gdshader` fragment(): UV.y for rim band, UV.x + TIME for scanline, `u_tint_color` for core -> Line2D renders through shader -> screen pixel

---

---

# Appendix D: Input Path Traces (Decision Framework Requirement)

For each interactive primitive, the complete input-to-state-change path:

---

**LcarsRail segment:**
Touch-down -> Godot emulate_mouse_from_touch -> `InputEventMouseButton` -> gui_input propagation -> segment Panel (mouse_filter = STOP) -> `_on_segment_gui_input(event, index)` -> start timers, apply active StyleBox -> on release: stop timers, emit `segment_pressed(index)` signal -> on 400ms timeout: emit `segment_long_pressed(index)` signal

**Chip:**
Touch-down -> Godot emulate_mouse_from_touch -> `InputEventMouseButton` -> gui_input propagation -> Chip Control (mouse_filter = STOP) -> `_gui_input(event)` -> start _rim_pulse_timer (0.2s), start _long_press_timer (0.4s), apply _stylebox_active -> at 0.2s: rim pulse visual -> at 0.4s: emit `long_pressed()` -> on release before 0.4s: emit `pressed()`, optionally flip toggled state, emit `toggled_changed(state)`

**SplineConnector:**
Touch-down -> Godot emulate_mouse_from_touch -> `InputEventMouseButton` -> gui_input propagation -> HitArea Control (mouse_filter = STOP) -> `_on_hit_area_gui_input(event)` -> compute distance from touch point to nearest polyline segment -> if distance <= 1.5u: start press timer -> on release: emit `connector_tapped(self)` -> on 400ms timeout: emit `connector_long_pressed(self)`

**GlassPane (pass-through):**
Touch-down -> gui_input propagation -> GlassPane PanelContainer (mouse_filter = PASS) -> event continues to ContentMargin children -> specific interactive child (Chip, slider, etc.) handles it

**Non-interactive primitives (Endcap, Elbow, Bracket):**
Touch-down -> gui_input propagation -> primitive (mouse_filter = IGNORE) -> event passes through to whatever is behind in the tree

---

End of Phase 1 Architectural Blueprint.
