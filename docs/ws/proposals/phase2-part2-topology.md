# Phase 2 — Demo HUD Blueprint

## Part 2: File Manifest, Connector Topology, Viewport Integration

---

## 1. File Manifest

### New composed scenes (`hud/composed/`)

| File | Type | Purpose |
|------|------|---------|
| `hud/composed/stack_panel.gd` | GDScript | StackPanel controller: manages chip toggles, rail segment presses, section switching, exposes ports |
| `hud/composed/stack_panel.tscn` | Scene | StackPanel scene tree (LcarsElbow + LcarsRail + LcarsEndcap + content area) |
| `hud/composed/inspector_panel.gd` | GDScript | InspectorPanel controller: collapsible sections, param row population, port exposure |
| `hud/composed/inspector_panel.tscn` | Scene | InspectorPanel scene tree (elbow + rail + endcap + collapsible sections) |
| `hud/composed/status_strip.gd` | GDScript | StatusStrip controller: mode chip toggling, telemetry value updates, port exposure |
| `hud/composed/status_strip.tscn` | Scene | StatusStrip scene tree (rail + endcaps + chip bar + telemetry readouts) |
| `hud/composed/collapsible_section.gd` | GDScript | Composed container: header Chip toggle → SectionContent visibility, no new primitive |
| `hud/composed/collapsible_section.tscn` | Scene | Reusable collapsible section (header + content VBox) |
| `hud/composed/param_row.gd` | GDScript | Single parameter row: label + value/control, uniform height (3u), touch-sized |
| `hud/composed/param_row.tscn` | Scene | ParamRow scene (HBoxContainer with label + value slots) |

### New workbench scene (`hud/workbench/`)

| File | Type | Purpose |
|------|------|---------|
| `hud/workbench/workbench_hud.gd` | GDScript | Top-level orchestrator: manages glass layer, structure layer, connector layer; handles BackBufferCopy sync |
| `hud/workbench/workbench_hud.tscn` | Scene | WorkbenchHUD CanvasLayer scene (assembles all layers) |
| `hud/workbench/connector_manager.gd` | GDScript | Spawns/despawns SplineConnector instances from net definitions; delegates routing |
| `hud/workbench/glass_layer.gd` | GDScript | Manages GlassPane instances and their BackBufferCopy siblings; rect sync on resize |

### New connector logic (`hud/connectors/`)

| File | Type | Purpose |
|------|------|---------|
| `hud/connectors/hud_port.gd` | GDScript | Port data class: port_id, node_id, side, role, priority, capacity, style, cap |
| `hud/connectors/hud_net.gd` | GDScript | Net data class: net_id, kind, importance, from/to port refs, routing hints |
| `hud/connectors/hud_bus.gd` | GDScript | Bus data class: bus_id, orientation, lane_count, rect, role |
| `hud/connectors/connector_router.gd` | GDScript | Deterministic routing engine: port resolution, arc construction, bus-branch routing |

### Summary

| Directory | New .gd files | New .tscn files | Total |
|-----------|---------------|-----------------|-------|
| `hud/composed/` | 5 | 5 | 10 |
| `hud/workbench/` | 3 | 1 | 4 |
| `hud/connectors/` | 4 | 0 | 4 |
| **Total** | **12** | **6** | **18** |

No new shaders. No new primitives. Everything composes from the existing 7 primitives + token system.

---

## 2. Connector Topology

### 2.1 Ports

Each composed panel exposes ports as defined in the scene tree spec (Part 1). Port positions are computed at runtime via `get_port_position(side)` on the panel's bounding Control.

