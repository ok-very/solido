# Connector Visual Redesign — From Bezier Splines to LCARS Traces

Blueprint for fixing the connector rendering layer. The data model (HudPort, HudNet, HudBus), ConnectorRouter, and ConnectorManager are reusable. This proposal addresses only the visual output: what gets drawn to screen, at what weight, in what style.

---

## 0. Problem Statement

Visual test at 1920x1080 viewport (unit_px=24) reveals:

1. **Primary connectors are 60px thick** (2.5u) — same visual weight as the StackPanel's 3u navigation rail (72px). Connectors should be subordinate infrastructure, not co-equal structural elements.
2. **Organic Bezier curves cross in the center gap** — produces a spaghetti tangle with no relationship to the LCARS design language of orthogonal bars and controlled elbows.
3. **The center bus is invisible** — rendered as a conceptual routing backbone only. In LCARS, a bus should BE a visible rail element.
4. **No elbow language** — all connections are smooth curves. LCARS canonical uses rectilinear elbows with specific inner/outer radii. Our LcarsElbow primitive exists but is not used by connectors.
5. **Connectors overwhelm panel content** — at 60px primary / 36px secondary, connectors compete with the UI elements they connect.

### Reference baseline (from lcars-connector-references.md)

Canonical LCARS does NOT use freeform splines. Connections are expressed through:
- Elbow-connected rails framing content areas
- Consistent-width horizontal/vertical bars
- Color-coded sections (hierarchy through color, not line weight)
- End caps (rounded terminations)
- Spatial adjacency

The ha-lcars project uses 35px vertical borders and 10px horizontal borders at screen scale. The cb-lcars project uses elbow-based framing as the primary visual connector — no freeform line elements at all.

---

## 1. Decision Log

### D1: Rectilinear elbows, not Bezier splines

**Options considered:**
| Option | Description | Verdict |
|--------|-------------|---------|
| A. Keep Bezier splines, reduce weight | Thinner curves, same organic shape | REJECTED — the shape language is wrong, not just the weight. Thin Bezier curves still look like node-graph wires, not LCARS infrastructure |
| B. Pure rectilinear (Manhattan routing) | All connectors are H/V segments with 90-degree elbows | **SELECTED** — matches LCARS language exactly, uses the same elbow radii as our LcarsElbow primitives |
| C. Hybrid: elbows for bus segments, arcs for direct | Bus connections rectilinear, direct connections curved | REJECTED — visual inconsistency. Two connector languages in one HUD is noisy |
| D. Remove explicit connectors, use color adjacency only | No drawn connections, rely on color and layout to imply relationships | REJECTED — we have cross-viewport connections (Stack E-side to Inspector W-side) that need explicit visual linkage |

**Rationale:** LCARS is fundamentally a rectilinear design language. Every reference project confirms this. Our elbows, rails, and endcaps are all orthogonal. Dropping Bezier splines in favor of Manhattan-routed traces with rounded elbows creates visual unity between connectors and the structural primitives that frame them.

### D2: Connector visual weight — subordinate to structure

**Options considered:**
| Option | Primary width | Secondary width | Tertiary width | Verdict |
|--------|-------------|----------------|----------------|---------|
| A. Current spec | 2.5u (60px) | 1.5u (36px) | 0.75u (18px) | REJECTED — primary matches rail weight |
| B. Rail-fraction | 0.5u (12px) | 0.25u (6px) | 2px | REJECTED — too thin at 3240x2160, invisible at distance |
| C. Schematic scale | 0.375u (9px) | 0.25u (6px) | 0.125u (3px) | **SELECTED** — visible trace, clearly subordinate to 2-3u rails |
| D. ha-lcars proportions | 10px fixed | 6px fixed | 2px fixed | REJECTED — pixel-fixed doesn't scale with our unit system |

**Rationale:** The structural rails are 2-3u (48-72px). Connectors should read as "traced routes," not "additional rails." A primary connector at 0.375u (9px) is ~1/8 the weight of the navigation rail — clearly infrastructure, not structure. This matches PCB trace / schematic diagram proportions where traces are thin lines subordinate to component outlines.

At 3240x2160: primary=9px, secondary=6px, tertiary=3px.
At 1920x1080: primary=9px, secondary=6px, tertiary=3px (unit_px stays 24 at all resolutions per the stretch=expand architecture).

