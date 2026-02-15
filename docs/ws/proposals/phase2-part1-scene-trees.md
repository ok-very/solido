# Phase 2 — Demo HUD Blueprint

## Part 1: Scene Trees

---

## 1. WorkbenchHUD — Top-Level Scene Tree

The WorkbenchHUD is a `CanvasLayer` (layer 10) that overlays the world viewport. All HUD content lives here. A single `SubViewportContainer` provides the 3D world feed that glass panes sample via `hint_screen_texture`.

```
WorkbenchHUD (CanvasLayer, layer=10)
├── WorldFeed (SubViewportContainer, stretch=true, size=3240×2160)
│   └── WorldViewport (SubViewport, size=3240×2160)
│       └── [3D scene root — camera, environment, meshes]
│
├── GlassLayer (Control, full_rect, z_index=0)
│   │   # Glass panes sample the world. BackBufferCopy between overlapping panes.
│   ├── StackGlass (GlassPane, role=NAV, anchors: left 0–0.22, top 0.06–0.94)
│   ├── BackBufferCopy (copy_mode=RECT, rect tracks StackGlass)
│   ├── InspectorGlass (GlassPane, role=EDIT, anchors: right 0.74–1.0, top 0.04–0.94)
│   ├── BackBufferCopy (copy_mode=RECT, rect tracks InspectorGlass)
│   └── StatusGlass (GlassPane, role=TELEMETRY, anchors: bottom 0.94–1.0, left 0.0–1.0)
│
├── StructureLayer (Control, full_rect, z_index=1)
│   │   # Solid LCARS: rails, elbows, endcaps — opaque, highest contrast
│   ├── StackPanel (hud/composed/stack_panel.tscn)
│   ├── InspectorPanel (hud/composed/inspector_panel.tscn)
│   └── StatusStrip (hud/composed/status_strip.tscn)
│
├── ConnectorLayer (Control, full_rect, z_index=2, mouse_filter=IGNORE)
│   │   # Schematic: spline connectors, brackets, tick groups
│   ├── [SplineConnector instances — dynamically managed by ConnectorManager]
│   └── [Bracket decorations — static, placed per-module]
│
└── TextLayer (Control, full_rect, z_index=3, mouse_filter=IGNORE)
    │   # Labels and icons — always on top, always crisp
    ├── StackLabels (Control)
    ├── InspectorLabels (Control)
    └── StatusLabels (Control)
```

**Key decisions:**
- `WorldFeed` is a sibling under the same CanvasLayer, not a separate CanvasLayer. The `SubViewportContainer` renders the 3D viewport at full resolution; glass shaders sample it via `SCREEN_UV`.
- `BackBufferCopy` nodes sit *between* adjacent `GlassPane` siblings to prevent double-blur feedback. The BackBufferCopy rect tracks the *preceding* glass pane's rect (updated on resize via `NOTIFICATION_RESIZED`).
- Four z-index layers enforce paint order: glass(0) → structure(1) → connectors(2) → text(3).
- `ConnectorLayer` and `TextLayer` have `mouse_filter = IGNORE` — interaction goes through StructureLayer controls.

---

## 2. StackPanel — Left Module (NAV)

The StackPanel is the left-side navigation panel: layer stack, node list, history. It anchors left (0–~720px at 3240w) and runs from below the top elbow to above the bottom elbow.

```
StackPanel (Control, anchors: left edge)
├── StackElbow (LcarsElbow)
│   rotation_index = TL
│   role = NAV
│   outer_radius_u = 4.0, inner_radius_u = 2.0
│   arm_h_thickness_u = 3.0, arm_v_thickness_u = 3.0
│   arm_h_length_u = 10.0, arm_v_length_u = 4.0
│
├── StackRail (LcarsRail)
│   orientation = VERTICAL (1)
│   role = NAV
│   thickness_u = 3.0
│   segment_count = 3
│   segment_ratios = [0.4, 0.35, 0.25]
│   segment_gap_u = 0.5
│   corner_radius_u = 0.0
│   # Segments: LAYERS / NODES / HISTORY (top to bottom)
│
├── StackEndcap (LcarsEndcap)
│   style = HALF_PILL
│   direction = DOWN
│   role = NAV
│   thickness_u = 3.0, length_u = 3.0
│
├── StackContentArea (Control, positioned right of rail)
│   │   # Content sits on the GlassPane (StackGlass in GlassLayer)
│   │   # This Control is positioned to overlay StackGlass's content margin
│   ├── StackChipBar (HBoxContainer, separation=1u)
│   │   ├── Chip (role=NAV, label="LAYERS", toggled=true)
│   │   ├── Chip (role=NAV, label="NODES")
│   │   └── Chip (role=NAV, label="HISTORY")
│   │
│   ├── StackList (VBoxContainer)
│   │   │   # Populated by schema — layer items, node items, etc.
│   │   └── [placeholder items for demo]
│   │
│   └── StackFooterBracket (Bracket)
│       style = TICK_GROUP
│       orientation = BOTTOM
│       role = NAV
│       tick_count = 5, tick_spacing_u = 2.0, tick_length_u = 0.5
│
└── StackRailLabel (Label, theme_type_variation="HudRailHeader")
    text = "NAV"
    # Positioned on first rail segment, rotated -90° for vertical reading
```

