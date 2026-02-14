```md
# ROUTER_PSEUDOCODE.md — Deterministic Connector Routing (Godot 4)

This file provides concrete pseudocode and implementation notes for the LCARS-glass spline connector router: stable ordering, lane assignment, routing candidates, collision checks, and rendering outputs.

Notes:
- For world-blur panes that sample the screen texture, be mindful of `BackBufferCopy` ordering and region capture rules; if you copy only a region and sample outside it, results are undefined. [web:54]
- For spline rendering, you can use Curve2D baking for stable point sampling and then render with a ribbon mesh or Line2D; Line2D supports a `width_curve` for width variation along the polyline. [web:199][web:193]

---

## 1) Inputs / outputs

### Inputs
- `nodes: Array[NodeSpec]`
- `nets: Array[NetSpec]`
- `buses: Array[BusSpec]`
- `viewport_rect: Rect2` (UI coordinate space, base 3240×2160 scaled by stretch)
- `unit_px: float` (e.g., 24 at native 3240×2160)
- `style_tokens: Dictionary` (role colors, thickness multipliers, alpha rules)
- `router_config: RouterConfig` (clearances, lane spacing, curvature limits)

### Outputs
- `render_nets: Array[RenderNet]` where each `RenderNet` contains:
  - `points: PackedVector2Array` (baked polyline in UI coords)
  - `width_px: float` (base width)
  - `width_profile: Curve` (optional)
  - `color: Color`
  - `caps: CapSpec` (start/end)
  - `debug: DebugInfo` (lane ids, detours used, collisions)

---

## 2) Stable sorting (mandatory)

All ordering must be stable and explicit to avoid jitter across runs:
- Ports: sort by `(priority DESC, node_id ASC, port_id ASC)`
- Nets: sort by `(importance DESC, net_id ASC)`
- Nodes: sort by `(z_group ASC, node_id ASC)` (geometry first, style later)

Pseudocode:
```pseudo
sort_ports(ports):
  return ports.sorted_by(
    -port.priority,
    port.node_id,
    port.port_id
  )

sort_nets(nets):
  return nets.sorted_by(
    -net.importance,
    net.net_id
  )
```

---

## 3) Port placement

### 3.1 Candidate slots per side
For each node side (N/E/S/W), allocate discrete “slot positions”:
- Slot spacing: `slot_step = max(unit_px, thickness_px * 1.25)`
- Slot range: avoid corners by `corner_guard = 2 * unit_px`

Pseudocode:
```pseudo
build_side_slots(node_rect, side, slot_step, corner_guard):
  segment = side_segment(node_rect, side)
  usable = segment.shrink_ends(corner_guard)
  slots = []
  t = 0
  while t <= usable.length:
    slots.append(point_along_segment(usable, t))
    t += slot_step
  return slots
```

### 3.2 Assign AUTO ports to slots
Assign highest-priority ports first:
```pseudo
place_ports(nodes):
  for each node in sort_nodes(nodes):
    for each side in [N,E,S,W]:
      slots = build_side_slots(node.rect, side, slot_step, corner_guard)
      side_ports = sort_ports(node.ports where port.side == side)

      occupied = boolean[slots.size] = false
      for port in side_ports:
        if port.pos_mode == PARAM:
          port.pos = lerp_side(node.rect, side, port.pos_u) + port.offset_px
          continue
        if port.pos_mode == ABS:
          port.pos = abs_anchor(node.rect, side, port.offset_px)
          continue

        # AUTO
        idx = pick_best_free_slot(slots, occupied, port, node)
        occupied[idx] = true
        port.pos = slots[idx] + port.offset_px
```

`pick_best_free_slot()` heuristic (stable):
- Prefer median slot (keeps symmetry)
- Then nearest to requested `pos_u` if provided
- Then lowest index (tie-break)

---

## 4) Reserved zones & obstacles

### 4.1 Build obstacle list
Obstacles include:
- Reserved zones (explicit)
- A padded rect for each node (so connectors don’t graze module edges)
- Optional viewport “safe center” rect

Pseudocode:
```pseudo
build_obstacles(nodes, global_reserved):
  obstacles = []
  for node in nodes:
    obstacles += inflate_all(node.reserved_zones, router_config.reserve_pad_px)
    obstacles += inflate(node.rect, router_config.node_pad_px)
  obstacles += inflate_all(global_reserved, router_config.reserve_pad_px)
  return merge_overlaps(obstacles)
```

---

## 5) Routing modes

### Modes
- `BUS_BRANCH`: from → bus_entry (arc), bus travel (rail), bus_exit → to (arc)
- `DIRECT_ARC`: single arc from port to port
- `ELBOW_FALLBACK`: orthogonal-ish 2-stage route with a corner point

Routing decision:
```pseudo
route_net(net):
  if net.routing.mode == BUS_BRANCH and bus_exists(net.routing.bus_id):
    return route_bus_branch(net)
  else:
    candidate = route_direct_arc(net)
    if violates_constraints(candidate):
      return route_elbow_fallback(net)
    return candidate
