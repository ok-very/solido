# CONNECTORS.md — Spline Connector System (LCARS Glass HUD)

This document specifies the connector system used by the clean LCARS-glass HUD: ports, nets, routing, spline construction, and deterministic styling so the layout stays predictable as schemas evolve.

Solido Tri-D is a Godot 4 shader-development platform with a schema-driven editor plugin that parses TOML and builds Control-based UI, so connectors must be data-driven and reproducible.

---

## 0) Design goals

- **Deterministic**: same schema + same window size -> identical connector geometry.
- **Modular**: connectors are generated from module "ports," not hand-placed.
- **Readable**: connectors reinforce hierarchy and grouping; they never obscure core controls.
- **Engineered arcs**: splines feel like routed traces (bounded curvature, consistent thickness, clean caps).
- **LCARS-friendly**: integrates with rails/endcaps/elbows; arcs are "branches," not spaghetti.

Non-goals:
- Full CAD-like routing (no perfect Manhattan solver required).
- Automatic labeling of every edge (labels are optional and constrained).

---

## 1) Vocabulary

- **Node**: a UI module (panel, strip, inspector section, stack group, viewport overlay).
- **Port**: connector attachment point on a Node boundary, with direction + style constraints.
- **Net**: logical connection between ports (data flow, dependency, grouping, selection linkage).
- **Segment**: a piece of a connector path. A Net can be: direct, bus+branch, or multi-drop.
- **Bus**: shared backbone rail line (often orthogonal) used by many Nets, with branch arcs to endpoints.
- **Reserved zones**: rectangles where connectors should not pass (text fields, dense UI, critical thumbnails).

---

## 2) Data model

### 2.1 Node model (runtime)
Each module exposes:
- `node_id: String`
- `rect: Rect2` (in UI logical coordinates)
- `z_group: int` (layering group; connectors route behind text)
- `reserved_zones: Array[Rect2]` (optional; can be generated from children)
- `ports: Array[Port]`

### 2.2 Port model
```json
{
  "port_id": "inspector.color.out",
  "node_id": "inspector.color",
  "side": "E",
  "pos_mode": "AUTO",
  "pos_u": 0.35,
  "offset_px": [0, 0],

  "role": "EDIT",
  "priority": 80,
  "capacity": 2,

  "thickness_u": 2.0,
  "style": "ARC",
  "cap": "ENDCAP",
  "allow_crossings": false
}
```

Fields:
- `side`: N | E | S | W | NE | NW | SE | SW
- `pos_mode`: AUTO (heuristic spacing), PARAM (use `pos_u` 0..1 along side), ABS (use `offset_px` from reference corner)
- `role`: semantic color/style token (NAV, EDIT, BLEND, TELEMETRY, ALERT, NEUTRAL)
- `priority`: routing importance; higher = gets better lanes, less likely to be thinned
- `capacity`: max nets attached before the port creates a "fanout" glyph
- `thickness_u`: base thickness in grid units (u)
- `style`: ARC or ELBOW (ELBOW is allowed as fallback)
- `cap`: DOT | ENDCAP | BRACKET | NONE

### 2.3 Net model
```json
{
  "net_id": "active_effect_to_viewport",
  "kind": "HIGHLIGHT",
  "importance": 90,
  "from": ["stack.active_effect"],
  "to": ["viewport.overlay.focus"],

  "routing": {
    "mode": "BUS_BRANCH",
    "bus_id": "ops_bus_right",
    "avoid": ["viewport.safe_center", "text_fields"]
  },

  "style_overrides": {
    "pulse": 0.15,
    "alpha": 0.8
  }
}
```

Kind suggestions: DATAFLOW (subtle), HIGHLIGHT (selection), GROUP (bracketing), WARNING (alert trace).

### 2.4 Bus model
```json
{
  "bus_id": "ops_bus_right",
  "orientation": "VERT",
  "lane_count": 3,
  "rect": [1700, 120, 60, 840],
  "role": "NAV",
  "thickness_u": 2.5,
  "endcaps": true
}
```

---

## 3) Routing algorithm (deterministic)

### 3.1 High-level flow
1. Collect geometry: node rects, reserved zones, port candidates.
2. Resolve port positions (AUTO -> concrete points):
   - Partition each side into lanes (based on u, thickness, and requested spacing).
   - Sort ports by `(priority DESC, node_id ASC, port_id ASC)` (stable tie-breaker).
   - Assign lane slots in stable order.
3. Route nets:
   - Prefer BUS_BRANCH if a bus exists in the net's routing hints.
   - Else DIRECT_ARC for short distances; fallback ELBOW if arc would violate curvature or clearance.
4. Post-process:
   - Inflate paths by thickness and run collision clearance against reserved zones and other high-priority connectors.
   - If collision found, shift to adjacent lane, or re-route via bus.
   - Bake render primitives: sampled polyline + per-point width/uv + caps.

### 3.2 Clearance & lanes
- Global clearance: `clearance_px = max(1u, 1.25 * thickness_px)`.
- Every connector occupies a "tube" area; collisions are tube-vs-rect checks.
- Lanes are discrete offsets from a bus/side to prevent jitter across resolutions.

### 3.3 Deterministic obstacle avoidance
Use a fixed, bounded set of candidate detours (always pick the first that satisfies constraints):
1. `lane_shift`: move branch attach point up/down (or left/right) by N lanes
2. `bus_attach_alt`: attach to a different bus lane
3. `control_point_push`: push Bezier control points away from obstacles by fixed increments
4. `style_fallback`: ARC -> ELBOW for that net only (last resort)

See `ROUTER_PSEUDOCODE.md` for complete pseudocode.