### D3: Bus should be a visible LCARS rail element

**Options considered:**
| Option | Description | Verdict |
|--------|-------------|---------|
| A. Keep invisible (current) | Bus is routing-only, never drawn | REJECTED — wastes the center gap, bus-branch connections have invisible middle segment |
| B. Thin drawn rail with endcaps | Visible vertical bar at 1u width with half-pill endcaps top and bottom | **SELECTED** — makes the bus a proper LCARS structural element |
| C. Full-weight rail (2.5u) | Same weight as panel rails | REJECTED — too heavy for a routing backbone, competes with panel rails |
| D. Dashed/dotted rail | Visible but schematic | REJECTED — dashes are not in LCARS vocabulary |

**Rationale:** The bus should be a visible LCARS rail in the center gap. At 1u (24px) thickness, it reads as a secondary structural element — present but subordinate to the 2-3u panel rails. Half-pill endcaps at top and bottom give it proper LCARS termination. Connectors visibly enter and exit this rail, making the routing logic legible.

### D4: Connector aesthetic — thin rectilinear traces with role color

**Options considered:**
| Option | Description | Verdict |
|--------|-------------|---------|
| A. Glowing wires | Bloom/glow shader on thin lines | REJECTED — "neon graffiti" risk, conflicts with glass panel subtlety |
| B. Flat-color traces | Solid color, no shader effects | REJECTED — too flat, no depth integration with beveled primitives |
| C. Role-tinted traces with rim highlight | Thin trace with 1px brighter edge, role color at moderate alpha | **SELECTED** — matches existing primitive rim language, reads as traced route |
| D. Gradient traces (source color to dest color) | Blend from role to role along path | REJECTED — ambiguous semantics, visual noise |

**Rationale:** The existing connector shader already has the rim highlight concept. Keep it — just apply it to much thinner, rectilinear geometry. The rim highlight gives traces subtle depth that matches beveled elbows and rails. Role color provides semantic meaning. Moderate alpha (not near-opaque) ensures traces don't compete with solid structural elements.

### D5: Elbow radius for connector turns — use inner_radius_u from token system

**Options considered:**
| Option | Description | Verdict |
|--------|-------------|---------|
| A. Sharp 90-degree corners | No radius | REJECTED — too harsh, breaks LCARS curved-corner language |
| B. Fixed 1u radius | 24px corner radius on all connector turns | **SELECTED** — proportional to trace width, matches LCARS inner radius scale |
| C. Match panel elbow radii (2-4u) | 48-96px corners | REJECTED — oversized for thin traces, wastes routing space |

**Rationale:** A 1u (24px) corner radius on a 9px trace produces a clean rounded elbow that is proportional to the trace weight. This is analogous to PCB trace routing where corner radius scales with trace width. The LCARS-SDK formula (inner radius = 35px at their scale) aligns with roughly 1.5x the trace width — our 24px radius on a 9px trace is ~2.7x, which is generous but keeps the turns legible.

### D6: SplineConnector adaptation vs replacement

**Options considered:**
| Option | Description | Verdict |
|--------|-------------|---------|
| A. Adapt SplineConnector in-place | Change point generation, keep Line2D renderer | **SELECTED** — Line2D handles polylines with round joints natively; the shader already works; hit detection is point-based |
| B. New RectilinearConnector class | Fresh implementation, retire SplineConnector | REJECTED — SplineConnector's Line2D rendering, shader integration, hit detection, interaction model are all reusable. Changing what points get fed to it is sufficient |
| C. Replace with elbow primitive composition | Compose from LcarsElbow + LcarsRail nodes | REJECTED — overkill for thin traces; dynamic routing needs programmatic point generation, not scene tree composition |

**Rationale:** SplineConnector already renders a polyline (Line2D), applies a shader, handles hit detection, and manages interaction state. The problem is the POINTS it receives, not the renderer. Feed it rectilinear points (H/V segments with rounded corners via short arc segments at turns) and it draws the right thing. The Curve2D `set_curve_points()` path is retired; `set_points_from_bake()` with pre-computed rectilinear points becomes the primary API.

---

## 2. Visual Specification

### 2.1 Connector Classes (revised)

