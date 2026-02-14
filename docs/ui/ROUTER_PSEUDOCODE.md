# ROUTER_PSEUDOCODE.md — Deterministic Connector Routing

This file provides concrete pseudocode for the LCARS-glass spline connector router: stable sorting, lane assignment, routing candidates, collision checks, and rendering outputs.

---

## 1) Stable sorting (mandatory)
Ports sorted by `(priority DESC, node_id ASC, port_id ASC)`.
Nets sorted by `(importance DESC, net_id ASC)`.

```pseudo
sort_ports(ports):
  return ports.sorted_by(-priority, node_id, port_id)

sort_nets(nets):
  return nets.sorted_by(-importance, net_id)
```

---

## 2) Port placement (lanes)
Place ports into discrete slot positions per node side.

```pseudo
build_side_slots(node_rect, side, slot_step, corner_guard):
  segment = side_segment(node_rect, side)
  usable = segment.shrink_ends(corner_guard)
  slots = []
  for t in range(0, usable.length, slot_step):
    slots.append(point_along_segment(usable, t))
  return slots

place_ports(node):
  for side in [N,E,S,W]:
    slots = build_side_slots(...)
    side_ports = sort_ports(ports_on_side)
    for port in side_ports:
      if port.pos_mode != AUTO:
        port.pos = resolve_non_auto(port)
      else:
        idx = pick_best_free_slot(slots, occupied)
        port.pos = slots[idx]
```

---

## 3) Routing modes
- BUS_BRANCH: arc → bus → arc
- DIRECT_ARC: arc only
- ELBOW fallback: 2-stage orthogonal route

```pseudo
route_net(net):
  if net.routing.mode == BUS_BRANCH and bus_exists(net.bus_id):
    return route_bus_branch(net)
  path = route_direct_arc(net)
  if violates_constraints(path):
    return route_elbow_fallback(net)
  return path
```

---

## 4) Arc generation (cubic Bézier)

```pseudo
dir_for_side(side):
  N:(0,-1) E:(1,0) S:(0,1) W:(-1,0)

control_dist(d):
  return clamp(d*0.25, 3u, 16u)

make_bezier(P0, side0, P3, side3):
  d = distance(P0,P3)
  t = control_dist(d)
  P1 = P0 + dir_for_side(side0)*t
  P2 = P3 - dir_for_side(side3)*t
  return Bezier(P0,P1,P2,P3)
```

Bake to polyline at a fixed density based on length.

---

## 5) Collision checking
Tube-vs-rect check against:
- reserved zones
- inflated node rects (padding)

```pseudo
path_collides(points, tube_radius, obstacles):
  for (A,B) in segments(points):
    for obs in obstacles_near_segment(A,B,tube_radius):
      if distance_segment_to_rect(A,B,obs) < tube_radius:
        return true
  return false
```

---

## 6) Detour candidates (ordered)
Try in this order (deterministic):
1. Source lane shift ±1, ±2
2. Destination lane shift ±1, ±2
3. Control point push +1u, +2u away from obstacle normal
4. Alternate bus lane ±1
5. Two-stage arc via mid anchor
6. ELBOW fallback

```pseudo
route_with_detours(net):
  base = build_base_route(net)
  if ok(base): return base
  for cand in detour_candidates(net):
    if ok(cand): return cand
  return route_elbow_fallback(net)
```

---

## 7) Rendering outputs
Two options:
- Prototype: Line2D from baked points.
- Recommended: ribbon mesh + shader (core fill + rim highlight + subtle shadow).

---

## 8) Debug overlay (required)
Draw:
- node rects
- reserved zones
- port points + ids
- net polylines + lane colors
- collision points + detour chosen