**Attachment geometry:**
- `StackElbow.H` port → `StackRail` top edge (the rail starts where the elbow's horizontal arm ends).
- `StackRail.S` port → `StackEndcap.PortFlat` (endcap attaches at rail bottom).
- `StackContentArea` is positioned to the right of the rail, offset by `1u` gap, aligned vertically with the glass pane content margin.

**Connector ports exposed:**
- `stack.active` — E side, pos_u=0.2, role=BLEND, priority=90 (active layer → inspector)
- `stack.selection` — E side, pos_u=0.5, role=NAV, priority=70 (selection → status)
- `stack.telemetry` — S side, pos_u=0.5, role=TELEMETRY, priority=40 (perf data → status)

---

## 3. InspectorPanel — Right Module (EDIT)

The InspectorPanel is the right-side property editor: collapsible sections, parameter rows, numeric readouts. It anchors right (~2400–3240px at 3240w).

```
InspectorPanel (Control, anchors: right edge)
├── InspectorElbow (LcarsElbow)
│   rotation_index = TR
│   role = EDIT
│   outer_radius_u = 3.0, inner_radius_u = 1.5
│   arm_h_thickness_u = 2.5, arm_v_thickness_u = 2.5
│   arm_h_length_u = 8.0, arm_v_length_u = 3.0
│   # Deliberately smaller radii than StackElbow — varied radii (Anti-Metro)
│
├── InspectorRail (LcarsRail)
│   orientation = VERTICAL (1)
│   role = EDIT
│   thickness_u = 2.5
│   segment_count = 4
│   segment_ratios = [0.15, 0.35, 0.3, 0.2]
│   segment_gap_u = 0.25
│   corner_radius_u = 0.5
│   # Segments: HEADER / TRANSFORM / MATERIAL / RENDER
│
├── InspectorEndcap (LcarsEndcap)
│   style = STEPPED
│   direction = DOWN
│   role = EDIT
│   thickness_u = 2.5, length_u = 3.5
│   step_depth_u = 1.0, step_offset_u = 1.0
│   # Stepped endcap — different from Stack's half-pill (endcap language variety)
│
├── InspectorContentArea (Control, positioned left of rail)
│   ├── InspectorHeader (HBoxContainer)
│   │   ├── Label (theme_type_variation="HudSectionHeader", text="INSPECTOR")
│   │   └── Chip (role=EDIT, label="LOCK", interactive=true)
│   │
│   ├── InspectorSections (VBoxContainer, separation=1u)
│   │   ├── TransformSection (CollapsibleSection)
│   │   │   ├── SectionHeader (HBoxContainer)
│   │   │   │   ├── Chip (role=EDIT, label="TRANSFORM", toggled=true)
│   │   │   │   └── Bracket (style=ANGLE_BRACKET, orientation=RIGHT, role=EDIT)
│   │   │   └── SectionContent (VBoxContainer)
│   │   │       ├── [ParamRow: Position X/Y/Z — Label + HudValue]
│   │   │       ├── [ParamRow: Rotation X/Y/Z]
│   │   │       └── [ParamRow: Scale X/Y/Z]
│   │   │
│   │   ├── MaterialSection (CollapsibleSection)
│   │   │   ├── SectionHeader (HBoxContainer)
│   │   │   │   ├── Chip (role=BLEND, label="MATERIAL")
│   │   │   │   └── Bracket (style=ANGLE_BRACKET, orientation=RIGHT, role=BLEND)
│   │   │   └── SectionContent (VBoxContainer)
│   │   │       ├── [ParamRow: Albedo — color swatch + hex]
│   │   │       ├── [ParamRow: Roughness — slider + value]
│   │   │       └── [ParamRow: Metallic — slider + value]
│   │   │
│   │   └── RenderSection (CollapsibleSection)
│   │       ├── SectionHeader (HBoxContainer)
│   │       │   ├── Chip (role=TELEMETRY, label="RENDER")
│   │       │   └── Bracket (style=TICK_GROUP, orientation=RIGHT, role=TELEMETRY, tick_count=3)
│   │       └── SectionContent (VBoxContainer)
│   │           ├── [ParamRow: Shader — dropdown]
│   │           └── [ParamRow: Passes — numeric]
│   │
│   └── InspectorFooterBracket (Bracket)
│       style = SQUARE_BRACKET
│       orientation = RIGHT
│       role = EDIT
│       arm_length_u = 1.5
│
└── InspectorRailLabel (Label, theme_type_variation="HudRailHeader")
    text = "EDIT"
```

**Anti-Metro audit:**
- Elbow uses smaller radii (3u outer vs Stack's 4u) — ✓ varied radii
- Endcap is STEPPED vs Stack's HALF_PILL — ✓ endcap language variety
- Rail corner_radius_u=0.5 vs Stack's 0.0 — ✓ further radius variation
- Sections use mixed Bracket styles (ANGLE + TICK_GROUP) — ✓ micro-structure
- Content is left-of-rail (mirrored from Stack's right-of-rail) — ✓ asymmetric

**Connector ports exposed:**
- `inspector.focus` — W side, pos_u=0.35, role=BLEND, priority=90 (receives from stack.active)
- `inspector.shader` — W side, pos_u=0.6, role=EDIT, priority=60 (shader binding)
- `inspector.telemetry` — S side, pos_u=0.5, role=TELEMETRY, priority=40

**Collapsible sections:**
Each `CollapsibleSection` is a composed scene (not a new primitive — composition only). The section header Chip toggles visibility of `SectionContent`. When collapsed, the Chip untoggled state removes `SectionContent` from layout (`.visible = false`). The Bracket beside the header serves as a visual anchor — it does not change on collapse.

---

## 4. StatusStrip — Bottom Module (TELEMETRY)

The StatusStrip is the bottom bar: tool mode, selection info, performance readouts, render mode. It spans the full width below the viewport, above the screen edge.

```
StatusStrip (Control, anchors: bottom edge, full width)
├── StatusRail (LcarsRail)
│   orientation = HORIZONTAL (0)
│   role = TELEMETRY
│   thickness_u = 2.0
│   segment_count = 4
│   segment_ratios = [0.2, 0.35, 0.25, 0.2]
│   segment_gap_u = 0.5
│   corner_radius_u = 0.25
│   # Segments: MODE / SELECTION / PERF / RENDER
│
├── StatusEndcapLeft (LcarsEndcap)
│   style = HALF_PILL
│   direction = LEFT
│   role = TELEMETRY
│   thickness_u = 2.0, length_u = 2.5
│
├── StatusEndcapRight (LcarsEndcap)
│   style = STEPPED
│   direction = RIGHT
│   role = TELEMETRY
│   thickness_u = 2.0, length_u = 2.5
│   step_depth_u = 0.75, step_offset_u = 0.75
│   # Left=HALF_PILL, Right=STEPPED — asymmetric terminations
│
├── StatusContentArea (Control, positioned above the rail)
│   ├── StatusModeChips (HBoxContainer, separation=1u)
│   │   ├── Chip (role=NAV, label="SELECT")
│   │   ├── Chip (role=EDIT, label="MOVE")
│   │   ├── Chip (role=EDIT, label="ROTATE")
│   │   └── Chip (role=BLEND, label="SCULPT")
│   │
│   ├── StatusSelectionInfo (HBoxContainer)
│   │   ├── Label (theme_type_variation="HudLabel", text="SEL:")
│   │   ├── Label (theme_type_variation="HudValue", text="Sphere_01")
│   │   └── Bracket (style=SQUARE_BRACKET, orientation=LEFT, role=NAV, arm_length_u=1.0)
│   │
│   ├── StatusTelemetry (HBoxContainer, separation=2u)
│   │   ├── TelemetryBlock (VBoxContainer)
│   │   │   ├── Label (theme_type_variation="HudMicro", text="FPS")
│   │   │   └── Label (theme_type_variation="HudValue", text="60.0")
│   │   ├── TelemetryBlock (VBoxContainer)
│   │   │   ├── Label (theme_type_variation="HudMicro", text="VRAM")
│   │   │   └── Label (theme_type_variation="HudValue", text="1.2 GB")
│   │   ├── TelemetryBlock (VBoxContainer)
│   │   │   ├── Label (theme_type_variation="HudMicro", text="TRIS")
│   │   │   └── Label (theme_type_variation="HudValue", text="42.1K")
│   │   └── Bracket (style=TICK_GROUP, orientation=BOTTOM, role=TELEMETRY, tick_count=4, tick_spacing_u=3.0)
│   │
│   └── StatusRenderMode (HBoxContainer)
│       ├── Chip (role=TELEMETRY, label="SOLID")
│       └── Chip (role=TELEMETRY, label="WIRE")
│
└── StatusRailLabels (Control)
    ├── Label (theme_type_variation="HudRailHeader", text="TELEMETRY")
    └── # Positioned centered on the rail, no rotation (horizontal rail)
```

**Layout notes:**
- StatusStrip is thinner than side panels (2u rail vs 2.5–3u) — deliberate hierarchy.
- Content sits *above* the rail (in the glass pane area), the rail itself is the bottom edge anchor.
- Left endcap is HALF_PILL, right endcap is STEPPED — asymmetric.
- Telemetry block uses `HudMicro` (1u / 24px) for labels and `HudValue` (1.5u / 36px) for numbers — Oxanium numeric font for tight alignment.
- Tick group bracket frames the telemetry readouts — micro-structure.

**Connector ports exposed:**
- `status.mode` — N side, pos_u=0.15, role=NAV, priority=50
- `status.selection` — N side, pos_u=0.4, role=NAV, priority=60
- `status.perf` — N side, pos_u=0.7, role=TELEMETRY, priority=30