| Class | Importance | Width | Alpha | Rim | Scanline | Elbow radius |
|-------|-----------|-------|-------|-----|----------|-------------|
| Primary | >=80 | 0.375u (9px) | 0.55 | 1px, 40% rim_color | Slow (speed=0.08) | 1u (24px) |
| Secondary | 50-79 | 0.25u (6px) | 0.40 | 1px, 30% rim_color | None | 0.75u (18px) |
| Tertiary | <50 | 0.125u (3px) | 0.30 | None | None | 0.5u (12px) |

**Comparison to structural elements:**

| Element | Typical width | Connector ratio |
|---------|-------------|----------------|
| StackPanel nav rail | 3u (72px) | Primary = 1:8 |
| InspectorPanel rail | 2.5u (60px) | Primary = 1:6.7 |
| StatusStrip rail | 2u (48px) | Primary = 1:5.3 |
| GlassPane rim | 2-4px | Tertiary = 1:1 (same visual layer) |
| Bracket stroke | 2px | Tertiary is slightly heavier |

### 2.2 Bus Rail Visual

| Property | Value | Notes |
|----------|-------|-------|
| Width | 1u (24px) | Visible structural rail |
| Role color | NAV (amber) | Matches the routing role |
| Alpha | 0.7 | Slightly translucent, reads as infrastructure |
| Corner radius | 0.5u (12px) | Endcap style, not sharp |
| Endcaps | Half-pill, top and bottom | Standard LCARS termination |
| Rim | 1px highlight | Matches rail rim language |

The bus is rendered as an actual LcarsRail instance (single segment, vertical, 1u thickness) with two LcarsEndcap half-pills. It is NOT rendered through SplineConnector — it is a composed structural element placed in the ConnectorLayer.

### 2.3 Color System (unchanged semantic, adjusted alpha)

Connector colors come from `HudPalette.get_solid(role)` with alpha applied by the shader. No new colors. The role color system already provides semantic differentiation:
- BLEND connections (purple) between Stack and Inspector
- NAV connections (amber) for selection linkage
- EDIT connections (cyan) for shader/mode linkage
- TELEMETRY connections (green) for perf/diagnostics

### 2.4 Rectilinear Routing Geometry

Each connector path is a sequence of horizontal and vertical segments connected by rounded elbows.

**Direct connection (e.g., stack.selection E-side to status.selection N-side):**
```
Port A (E-side) ──── H segment ──── elbow(radius) ──── V segment ──── Port B (N-side)
```

The routing algorithm produces an L-shaped or Z-shaped path:
- **L-route:** One horizontal + one vertical segment (1 elbow). Used when ports are on perpendicular sides.
- **Z-route:** Horizontal + vertical + horizontal (2 elbows). Used when ports are on opposite sides with vertical offset.
- **U-route:** Vertical + horizontal + vertical (2 elbows). Used when ports are on the same side.

**Bus-branch connection (e.g., active_to_focus via center_bus):**
```
Port A (E-side) ── H segment ── elbow ── V segment ── [BUS ENTRY]
                                                         |
                                                    BUS RAIL (visible)
                                                         |
                                                      [BUS EXIT] ── V segment ── elbow ── H segment ── Port B (W-side)
```

The branch arcs become rectilinear branches: a horizontal extension from the port, an elbow, a vertical drop to the bus entry point. The bus itself is the visible rail. On the other side, vertical rise from bus exit, elbow, horizontal extension to destination port.

### 2.5 Elbow Point Generation

Each elbow is generated as a short arc of baked points (8-12 points for a quarter-circle at the specified radius). This feeds into the existing Line2D renderer seamlessly — the points are pre-computed, not relying on Curve2D Bezier math.

```
Quarter-arc point generation:
  center = elbow corner point
  start_angle = angle of incoming segment direction
  end_angle = start_angle + PI/2 (or -PI/2 depending on turn direction)
  for i in range(arc_point_count):
    t = float(i) / float(arc_point_count - 1)
    angle = lerp(start_angle, end_angle, t)
    point = center + Vector2(cos(angle), sin(angle)) * radius
```

---

## 3. Scene Tree Changes

### 3.1 ConnectorLayer (revised)

