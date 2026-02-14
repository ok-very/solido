# CONNECTORS.md — Spline Connector System (LCARS Glass HUD)

This document specifies the connector system used by the clean LCARS-glass HUD: ports, nets, routing, spline construction, and deterministic styling so the layout stays predictable as schemas evolve.

---

## 0) Design goals
- Deterministic: same schema + same window size → identical connector geometry.
- Modular: connectors are generated from module “ports,” not hand-placed.
- Readable: connectors reinforce hierarchy and grouping; never obscure core controls.
- Engineered arcs: bounded curvature, consistent thickness, clean caps.
- LCARS-friendly: integrates with rails/endcaps/elbows; arcs are “branches,” not spaghetti.

---

## 1) Vocabulary
- Node: a UI module (panel, strip, inspector section, stack group, viewport overlay).
- Port: attachment point on a Node boundary.
- Net: logical connection between ports.
- Bus: shared backbone rail with branch arcs.
- Reserved zones: rectangles connectors may not cross.

---

## 2) Data model

### 2.1 Port
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

### 2.2 Net
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

### 2.3 Bus
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

## 3) Routing overview
1. Resolve port positions (AUTO → concrete points), lane-based.
2. Route nets using BUS_BRANCH when possible; else DIRECT_ARC.
3. Check collisions with reserved zones and module padding.
4. Apply deterministic detours (lane shifts, control point pushes, alternate bus lane).
5. Bake render primitives (polyline + caps).

---

## 4) Spline style
- Default connector: cubic Bézier/Curve2D.
- Clamp curvature: avoid “noodle” curves.
- Endcaps communicate hierarchy and role.

---

## 5) Reserved zones
Reserved zones are mandatory for:
- Text-entry rows, dense parameter grids, thumbnails.
- Viewport “safe center” region.

---

## 6) Debug overlay (required)
Must visualize:
- Node rects
- Ports + IDs
- Reserved zones
- Routed paths + lane ids
- Collision markers and chosen detour