| Port ID | Module | Side | pos_u | Role | Priority | Style | Cap |
|---------|--------|------|-------|------|----------|-------|-----|
| `stack.active` | StackPanel | E | 0.20 | BLEND | 90 | ARC | ENDCAP |
| `stack.selection` | StackPanel | E | 0.50 | NAV | 70 | ARC | DOT |
| `stack.telemetry` | StackPanel | S | 0.50 | TELEMETRY | 40 | ARC | NONE |
| `inspector.focus` | InspectorPanel | W | 0.35 | BLEND | 90 | ARC | BRACKET |
| `inspector.shader` | InspectorPanel | W | 0.60 | EDIT | 60 | ARC | DOT |
| `inspector.telemetry` | InspectorPanel | S | 0.50 | TELEMETRY | 40 | ARC | NONE |
| `status.mode` | StatusStrip | N | 0.15 | NAV | 50 | ARC | NONE |
| `status.selection` | StatusStrip | N | 0.40 | NAV | 60 | ARC | BRACKET |
| `status.perf` | StatusStrip | N | 0.70 | TELEMETRY | 30 | ARC | NONE |

### 2.2 Nets

| Net ID | Kind | Importance | From | To | Routing | Visual Class |
|--------|------|-----------|------|-----|---------|-------------|
| `active_to_focus` | HIGHLIGHT | 90 | `stack.active` | `inspector.focus` | BUS_BRANCH via `center_bus` | **Primary** — thick, scanline |
| `selection_to_status` | DATAFLOW | 70 | `stack.selection` | `status.selection` | DIRECT_ARC | **Secondary** — medium |
| `stack_telem_to_status` | DATAFLOW | 40 | `stack.telemetry` | `status.perf` | DIRECT_ARC | **Tertiary** — thin schematic |
| `inspector_telem_to_status` | DATAFLOW | 40 | `inspector.telemetry` | `status.perf` | DIRECT_ARC | **Tertiary** — thin schematic |
| `mode_to_inspector` | HIGHLIGHT | 60 | `status.mode` | `inspector.shader` | DIRECT_ARC | **Secondary** — medium |

### 2.3 Buses

| Bus ID | Orientation | Lanes | Rect (approx, px) | Role | Thickness |
|--------|-------------|-------|-------------------|------|-----------|
| `center_bus` | VERT | 3 | [1560, 120, 72, 1800] | NAV | 2.5u |

The `center_bus` runs vertically through the center gap between StackPanel and InspectorPanel. The `active_to_focus` net (primary, importance=90) routes:
1. **ARC**: `stack.active` (E side, ~720px from left) → `center_bus` entry point (~1560px, y≈432)
2. **BUS**: vertical rail segment within `center_bus` lane 0 (entry y → exit y)
3. **ARC**: `center_bus` exit point (~1560px, y≈756) → `inspector.focus` (W side, ~2400px from left)

This keeps the highest-importance connection routed through a stable LCARS rail backbone. Lower-importance nets route as direct arcs, avoiding the bus.

### 2.4 Routing Details

**Port position resolution:**
- `pos_u` is a 0–1 fraction along the specified side
- Concrete position: `module_rect.position + side_direction * (pos_u * side_length)`
- Quantized to nearest `0.5u` (12px) for determinism

**Arc construction (direct):**
- `d = distance(P0, P3)`
- `t = clamp(d * 0.25, 3u, 16u)` = `clamp(d * 0.25, 72, 384)`
- P1 = P0 + outward_normal × t
- P2 = P3 + inward_normal × t
- Bake at `8px` intervals → 16–64 sample points depending on length

**Bus-branch routing:**
- Entry/exit Y positions on bus are the Y coordinates of the from/to port positions
- Bus lane assignment: lane 0 for highest priority, lane 1 for next, etc.
- Lane offset: `lane_index * 1u` from bus center

**Clearance:**
- Global: `max(1u, 1.25 × thickness_px)`
- The `center_bus` reserves a 3u-wide zone (72px) that lower-priority direct arcs route around

### 2.5 Importance Classes (rendering)