```
ConnectorLayer (Control, z_index=2)
  ├── BusRails (Control)                   # NEW: container for visible bus rail elements
  │   └── CenterBusRail (LcarsRail)        # 1u vertical rail, NAV role
  │       ├── TopEndcap (LcarsEndcap)       # half-pill termination
  │       └── BottomEndcap (LcarsEndcap)    # half-pill termination
  ├── ConnectorManager (Node)              # UNCHANGED: lifecycle manager
  │   ├── [SplineConnector instances]      # children, managed dynamically
  │   └── ...
  └── DebugOverlay (Control)               # optional, toggleable
```

### 3.2 Key difference

Previously, the ConnectorLayer contained only ConnectorManager with dynamically spawned SplineConnectors. Now it also contains a `BusRails` container with actual LcarsRail + LcarsEndcap compositions for each bus definition. The bus is a real structural element, not a rendered segment of a SplineConnector.

---

## 4. File Manifest (implementation changes)

| File | Change Type | Description |
|------|------------|-------------|
| `hud/connectors/connector_router.gd` | MODIFY | Replace `_build_arc()` with `_build_rectilinear()`. New methods: `_build_l_route()`, `_build_z_route()`, `_build_elbow_points()`. Bus-branch routing produces rectilinear branch segments instead of Bezier arcs. Remove Curve2D usage entirely. |
| `hud/primitives/spline_connector.gd` | MODIFY | Update `_apply_importance()` with new width/alpha/scanline values. Remove `set_curve_points()` (or deprecate). `set_points_from_bake()` becomes the primary API. Adjust hit detection threshold to match thinner traces. Rename class to `TraceConnector` (optional, breaks tests — decide during implementation). |
| `hud/connector_manager.gd` | MODIFY | Add bus rail lifecycle: on `configure()`, instantiate LcarsRail + LcarsEndcap for each bus. Position and resize bus rails on viewport resize. Bus segments from router no longer spawn SplineConnectors — the bus rail IS the visual. |
| `hud/shaders/hud_connector.gdshader` | MODIFY | Reduce rim band width (currently 0.12/0.15 UV fraction — too wide for thin traces). Adjust to 0.25/0.3 UV fraction (wider relative band on a narrower trace keeps the 1px rim visible). Reduce scanline speed default. |
| `hud/workbench_hud.gd` | MODIFY | Update `_build_bus_definitions()` to use new bus width (1u instead of 2.5u for bus_w). No structural changes. |
| `test/unit/test_connector_router.gd` | MODIFY | Update expected point patterns: rectilinear instead of Bezier curves. Add tests for L-route, Z-route, elbow point generation. |
| `test/unit/test_spline_connector.gd` | MODIFY | Update width/alpha expectations for new importance classes. |

**No new files required.** The bus rail is composed from existing LcarsRail + LcarsEndcap primitives, instantiated by ConnectorManager.

---

## 5. Router Algorithm Changes

### 5.1 Replace `_build_arc()` with `_build_rectilinear()`

Current `_build_arc()` computes cubic Bezier control points and bakes via Curve2D.

New `_build_rectilinear()` computes a sequence of H/V segments with elbow points:

```
_build_rectilinear(p0, side0, p3, side3, unit_px, elbow_radius) -> PackedVector2Array:
    dir0 = dir_for_side(side0)  # outward normal from source
    dir3 = dir_for_side(side3)  # outward normal from dest

    # Determine route shape based on port sides
    if sides_are_perpendicular(side0, side3):
        return _build_l_route(p0, dir0, p3, dir3, elbow_radius)
    elif sides_are_opposite(side0, side3):
        return _build_z_route(p0, dir0, p3, dir3, unit_px, elbow_radius)
    else:  # same side
        return _build_u_route(p0, dir0, p3, dir3, unit_px, elbow_radius)
```

### 5.2 L-Route (perpendicular sides)

Source E-side, Dest N-side example:
```
p0 ────────── corner ── p3
              |          ^
              corner is the intersection of the horizontal from p0 and vertical to p3
```

Points: `[p0, ..., elbow_arc_points, ..., p3]`

### 5.3 Z-Route (opposite sides, e.g., E-to-W)

Source E-side, Dest W-side:
```
p0 ────── midpoint_x ── elbow
                           |
                         elbow ── midpoint_x ── p3
```

The midpoint_x is the horizontal center between source and destination (or the center of the gap). Two elbows connect the three segments.