---

## 4) Spline construction spec

### 4.1 Curve choice
Cubic Bezier segments:
- Start at P0 (from-port), end at P3 (to-port).
- Control points P1, P2 computed from direction + distance.

### 4.2 Control point rules (engineered arcs)
- `d = distance(P0, P3)`
- `t = clamp(d * 0.25, min_t=3u, max_t=16u)`
- `dir0` = outward normal from port side
- `dir3` = outward normal from port side (pointing backward into approach)
- `P1 = P0 + dir0 * t`
- `P2 = P3 + dir3 * t`

Curvature clamp: if curve would exceed max_curvature (approx by sampling), reduce t or use two-stage routing via a bus anchor.

### 4.3 Bus+branch routing
A BUS_BRANCH net becomes:
1. ARC: from-port -> bus entry point
2. BUS: along bus lane (orthogonal rail, rendered as LCARS rail segment)
3. ARC: bus exit point -> to-port

This keeps the "LCARS rails" aesthetic and makes routing stable.

### 4.4 Sampling for rendering
Sample curve at N points (N based on length; e.g., 16-64). Store per point:
- `pos: Vector2`
- `t: float` (0..1 along net)
- `width_px: float` (may vary slightly near caps)
- `lane_id: int` (debug)

Render as: thick polyline mesh (recommended), OR shader-based ribbon quad strip.

---

## 5) Style matrix (primary / secondary / tertiary)

### 5.1 Connector classes
- **Primary** (importance >= 80): thicker, higher alpha, subtle scanline.
- **Secondary** (50-79): medium thickness, lower alpha, no scanline.
- **Tertiary** (< 50): thin schematic trace, mostly for grouping.

### 5.2 Visual recipe (bevel + rim)
For each connector ribbon:
- Core fill: role-tinted, semi-opaque.
- Rim: 1 px highlight on one side (gives bevel impression).
- Soft shadow: minimal, only if it improves contrast over world blur.

Caps:
- DOT: small circular cap.
- ENDCAP: half-pill cap matching thickness.
- BRACKET: tiny bracket tick (schematic).

### 5.3 Interaction behavior
- Hover on a connected module: highlight all attached nets (alpha + rim boost).
- Active selection: primary net pulses slightly (0.1-0.2 amplitude); endpoints show "target brackets" (optional).
- Touch: first tap = select + highlight, second tap = activate (no hover precursor).

---

## 6) Reserved zones & readability

### 6.1 Reserved zone generation
Each Node may generate reserved zones automatically:
- Text-entry rows, dense parameter grids, thumbnails, mini-graphs.
- The viewport's "safe center" (composition focal region).

### 6.2 World-blur + connector contrast
Because panes blur the world at ~33% opacity, connectors must remain visible without becoming neon:
- Apply a mild "contrast guard": if sampled background luminance is too close to connector luminance, raise rim contrast (not saturation).

---

## 7) Implementation plan (Godot / Solido)

### 7.1 Suggested files
- `hud/connectors/port.gd`
- `hud/connectors/net.gd`
- `hud/connectors/router.gd`
- `hud/primitives/spline_connector.tscn`
- `hud/primitives/bus_rail.tscn`
- `hud/shaders/hud_connector.gdshader`
- `hud/shaders/hud_glass.gdshader`

### 7.2 Integration points
UI builder emits Nodes with `node_id` and port definitions. Router runs on:
- Window resize.
- Module collapse/expand.
- Selection change (style only; avoid full reroute unless geometry changes).

### 7.3 Debug overlay (required)
Add a toggleable overlay that draws:
- Node rects
- Port points + IDs
- Reserved zones
- Routed polylines with lane colors
- Collision markers and chosen detour strategy

This is mandatory to keep the system "predictable, not magic."

---

## 8) Determinism checklist

To guarantee stable output:
1. Sort everything with explicit tie-breakers (IDs).
2. Quantize key values to u-based steps (lane offsets, bus lane spacing).
3. Avoid float drift: round sampled points to 0.25 px (or similar) before mesh build.
4. Keep routing candidates ordered and bounded.

---

## 9) Minimal JSON example (end-to-end)

```json
{
  "unit_px": 24,
  "nodes": [
    {
      "node_id": "stack",
      "rect": [0, 0, 480, 2160],
      "ports": [
        {
          "port_id": "stack.active",
          "side": "E",
          "pos_mode": "PARAM",
          "pos_u": 0.2,
          "role": "BLEND",
          "priority": 90,
          "style": "ARC",
          "thickness_u": 2.5,
          "cap": "ENDCAP"
        }
      ],
      "reserved_zones": []
    },
    {
      "node_id": "inspector",
      "rect": [2400, 0, 840, 2160],
      "ports": [
        {
          "port_id": "inspector.focus",
          "side": "W",
          "pos_mode": "PARAM",
          "pos_u": 0.35,
          "role": "BLEND",
          "priority": 90,
          "style": "ARC",
          "thickness_u": 2.5,
          "cap": "BRACKET"
        }
      ],
      "reserved_zones": []
    }
  ],
  "buses": [
    {
      "bus_id": "right_bus",
      "orientation": "VERT",
      "lane_count": 3,
      "rect": [2340, 120, 60, 1920],
      "role": "NAV",
      "thickness_u": 2.0
    }
  ],
  "nets": [
    {
      "net_id": "active_to_focus",
      "kind": "HIGHLIGHT",
      "importance": 90,
      "from": ["stack.active"],
      "to": ["inspector.focus"],
      "routing": {
        "mode": "BUS_BRANCH",
        "bus_id": "right_bus"
      }
    }
  ]
}
```