| Class | Importance | Line Width | Alpha | Scanline | Rim |
|-------|-----------|-----------|-------|----------|-----|
| Primary | ≥80 | 2.5u (60px) | 0.8 | Yes (speed=0.15) | 1px highlight |
| Secondary | 50–79 | 1.5u (36px) | 0.6 | No | 1px highlight |
| Tertiary | <50 | 0.75u (18px) | 0.4 | No | None |

---

## 3. Viewport Integration

### 3.1 CanvasLayer Strategy

A single `CanvasLayer` (layer=10) contains the entire HUD. The world viewport is a `SubViewportContainer` as the *first child* of this CanvasLayer, rendering at full resolution (3240×2160). This means:

1. The SubViewportContainer renders the 3D world to the screen
2. Glass panes (children of the same CanvasLayer) read the screen via `hint_screen_texture` + `SCREEN_UV`
3. Solid LCARS blocks paint over the glass
4. Connectors and text paint over everything

**Why single CanvasLayer, not multiple:**
- Glass shaders need `SCREEN_UV` to sample whatever is behind them. With one CanvasLayer, the SubViewport output is what they sample — correct behavior.
- Multiple CanvasLayers would require each glass pane to be in a *lower* CanvasLayer than the structure it overlays, creating complex ordering issues.
- z_index within a single CanvasLayer is sufficient for the 4 layers (glass=0, structure=1, connectors=2, text=3).

### 3.2 Z-Index Map

| Layer | z_index | Contains | mouse_filter |
|-------|---------|----------|-------------|
| GlassLayer | 0 | GlassPane instances + BackBufferCopy | PASS (content interactive) |
| StructureLayer | 1 | Rails, elbows, endcaps, chips | STOP on interactive elements |
| ConnectorLayer | 2 | SplineConnector instances, brackets | IGNORE (connectors handle own hit detection) |
| TextLayer | 3 | Labels, readouts | IGNORE |

### 3.3 BackBufferCopy Placement

Each `BackBufferCopy` node is placed *after* the glass pane it captures, *before* the next glass pane that needs to sample it. In the GlassLayer:

```
GlassLayer
├── StackGlass (GlassPane)       ← samples world (SubViewport)
├── BackBufferCopy               ← captures StackGlass result
├── InspectorGlass (GlassPane)   ← samples world + StackGlass result
├── BackBufferCopy               ← captures InspectorGlass result
└── StatusGlass (GlassPane)      ← samples world + previous panes
```

In practice, the side panels don't geometrically overlap each other (left vs right), so the BackBufferCopy between them is a safety measure for edge cases (ultrawide where panels might approach center). The StatusGlass *does* overlap with both side panels at the bottom corners — the second BackBufferCopy ensures it sees correct composited content.

**Rect sync:** `glass_layer.gd` connects to each GlassPane's `NOTIFICATION_RESIZED` and updates the corresponding BackBufferCopy's `rect` property to match the preceding GlassPane's `get_rect()`.

### 3.4 SubViewport Setup

```
WorldFeed (SubViewportContainer)
  stretch = true
  size = Vector2i(3240, 2160)
  stretch_shrink = 1

  WorldViewport (SubViewport)
    size = Vector2i(3240, 2160)
    render_target_update_mode = UPDATE_ALWAYS
    transparent_bg = false
    msaa_3d = MSAA_2X  # tunable
    └── [3D scene: Camera3D, Environment, meshes]
```

The SubViewportContainer fills the CanvasLayer. Glass panes reference the rendered output via screen texture — no explicit texture pass is needed.

### 3.5 Resolution Behavior

- Native: 3240×2160 (3:2). All u-values at 1:1.
- Stretch Aspect: `expand` — viewport stretches to fill window, HUD anchors maintain proportions.
- Anchors on panels use fractional positions (e.g., StackPanel: left=0.0, right=0.22) so they scale proportionally.
- `unit_px` remains 24 at all resolutions — rim/bevel stability guaranteed by logical-pixel sizing.
- At 1920×1080, panels compress but maintain touch targets (3u = 72px still ≥ 7mm at typical DPI).