### 5.4 Bus-Branch Route (revised)

```
_route_bus_branch(from_port, from_pos, to_port, to_pos, bus, unit_px):
    lane = bus.pick_lane(...)
    entry = bus.project_to_lane(lane, from_pos, unit_px)
    exit  = bus.project_to_lane(lane, to_pos, unit_px)

    # Branch 1: from_port to bus entry (rectilinear)
    branch1 = _build_rectilinear(from_pos, from_port.side, entry, bus_side, unit_px, elbow_radius)

    # Bus segment: NO LONGER a SplineConnector segment
    # The bus rail is a separate visible LcarsRail element
    # Return only a marker so ConnectorManager knows to skip this segment

    # Branch 2: bus exit to to_port (rectilinear)
    branch2 = _build_rectilinear(exit, bus_side, to_pos, to_port.side, unit_px, elbow_radius)

    return [
        { "points": branch1, "type": "trace" },
        { "points": PackedVector2Array([entry, exit]), "type": "bus_segment" },  # metadata only
        { "points": branch2, "type": "trace" },
    ]
```

The ConnectorManager renders "trace" segments as SplineConnector instances. "bus_segment" entries are ignored for rendering (the bus rail handles that) but retained for debug overlay visualization.

---

## 6. Before/After: The 5 Demo Nets

### Net 1: `active_to_focus` (Primary, importance=90, BUS_BRANCH)
**Stack.active (E-side, y=20%) -> center_bus -> Inspector.focus (W-side, y=35%)**

**BEFORE:** 60px thick Bezier curve from stack right edge, swooping through center gap to inspector left edge. Purple ribbon dominates viewport center. Bus invisible — curve passes through where bus should be with no visual anchoring.

**AFTER:** 9px BLEND-purple trace exits Stack E-side horizontally, turns down via rounded elbow (24px radius), drops vertically to meet the visible 24px amber NAV bus rail. The trace visually terminates where it meets the bus. On the exit side, a 9px trace leaves the bus, turns via elbow, runs horizontally into Inspector W-side. The center bus is a visible vertical amber rail with half-pill endcaps top and bottom. The traces enter/exit the bus cleanly. Slow scanline animation pulses along the trace segments. Total visual weight: two thin role-colored traces + one medium structural rail.

### Net 2: `selection_to_status` (Secondary, importance=70, DIRECT_ARC)
**Stack.selection (E-side, y=50%) -> Status.selection (N-side, x=40%)**

**BEFORE:** 36px thick Bezier curve swooping from stack middle-right down and across to status strip. Crosses Net 1 in the center gap.

**AFTER:** 6px NAV-amber trace exits Stack E-side horizontally, extends into the gap, turns downward via rounded elbow (18px radius), drops vertically to Status N-side. L-shaped route. No crossing with Net 1 because the trace routes to the right of the bus rail and drops straight down. Clean perpendicular entry into status strip.

### Net 3: `stack_telem_to_status` (Tertiary, importance=40, DIRECT_ARC)
**Stack.telemetry (S-side, x=50%) -> Status.perf (N-side, x=70%)**

**BEFORE:** 18px Bezier curve from stack bottom to status top. Visible but organic shape looks out of place next to the orthogonal panel frames.

**AFTER:** 3px TELEMETRY-green trace exits Stack S-side downward, extends vertically, turns right via tiny elbow (12px radius), runs horizontally to align with Status.perf x-position, turns down via elbow, enters Status N-side. Z-route or L-route depending on relative positions. At 3px width and 0.30 alpha, this reads as a faint schematic trace — background infrastructure, not foreground information. No rim highlight.

### Net 4: `inspector_telem_to_status` (Tertiary, importance=40, DIRECT_ARC)
**Inspector.telemetry (S-side, x=50%) -> Status.perf (N-side, x=70%)**

**BEFORE:** 18px Bezier curve from inspector bottom to status top. Converges with Net 3 near status.perf port — two organic curves merging looks cluttered.

**AFTER:** 3px TELEMETRY-green trace exits Inspector S-side downward, turns left via elbow, runs horizontally toward status.perf, turns down via elbow, enters Status N-side. Parallel to Net 3 but from the opposite direction. The two tertiary traces approach status.perf from different sides — visually reads as two diagnostic feeds converging on a telemetry readout. Clean, schematic, unobtrusive.