```

---

## 6) Arc generation (Bezier)

### 6.1 Directions by port side
```pseudo
dir_for_side(side):
  N -> (0,-1)
  E -> (1,0)
  S -> (0,1)
  W -> (-1,0)
```

### 6.2 Control distance clamp
```pseudo
control_dist(d):
  return clamp(d * 0.25, min_t = 3u, max_t = 16u)
```

### 6.3 Build Bezier
```pseudo
make_bezier(P0, side0, P3, side3):
  d = distance(P0, P3)
  t = control_dist(d)

  dir0 = dir_for_side(side0)
  dir3 = dir_for_side(side3)

  P1 = P0 + dir0 * t
  P2 = P3 - dir3 * t          # approach into P3

  curve = Bezier(P0,P1,P2,P3)
  return curve
```

### 6.4 Bake points (stable sampling)
Use fixed bake resolution per net:
```pseudo
bake_curve(curve, bake_px):
  # bake_px is “distance between baked points”
  # Smaller = smoother, more points.
  return curve.sample_polyline(bake_px)
```

Using Curve2D baking in Godot yields baked points and baked length, useful for stable sampling and proportional effects. [web:199]

---

## 7) Collision checking

### 7.1 Tube-vs-rect test
Inflate each segment by radius = `(width_px/2 + clearance_px)`:
```pseudo
path_collides(points, tube_radius, obstacles):
  for each segment (A,B) in polyline(points):
    seg_rect = bounding_rect(A,B).inflate(tube_radius)
    for obs in obstacles intersect seg_rect:
      if distance_segment_to_rect(A,B,obs) < tube_radius:
        return true
  return false
```

### 7.2 Constraint checks
A route is invalid if any are true:
- Collides with obstacles (reserved zones, module padding)
- Exceeds max curvature (approx by angle change per length)
- Crosses another connector with higher priority when crossings disabled

---

## 8) Detour candidates (ordered list)

For each net, generate candidates in this exact order:

1. **Lane shift on source side**: adjust port “slot index” ±1, ±2
2. **Lane shift on destination side**: adjust similarly
3. **Control point push**: push P1/P2 away from obstacle normal by `+1u`, `+2u`
4. **Bus attach alternate lane** (BUS_BRANCH only): lane ±1
5. **Two-stage arc**: source → mid_anchor, mid_anchor → dest
6. **ELBOW fallback**: orthogonal corner route (last resort)

Pseudocode:
```pseudo
route_with_detours(net):
  base = build_base_route(net)
  if ok(base): return base

  for candidate in generate_detour_candidates(net):
    if ok(candidate): return candidate

  return build_elbow_fallback(net)  # guaranteed output
```

---

## 9) BUS_BRANCH routing

### 9.1 Pick bus lane (deterministic)
Choose a lane index by hashing stable ids (but deterministic):
```pseudo
pick_lane(net_id, lane_count):
  h = stable_hash(net_id)
  return h % lane_count
```

### 9.2 Entry/exit anchors
- Entry point = closest point on bus lane to source port (project onto bus line)
- Exit point = closest point on bus lane to dest port

### 9.3 Route pieces
```pseudo
route_bus_branch(net):
  lane = pick_lane(net.net_id, bus.lane_count)
  entry = project_to_bus_lane(bus, lane, from_port.pos)
  exit  = project_to_bus_lane(bus, lane, to_port.pos)

  arc1 = make_bezier(from_port.pos, from_port.side, entry, bus_side_facing(from_port))
  bus_seg = straight_polyline(entry, exit) # rendered as LCARS rail segment
  arc2 = make_bezier(exit, bus_side_facing(to_port), to_port.pos, to_port.side)

  points = bake(arc1) + bus_seg + bake(arc2)
  return points
```

---

## 10) Rendering outputs

### 10.1 Option A (fast prototyping): Line2D
- Feed baked points to Line2D.
- Set `width` for base thickness.
- Optionally use `width_curve` to taper near endpoints or emphasize direction. [web:193]

Caveat: Line2D fidelity depends on segmenting and bake resolution; you may increase bake density for smooth bevel-like edges.

### 10.2 Option B (recommended): Ribbon mesh + shader
- Build a quad strip along the polyline with per-vertex normal.
- Shader paints core + rim highlight + subtle shadow (bevel illusion).

---

## 11) Debugging overlay (required)

Draw:
- Node rects (inflated)
- Reserved zones
- Port points with IDs
- Each net polyline with lane color
- Collision points + chosen detour label

This ensures “predictability over magic,” consistent with the project’s architecture principles. [cite:180]

---

## 12) Screen-reading / blur ordering notes (glass panes)

If multiple screen-reading shaders sample the screen texture in 2D, Godot recommends using `BackBufferCopy` between them so later elements don’t unexpectedly sample the same buffer and cause earlier elements to “disappear” or look wrong; `BackBufferCopy` can copy whole screen or region. [web:54]  
If using region copy, never sample outside the copied region; results are undefined and may show garbage from previous frames. [web:54]

---
```

If you want the next step, I can convert this into **actual GDScript skeletons** (`router.gd`, `port.gd`, `net.gd`) that match Solido’s schema-driven UI builder approach described in the repo README.