### Net 5: `mode_to_inspector` (Secondary, importance=60, DIRECT_ARC)
**Status.mode (N-side, x=15%) -> Inspector.shader (W-side, y=60%)**

**BEFORE:** 36px Bezier curve swooping up and to the right from status strip to inspector middle. Crosses the center gap diagonally, intersecting Nets 1 and 2.

**AFTER:** 6px NAV-amber trace exits Status N-side upward, extends vertically into the gap, turns right via rounded elbow (18px radius), runs horizontally to Inspector W-side. L-route. The vertical segment runs in the gap between bus rail and inspector panel — no crossings with other connectors because rectilinear routing uses distinct horizontal/vertical lanes. At 6px width and 0.40 alpha, clearly secondary to the primary active_to_focus traces but more visible than the tertiary telemetry traces.

### Summary of visual change

| Aspect | Before | After |
|--------|--------|-------|
| Primary width | 60px | 9px (85% reduction) |
| Secondary width | 36px | 6px (83% reduction) |
| Tertiary width | 18px | 3px (83% reduction) |
| Path shape | Organic Bezier curves | Rectilinear H/V segments with rounded elbows |
| Center bus | Invisible routing concept | Visible 24px amber rail with endcaps |
| Crossings | Curves cross in center gap | Rectilinear routing uses lanes, minimal crossings |
| Visual hierarchy | Connectors compete with rails | Connectors clearly subordinate to structural elements |
| LCARS coherence | Foreign element (splines in orthogonal UI) | Native element (traces match elbow/rail language) |

---

## 7. Shader Changes

### 7.1 `hud_connector.gdshader` adjustments

The shader fundamentally works — it paints a core stroke with rim highlight and optional scanline. Changes are parameter-level:

**Rim band calculation (current):**
```glsl
float edge_rim = 1.0 - smoothstep(0.0, 0.12, UV.y);
edge_rim += 1.0 - smoothstep(0.88, 1.0, 1.0 - UV.y);
```

At 60px width, the 12% rim band = 7.2px. At 9px width, 12% = 1.08px — that is correct (approximately 1px rim). The UV-relative calculation naturally scales. **No change needed for rim calculation.**

**Scanline speed:** Reduce default from 0.15 to 0.08 for primary. Thinner traces with fast scanlines look jittery. Slow scanline on a thin trace reads as subtle data flow indicator.

**Alpha values:** Reduced across all classes (see spec in Section 2.1). Set via `u_alpha` uniform from `_apply_importance()`.

### 7.2 No new shader required

The existing `hud_connector.gdshader` handles the new visual spec. The Line2D UV mapping works identically for rectilinear polylines as for curved ones — UV.x is parametric along the polyline, UV.y is across width.

---

## 8. Interaction Changes

### 8.1 Hit detection threshold

Current hit threshold: `1.5 * unit_px` = 36px. This is a generous touch zone around the connector.

With thinner traces, the hit zone must remain touch-friendly. The 36px threshold is fine — it means the touch zone extends ~18px on each side of even a 3px tertiary trace. This is a feature, not a bug: thin visual trace, generous touch target.

**No change to hit detection algorithm.** The `_is_near_polyline()` threshold stays at 1.5u.

### 8.2 Hover/active state

On hover, the shader brightens alpha by 0.2 and adds 0.05 to color. On a thinner trace, this is proportionally more visible. May need tuning during implementation, but the mechanism is correct.

---

## 9. ConnectorManager Bus Rail Lifecycle

New responsibility: ConnectorManager creates and manages bus rail visual elements.

```
configure(ports, nets, buses):
    # Existing: store ports, nets, buses, mark dirty
    # NEW: for each bus, create a LcarsRail + 2 LcarsEndcap children

    for bus_id in buses:
        bus = buses[bus_id]
        if not _bus_rails.has(bus_id):
            var rail = LcarsRail.new()
            rail.orientation = 1 if bus.orientation == "VERT" else 0
            rail.thickness_u = 1.0  # bus visual thickness
            rail.role = bus.role
            rail.segment_count = 1
            # Position from bus.rect
            add_child(rail)  # or add to a BusRails container

            var top_cap = LcarsEndcap(...)  # half-pill, direction UP
            var bottom_cap = LcarsEndcap(...)  # half-pill, direction DOWN
            rail.add_child(top_cap)
            rail.add_child(bottom_cap)
            _bus_rails[bus_id] = rail
```

Bus rail position updates on resize (when `update_buses()` is called).

---

## 10. Migration Path

### Phase A: Router changes (no visual regression risk)
1. Add `_build_rectilinear()`, `_build_l_route()`, `_build_z_route()`, `_build_u_route()`, `_build_elbow_points()` to ConnectorRouter.
2. Add `_elbow_radius_for_importance(importance, unit_px)` helper.
3. Update `_route_direct_arc()` to call `_build_rectilinear()` instead of `_build_arc()`.
4. Update `_route_bus_branch()` to produce rectilinear branch segments and `bus_segment` metadata.
5. Update tests.

### Phase B: Visual weight changes
1. Update `_apply_importance()` in SplineConnector with new width/alpha/scanline values.
2. Update ConnectorManager to skip rendering `bus_segment` type entries.
3. Update tests.

### Phase C: Bus rail rendering
1. Add bus rail lifecycle to ConnectorManager.
2. Update WorkbenchHUD `_build_bus_definitions()` bus_w to 1u.
3. Update tests.

### Phase D: Polish
1. Tune elbow radii at multiple resolutions (3240x2160, 1920x1080, 2560x1440).
2. Adjust scanline speed.
3. Verify hit detection at all sizes.
4. Debug overlay: update to show rectilinear paths + bus rail bounds.

---

## 11. What to Keep vs What to Change

### KEEP (no changes)
- `hud/connectors/hud_port.gd` — data class, unchanged
- `hud/connectors/hud_net.gd` — data class, unchanged
- `hud/connectors/hud_bus.gd` — data class, unchanged (lane projection still useful for determining entry/exit Y coordinates on the bus)
- `hud/primitives/spline_connector.tscn` — scene structure (Line2D + RimLine + HitArea + caps), unchanged
- SplineConnector interaction model (tap, long-press, hover signals)
- SplineConnector shader registration with HudTheme
- ConnectorManager dirty-flag + deferred routing pattern
- ConnectorManager SplineConnector lifecycle (create/reuse/orphan)
- All port definitions in WorkbenchHUD
- All net definitions in WorkbenchHUD
- Debug overlay concept

### CHANGE (modifications to existing files)
- `connector_router.gd` — replace Bezier arc generation with rectilinear path generation
- `spline_connector.gd` — update importance class visual parameters (width, alpha, scanline)
- `connector_manager.gd` — add bus rail lifecycle, skip rendering bus_segment entries
- `hud_connector.gdshader` — minor parameter tuning (optional, may not need changes)
- `workbench_hud.gd` — update bus_w from 3u to 1u in `_build_bus_definitions()`
- Router and connector test files — update expected values

### RETIRE (no longer used)
- `SplineConnector.set_curve_points(p0, p1, p2, p3)` — Bezier-specific API. Keep the method for backward compat but mark as unused. `set_points_from_bake()` is the sole entry point.
- `ConnectorRouter._build_arc()` — replaced by `_build_rectilinear()`
- Curve2D usage in ConnectorRouter — no longer needed

---

## 12. Open Questions (for implementation)

1. **Connector-to-bus junction glyph:** When a trace meets the bus rail, should there be a small visual indicator (dot, notch, color blend) at the junction point? Or does the trace just terminate at the bus edge? Recommend: small 3px dot cap (CAP_DOT style) at bus entry/exit points.

2. **Lane separation for parallel rectilinear traces:** When multiple traces run parallel in the same gap region, what minimum separation? Recommend: 1u (24px) center-to-center, consistent with the existing clearance spec.

3. **Z-route midpoint selection:** For E-to-W Z-routes across the center gap, should the midpoint X always be the center of the gap, or offset to avoid the bus rail? Recommend: offset to one side of the bus rail (bus_x + bus_width/2 + 1u for routes going right of bus, bus_x - bus_width/2 - 1u for routes going left).

4. **Class rename:** Should `SplineConnector` be renamed to `TraceConnector` since it no longer renders splines? Breaks 427 tests. Recommend: defer rename to a dedicated cleanup pass.
