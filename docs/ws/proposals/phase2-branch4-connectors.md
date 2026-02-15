# Phase 2 Branch 4 — Connector Data Layer + Router

Blueprint for the connector system: data classes, routing engine, connector manager, and WorkbenchHUD integration.

---

## 0. Context

**Completed branches:**
- Branch 1: Composed panels (StackPanel, InspectorPanel, StatusStrip) — structural frames
- Branch 2: Collapsible sections + param rows — content layer
- Branch 3: WorkbenchHUD scene assembly — 4 CanvasLayers (WorldFeed, GlassLayer, StructureLayer, ConnectorLayer, TextLayer)

**What exists:**
- `hud/workbench_hud.{gd,tscn}` — top-level CanvasLayer orchestrator (flat, not in subdirectory)
- `hud/primitives/spline_connector.{gd,tscn}` — rendering primitive with `set_curve_points()`, `set_points_from_bake()`, `set_bus_segment()`, importance classes, hit detection
- 3 composed panels each exposing `get_port_positions() -> Dictionary` and `signal port_positions_changed(ports: Dictionary)`
- WorkbenchHUD forwards these as `panel_port_positions_changed(panel_name, ports)` and exposes `get_all_port_positions() -> Dictionary`
- ConnectorLayer exists in the scene tree (`z_index=2`, `mouse_filter=MOUSE_FILTER_IGNORE`), currently empty

**What's missing:**
- Port/Net/Bus data classes
- Routing engine (Bezier construction, bus-branch routing, port direction mapping)
- ConnectorManager (lifecycle of SplineConnector instances)
- Wiring between panel port signals and connector updates

---

## 1. Scope Decision

**Choice: (b) Data classes + basic routing, without collision/detour.**

Branch 4 delivers:
- HudPort, HudNet, HudBus data classes
- ConnectorRouter with direct arc routing and bus-branch routing
- ConnectorManager that spawns/manages SplineConnectors
- Full integration with WorkbenchHUD port signals
- Debug overlay (port markers + net polylines, toggleable)

**Deferred to Branch 5 (or later):**
- Collision detection (tube-vs-rect checks against panel rects and reserved zones)
- Detour candidates (lane shifts, control point push, elbow fallback)
- Reserved zone generation from panel children
- Curvature clamp / max_curvature validation
- Cap rendering (DOT, ENDCAP, BRACKET cap glyphs on SplineConnector)

**Rationale:**
The full router with obstacle avoidance is ~300+ lines of collision geometry code that adds zero visual value until we have enough connectors to actually collide. The 5 nets defined for the demo HUD route through open space between well-separated panels — collisions are geometrically impossible at the current layout. Shipping data classes + basic routing first lets us validate the end-to-end signal flow (panel resize -> port update -> route -> render) and visually confirm the LCARS connector aesthetic before layering complexity.

---

## 2. File Manifest

| File | Type | Est. Lines | Purpose |
|------|------|-----------|---------|
| `hud/connectors/hud_port.gd` | GDScript (RefCounted) | ~60 | Port data class |
| `hud/connectors/hud_net.gd` | GDScript (RefCounted) | ~55 | Net data class |
| `hud/connectors/hud_bus.gd` | GDScript (RefCounted) | ~65 | Bus data class with lane projection |
| `hud/connectors/connector_router.gd` | GDScript (RefCounted) | ~180 | Deterministic routing engine |
| `hud/connector_manager.gd` | GDScript (Node) | ~200 | Spawns/manages SplineConnector instances, lives in scene tree |
| `test/unit/test_hud_port.gd` | GDScript (GutTest) | ~60 | Port data class tests |
| `test/unit/test_hud_net.gd` | GDScript (GutTest) | ~50 | Net data class tests |
| `test/unit/test_hud_bus.gd` | GDScript (GutTest) | ~70 | Bus data class + lane projection tests |
| `test/unit/test_connector_router.gd` | GDScript (GutTest) | ~200 | Router determinism, arc construction, bus-branch routing |
| `test/unit/test_connector_manager.gd` | GDScript (GutTest) | ~150 | Manager lifecycle, signal wiring, SplineConnector spawning |

**Total:** 5 production files (~560 lines), 5 test files (~530 lines)

### Path rationale

- Data classes go in `hud/connectors/` (new directory) — logically grouped, separate from primitives
- `connector_manager.gd` lives at `hud/connector_manager.gd` (flat, alongside `workbench_hud.gd`) — it's a workbench-level orchestration node, not a data class. The original blueprint placed it in `hud/workbench/` but WorkbenchHUD was built flat, so the manager follows suit
- No `.tscn` for ConnectorManager — it's a plain Node added as a child of ConnectorLayer at runtime by WorkbenchHUD, or instantiated in the .tscn as a child of ConnectorLayer

---

## 3. Data Class Design

### 3.1 HudPort

```
class_name HudPort
extends RefCounted

# Port data class. Immutable once constructed.
# Represents an attachment point on a panel boundary.

# --- Properties ---
var port_id: String          # e.g. "stack.active"
var node_id: String          # e.g. "stack" (panel name)
var side: String             # "N", "E", "S", "W"
var pos_u: float             # 0.0-1.0 fraction along side
var role: int                # HudRole constant
var priority: int            # 0-100, higher = more important
var capacity: int            # max nets before fanout (default 1)
var style: String            # "ARC" or "ELBOW"
var cap: String              # "NONE", "DOT", "ENDCAP", "BRACKET"

# --- Constructor ---
static func create(p_port_id: String, p_node_id: String, p_side: String,
        p_pos_u: float, p_role: int, p_priority: int,
        p_style: String = "ARC", p_cap: String = "NONE",
        p_capacity: int = 1) -> HudPort:
    var port := HudPort.new()
    port.port_id = p_port_id
    port.node_id = p_node_id
    port.side = p_side
    port.pos_u = p_pos_u
    port.role = p_role
    port.priority = p_priority
    port.style = p_style
    port.cap = p_cap
    port.capacity = p_capacity
    return port

# --- Utility ---
static func dir_for_side(side: String) -> Vector2:
    # Returns the outward normal direction for a port side.
    match side:
        "N": return Vector2(0, -1)
        "E": return Vector2(1, 0)
        "S": return Vector2(0, 1)
        "W": return Vector2(-1, 0)
        _: return Vector2.ZERO
```

**Notes:**
- `RefCounted`, not `Resource` — these are ephemeral runtime data, never saved to disk.
- Static `create()` factory instead of overloading `_init()` — avoids issues with GDScript constructor limitations (all params would need defaults, order matters).
- `dir_for_side()` is static on HudPort because it's port-centric. Router calls it for Bezier control point calculation.

### 3.2 HudNet

```
class_name HudNet
extends RefCounted

# Net data class. Represents a logical connection between ports.

# --- Kind constants ---
const KIND_DATAFLOW := "DATAFLOW"
const KIND_HIGHLIGHT := "HIGHLIGHT"
const KIND_GROUP := "GROUP"
const KIND_WARNING := "WARNING"

# --- Properties ---
var net_id: String           # e.g. "active_to_focus"
var kind: String             # KIND_* constant
var importance: int          # 0-100, determines visual class
var from_port_id: String     # source port_id
var to_port_id: String       # destination port_id
var routing_mode: String     # "DIRECT_ARC" or "BUS_BRANCH"
var bus_id: String           # only used when routing_mode == "BUS_BRANCH"

# --- Constructor ---
static func create(p_net_id: String, p_kind: String, p_importance: int,
        p_from: String, p_to: String,
        p_routing_mode: String = "DIRECT_ARC",
        p_bus_id: String = "") -> HudNet:
    var net := HudNet.new()
    net.net_id = p_net_id
    net.kind = p_kind
    net.importance = p_importance
    net.from_port_id = p_from
    net.to_port_id = p_to
    net.routing_mode = p_routing_mode
    net.bus_id = p_bus_id
    return net

# --- Utility ---
func get_visual_class() -> String:
    if importance >= 80:
        return "primary"
    elif importance >= 50:
        return "secondary"
    return "tertiary"
```

### 3.3 HudBus

```
class_name HudBus
extends RefCounted

# Bus data class. Shared backbone rail that nets can route through.

# --- Orientation constants ---
const ORIENT_VERT := "VERT"
const ORIENT_HORIZ := "HORIZ"

# --- Properties ---
var bus_id: String           # e.g. "center_bus"
var orientation: String      # ORIENT_VERT or ORIENT_HORIZ
var lane_count: int          # number of parallel lanes
var rect: Rect2              # bounding rect in UI coords (x, y, w, h)
var role: int                # HudRole constant
var thickness_u: float       # base thickness in u-units

# --- Constructor ---
static func create(p_bus_id: String, p_orientation: String, p_lane_count: int,
        p_rect: Rect2, p_role: int, p_thickness_u: float = 2.5) -> HudBus:
    var bus := HudBus.new()
    bus.bus_id = p_bus_id
    bus.orientation = p_orientation
    bus.lane_count = p_lane_count
    bus.rect = p_rect
    bus.role = p_role
    bus.thickness_u = p_thickness_u
    return bus

# --- Lane projection ---
func get_lane_offset(lane_index: int, unit_px: float) -> float:
    # Returns the perpendicular offset from bus center for a given lane.
    # Lane 0 = center, lanes spread symmetrically: 0, -1u, +1u, -2u, +2u...
    if lane_index == 0:
        return 0.0
    var side: int = 1 if lane_index % 2 == 1 else -1
    var rank: int = (lane_index + 1) / 2
    return float(side * rank) * unit_px

func get_lane_position(lane_index: int, unit_px: float) -> float:
    # Returns the absolute perpendicular coordinate for a lane.
    # For VERT bus: returns X coordinate. For HORIZ: returns Y coordinate.
    var center: float
    if orientation == ORIENT_VERT:
        center = rect.position.x + rect.size.x * 0.5
    else:
        center = rect.position.y + rect.size.y * 0.5
    return center + get_lane_offset(lane_index, unit_px)

func project_to_lane(lane_index: int, source_pos: Vector2, unit_px: float) -> Vector2:
    # Projects a source position onto a bus lane.
    # Returns the closest point on the lane line to the source.
    var lane_coord := get_lane_position(lane_index, unit_px)
    if orientation == ORIENT_VERT:
        # Vertical bus: lane_coord is X, clamp Y to bus vertical range
        var y_clamped := clampf(source_pos.y, rect.position.y, rect.end.y)
        return Vector2(lane_coord, y_clamped)
    else:
        # Horizontal bus: lane_coord is Y, clamp X to bus horizontal range
        var x_clamped := clampf(source_pos.x, rect.position.x, rect.end.x)
        return Vector2(x_clamped, lane_coord)

func pick_lane(net_id: String) -> int:
    # Deterministic lane assignment: hash net_id, modulo lane_count.
    return net_id.hash() % lane_count if lane_count > 0 else 0
```

**Notes:**
- `rect.end` is a property on `Rect2` in Godot 4 that returns `position + size`. Verified via existing codebase patterns (GlassPane uses `get_rect()`).
- `pick_lane()` uses `String.hash()` for determinism — same `net_id` always gets the same lane.
- Lane offset uses symmetric spread (0, -1u, +1u) so the center lane is always lane 0 (highest priority gets the cleanest path).

---

## 4. ConnectorRouter Design

### 4.1 Role

Pure computation. No scene tree dependency, no Node, no signals. Takes data in, returns route data out. Fully testable without Godot scene tree.

### 4.2 Class Structure

```
class_name ConnectorRouter
extends RefCounted

# Deterministic routing engine.
# Input: ports (resolved positions), nets, buses, unit_px.
# Output: Array of RouteResult (per net).

# --- Inner data: RouteResult ---
# Each RouteResult is a Dictionary with these keys:
#   "net_id": String
#   "segments": Array[Dictionary]  — each segment is:
#       { "points": PackedVector2Array, "type": "arc" | "bus" }
#   "role": int
#   "importance": int
```

### 4.3 Input Contract

```
func route_all(
    ports: Dictionary,        # port_id -> HudPort (with resolved global position in .position)
    port_positions: Dictionary, # port_id -> Vector2 (global positions from panels)
    nets: Array[HudNet],
    buses: Dictionary,        # bus_id -> HudBus
    unit_px: float
) -> Array[Dictionary]:       # Array of RouteResult dictionaries
```

**Why `port_positions` is separate from `ports`:** The HudPort objects define the port schema (side, role, priority). The port_positions dictionary comes from the live panel `get_port_positions()` calls and contains the actual screen coordinates. Keeping them separate means port schema is static config while positions update on resize.

### 4.4 Routing Algorithm (Branch 4 scope)

```
func route_all(...) -> Array[Dictionary]:
    # 1. Sort nets by importance DESC, net_id ASC (deterministic)
    var sorted_nets := _sort_nets(nets)

    # 2. Route each net
    var results: Array[Dictionary] = []
    for net in sorted_nets:
        var result := _route_net(net, ports, port_positions, buses, unit_px)
        if result.size() > 0:
            results.append(result)
    return results


func _sort_nets(nets: Array[HudNet]) -> Array[HudNet]:
    # Godot's sort_custom is NOT stable (confirmed via Context7).
    # To guarantee determinism: encode a composite sort key, then sort.
    # Strategy: create [sort_key, net] pairs, sort by key.
    var keyed: Array = []
    for net in nets:
        # Importance descending (negate), then net_id ascending
        keyed.append([net, net.importance, net.net_id])
    keyed.sort_custom(func(a, b):
        if a[1] != b[1]:
            return a[1] > b[1]  # higher importance first
        return a[2] < b[2]      # alphabetical net_id tiebreak
    )
    var result: Array[HudNet] = []
    for entry in keyed:
        result.append(entry[0])
    return result


func _route_net(net: HudNet, ports: Dictionary, port_positions: Dictionary,
        buses: Dictionary, unit_px: float) -> Dictionary:
    var from_port: HudPort = ports.get(net.from_port_id)
    var to_port: HudPort = ports.get(net.to_port_id)
    var from_pos: Vector2 = port_positions.get(net.from_port_id, Vector2.ZERO)
    var to_pos: Vector2 = port_positions.get(net.to_port_id, Vector2.ZERO)

    if from_port == null or to_port == null:
        return {}

    var segments: Array[Dictionary] = []

    if net.routing_mode == "BUS_BRANCH" and buses.has(net.bus_id):
        segments = _route_bus_branch(from_port, from_pos, to_port, to_pos,
                                     buses[net.bus_id], unit_px)
    else:
        segments = _route_direct_arc(from_port, from_pos, to_port, to_pos, unit_px)

    return {
        "net_id": net.net_id,
        "segments": segments,
        "role": from_port.role,
        "importance": net.importance,
    }
```

### 4.5 Direct Arc Construction

```
func _route_direct_arc(from_port: HudPort, from_pos: Vector2,
        to_port: HudPort, to_pos: Vector2, unit_px: float) -> Array[Dictionary]:
    var curve := Curve2D.new()
    var d := from_pos.distance_to(to_pos)
    var min_t := 3.0 * unit_px    # 72px
    var max_t := 16.0 * unit_px   # 384px
    var t := clampf(d * 0.25, min_t, max_t)

    var dir0 := HudPort.dir_for_side(from_port.side)
    var dir3 := HudPort.dir_for_side(to_port.side)

    var p1 := from_pos + dir0 * t
    var p2 := to_pos - dir3 * t    # approach INTO to_port

    # Curve2D.add_point(position, in_control, out_control)
    # First point: no in-control, out-control = p1 - from_pos
    curve.add_point(from_pos, Vector2.ZERO, p1 - from_pos)
    # Second point: in-control = p2 - to_pos, no out-control
    curve.add_point(to_pos, p2 - to_pos, Vector2.ZERO)

    curve.bake_interval = 8.0
    var points := curve.get_baked_points()

    return [{ "points": points, "type": "arc" }]
```

**Curve2D usage verified:** `add_point(position, in_control, out_control)` where controls are relative to the point. `bake_interval` controls density. `get_baked_points()` returns `PackedVector2Array`. This matches how SplineConnector already uses Curve2D in `set_curve_points()`.

### 4.6 Bus-Branch Routing

```
func _route_bus_branch(from_port: HudPort, from_pos: Vector2,
        to_port: HudPort, to_pos: Vector2,
        bus: HudBus, unit_px: float) -> Array[Dictionary]:
    # 1. Pick lane deterministically
    var lane := bus.pick_lane(from_port.port_id + ":" + to_port.port_id)

    # 2. Compute entry/exit points on bus lane
    var entry := bus.project_to_lane(lane, from_pos, unit_px)
    var exit := bus.project_to_lane(lane, to_pos, unit_px)

    # 3. Build arc: from_port -> bus entry
    var bus_facing_from := _bus_side_facing(bus, from_pos)
    var arc1 := _build_arc(from_pos, from_port.side, entry, bus_facing_from, unit_px)

    # 4. Bus segment (straight line)
    var bus_seg := PackedVector2Array([entry, exit])

    # 5. Build arc: bus exit -> to_port
    var bus_facing_to := _bus_side_facing(bus, to_pos)
    var arc2 := _build_arc(exit, bus_facing_to, to_pos, to_port.side, unit_px)

    return [
        { "points": arc1, "type": "arc" },
        { "points": bus_seg, "type": "bus" },
        { "points": arc2, "type": "arc" },
    ]


func _bus_side_facing(bus: HudBus, source_pos: Vector2) -> String:
    # Determine which side of the bus faces the source position.
    if bus.orientation == HudBus.ORIENT_VERT:
        var bus_center_x := bus.rect.position.x + bus.rect.size.x * 0.5
        return "W" if source_pos.x < bus_center_x else "E"
    else:
        var bus_center_y := bus.rect.position.y + bus.rect.size.y * 0.5
        return "N" if source_pos.y < bus_center_y else "S"


func _build_arc(p0: Vector2, side0: String, p3: Vector2, side3: String,
        unit_px: float) -> PackedVector2Array:
    var curve := Curve2D.new()
    var d := p0.distance_to(p3)
    var min_t := 3.0 * unit_px
    var max_t := 16.0 * unit_px
    var t := clampf(d * 0.25, min_t, max_t)

    var dir0 := HudPort.dir_for_side(side0)
    var dir3 := HudPort.dir_for_side(side3)

    var p1 := p0 + dir0 * t
    var p2 := p3 - dir3 * t

    curve.add_point(p0, Vector2.ZERO, p1 - p0)
    curve.add_point(p3, p2 - p3, Vector2.ZERO)
    curve.bake_interval = 8.0
    return curve.get_baked_points()
```

### 4.7 Determinism Guarantees

1. **Net ordering**: Sorted by `(-importance, net_id)` with explicit comparator. Unstable sort is safe because the composite key `(importance, net_id)` is unique — no two nets share the same `net_id`.
2. **Lane assignment**: `String.hash() % lane_count` — deterministic for same input.
3. **Bezier construction**: Control distance `clamp(d * 0.25, 3u, 16u)` — same positions produce same control points.
4. **Bake interval**: Fixed 8px — same curve produces same point count.

Port position quantization (round to nearest 0.5u / 12px) is **deferred** to Branch 5. At the current stage, panels use anchor-based layout which produces stable positions at a given viewport size. Quantization matters when we have collision detection and need sub-pixel stability, which Branch 5 will address.

---

## 5. ConnectorManager Design

### 5.1 Scene Tree Placement

```
WorkbenchHUD (CanvasLayer)
  ├── WorldFeed
  ├── GlassLayer
  ├── StructureLayer
  ├── ConnectorLayer (Control, z_index=2, mouse_filter=IGNORE)
  │   └── ConnectorManager (Node)           <-- NEW
  │       ├── SplineConnector (net: active_to_focus, segment 0: arc)
  │       ├── SplineConnector (net: active_to_focus, segment 1: bus)
  │       ├── SplineConnector (net: active_to_focus, segment 2: arc)
  │       ├── SplineConnector (net: selection_to_status, segment 0: arc)
  │       ├── SplineConnector (net: stack_telem_to_status, segment 0: arc)
  │       ├── SplineConnector (net: inspector_telem_to_status, segment 0: arc)
  │       └── SplineConnector (net: mode_to_inspector, segment 0: arc)
  └── TextLayer
```

ConnectorManager is a **Node** (not Control) — it manages children but has no visual representation itself. SplineConnectors are Control nodes that render via their internal Line2D children.

### 5.2 Class Structure

```
class_name ConnectorManager
extends Node

# Manages SplineConnector lifecycle. Owns the net/port/bus definitions
# and delegates routing to ConnectorRouter.

const CONNECTOR_SCENE := preload("res://hud/primitives/spline_connector.tscn")

# --- Configuration ---
var _ports: Dictionary = {}           # port_id -> HudPort
var _nets: Array[HudNet] = []
var _buses: Dictionary = {}           # bus_id -> HudBus
var _router: ConnectorRouter

# --- Runtime state ---
var _port_positions: Dictionary = {}  # port_id -> Vector2 (live positions)
var _connectors: Dictionary = {}      # "net_id:segment_index" -> SplineConnector
var _dirty: bool = false              # marks that a re-route is needed

# --- Signals ---
signal connectors_updated                # emitted after a route pass completes
```

### 5.3 Initialization

```
func _ready() -> void:
    _router = ConnectorRouter.new()


func configure(ports: Dictionary, nets: Array[HudNet], buses: Dictionary) -> void:
    # Called once by WorkbenchHUD after scene is ready.
    # ports: port_id -> HudPort
    _ports = ports
    _nets = nets
    _buses = buses
    _dirty = true
```

### 5.4 Port Position Updates

```
func update_port_positions(panel_name: String, ports: Dictionary) -> void:
    # Called when a panel emits port_positions_changed.
    # ports is e.g. {"stack.active": Vector2(...), "stack.selection": Vector2(...)}
    for port_id in ports:
        _port_positions[port_id] = ports[port_id]
    _dirty = true


func update_all_port_positions(all_ports: Dictionary) -> void:
    # Bulk update — called during initial setup.
    _port_positions = all_ports.duplicate()
    _dirty = true
```

### 5.5 Route + Render Cycle

```
func _process(_delta: float) -> void:
    if _dirty:
        _dirty = false
        _execute_route()


func _execute_route() -> void:
    # 1. Route
    var results := _router.route_all(_ports, _port_positions, _nets, _buses,
                                     HudTheme.unit_px)

    # 2. Track which connector keys are still valid
    var active_keys: Dictionary = {}

    # 3. Create/update SplineConnectors
    for result in results:
        var net_id: String = result["net_id"]
        var segments: Array = result["segments"]
        var role: int = result["role"]
        var importance: int = result["importance"]

        for seg_idx in segments.size():
            var seg: Dictionary = segments[seg_idx]
            var key := net_id + ":" + str(seg_idx)
            active_keys[key] = true

            var connector: SplineConnector
            if _connectors.has(key):
                connector = _connectors[key]
            else:
                connector = CONNECTOR_SCENE.instantiate()
                add_child(connector)
                _connectors[key] = connector

            # Configure connector
            connector.set_role(role)
            connector.set_importance(importance)

            var points: PackedVector2Array = seg["points"]
            var seg_type: String = seg["type"]

            if seg_type == "bus":
                connector.set_bus_segment(points[0], points[points.size() - 1])
            else:
                connector.set_points_from_bake(points)

    # 4. Remove orphaned connectors
    var orphan_keys: Array = []
    for key in _connectors:
        if not active_keys.has(key):
            orphan_keys.append(key)
    for key in orphan_keys:
        var connector: SplineConnector = _connectors[key]
        connector.queue_free()
        _connectors.erase(key)

    connectors_updated.emit()
```

### 5.6 Key Design Decisions

- **Deferred re-route via `_dirty` flag in `_process()`**: Multiple panels may resize in the same frame (e.g., on window resize). Rather than re-routing once per panel signal, we mark dirty and batch all updates into one route pass per frame. This is the standard Godot pattern for deferred work — `_process()` runs once per frame after all notifications.

- **One SplineConnector per segment, not per net**: A bus-branch net produces 3 segments (arc + bus + arc). Each segment gets its own SplineConnector because they may have different rendering paths (`set_bus_segment` vs `set_points_from_bake`). The key format `"net_id:segment_index"` uniquely identifies each.

- **`set_points_from_bake()` for arcs, not `set_curve_points()`**: The router already bakes the Curve2D into a `PackedVector2Array`. Passing pre-baked points avoids double-baking (router bakes, then SplineConnector would bake again if we used `set_curve_points()`).

---

## 6. Integration with WorkbenchHUD

### 6.1 Scene Tree Changes

**Option A (preferred): Add ConnectorManager as child of ConnectorLayer in .tscn**

Add to `workbench_hud.tscn`:
```
[ext_resource type="Script" path="res://hud/connector_manager.gd" id="6"]

[node name="ConnectorManager" type="Node" parent="ConnectorLayer"]
script = ExtResource("6")
```

**Option B: WorkbenchHUD creates ConnectorManager in _ready()**

Either works. Option A is preferred because it's visible in the scene tree and follows the existing pattern (panels are instanced in .tscn, not created in code).

### 6.2 WorkbenchHUD Changes

New member variable and initialization in `workbench_hud.gd`:

```
var _connector_manager: ConnectorManager   # add to declarations

# In _ready(), after existing panel signal connections:
_connector_manager = $ConnectorLayer/ConnectorManager

# Configure with port schema (static definitions)
_connector_manager.configure(
    _build_port_definitions(),
    _build_net_definitions(),
    _build_bus_definitions()
)

# Wire panel port position signals
_connector_manager.update_all_port_positions(get_all_port_positions())

# Connect panel signals to manager
panel_port_positions_changed.connect(
    func(panel_name: String, ports: Dictionary):
        _connector_manager.update_port_positions(panel_name, ports)
)
```

### 6.3 Schema Definition Methods

New private methods on WorkbenchHUD:

```
func _build_port_definitions() -> Dictionary:
    # Returns port_id -> HudPort for all 9 ports
    var ports := {}
    ports["stack.active"] = HudPort.create("stack.active", "stack", "E",
        0.2, HudRole.BLEND, 90, "ARC", "ENDCAP")
    ports["stack.selection"] = HudPort.create("stack.selection", "stack", "E",
        0.5, HudRole.NAV, 70, "ARC", "DOT")
    ports["stack.telemetry"] = HudPort.create("stack.telemetry", "stack", "S",
        0.5, HudRole.TELEMETRY, 40, "ARC", "NONE")
    ports["inspector.focus"] = HudPort.create("inspector.focus", "inspector", "W",
        0.35, HudRole.BLEND, 90, "ARC", "BRACKET")
    ports["inspector.shader"] = HudPort.create("inspector.shader", "inspector", "W",
        0.6, HudRole.EDIT, 60, "ARC", "DOT")
    ports["inspector.telemetry"] = HudPort.create("inspector.telemetry", "inspector", "S",
        0.5, HudRole.TELEMETRY, 40, "ARC", "NONE")
    ports["status.mode"] = HudPort.create("status.mode", "status", "N",
        0.15, HudRole.NAV, 50, "ARC", "NONE")
    ports["status.selection"] = HudPort.create("status.selection", "status", "N",
        0.4, HudRole.NAV, 60, "ARC", "BRACKET")
    ports["status.perf"] = HudPort.create("status.perf", "status", "N",
        0.7, HudRole.TELEMETRY, 30, "ARC", "NONE")
    return ports


func _build_net_definitions() -> Array[HudNet]:
    return [
        HudNet.create("active_to_focus", HudNet.KIND_HIGHLIGHT, 90,
            "stack.active", "inspector.focus", "BUS_BRANCH", "center_bus"),
        HudNet.create("selection_to_status", HudNet.KIND_DATAFLOW, 70,
            "stack.selection", "status.selection"),
        HudNet.create("stack_telem_to_status", HudNet.KIND_DATAFLOW, 40,
            "stack.telemetry", "status.perf"),
        HudNet.create("inspector_telem_to_status", HudNet.KIND_DATAFLOW, 40,
            "inspector.telemetry", "status.perf"),
        HudNet.create("mode_to_inspector", HudNet.KIND_HIGHLIGHT, 60,
            "status.mode", "inspector.shader"),
    ]


func _build_bus_definitions() -> Dictionary:
    # Bus rect is approximate — centered between panels.
    # StackPanel right edge at anchor 0.22, InspectorPanel left edge at anchor 0.74.
    # Midpoint at anchor ~0.48 -> ~1555px at 3240 width.
    # Bus width = 3u = 72px, so rect starts at 1555 - 36 = 1519.
    # Vertical range: from top panel area (0.06 -> 130px) to bottom (0.94 -> 2030px).
    var bus_rect := Rect2(1519, 130, 72, 1900)
    return {
        "center_bus": HudBus.create("center_bus", HudBus.ORIENT_VERT, 3,
            bus_rect, HudRole.NAV, 2.5),
    }
```

### 6.4 Signal Flow (End-to-End)

```
Panel resize (NOTIFICATION_RESIZED)
  -> panel.port_positions_changed.emit(panel.get_port_positions())
  -> WorkbenchHUD lambda -> panel_port_positions_changed.emit("stack", ports)
  -> WorkbenchHUD lambda -> _connector_manager.update_port_positions("stack", ports)
  -> _connector_manager._dirty = true
  -> next _process() frame:
     -> _connector_manager._execute_route()
     -> ConnectorRouter.route_all(ports, positions, nets, buses, unit_px)
     -> returns Array[RouteResult]
     -> for each result/segment: create or update SplineConnector
     -> SplineConnector.set_points_from_bake(points) or .set_bus_segment(entry, exit)
     -> SplineConnector._render_points() updates Line2D children
     -> visual update on screen
```

### 6.5 Bus Rect Responsiveness

The bus rect in `_build_bus_definitions()` uses hardcoded pixel coordinates based on 3240x2160 native resolution. This works for Branch 4 because:
- The demo HUD is designed for native res
- Panel anchors are fixed (StackPanel right=0.22, InspectorPanel left=0.74)

For Branch 5+ (responsiveness), the bus rect must be computed dynamically based on actual panel positions. This means `_build_bus_definitions()` becomes `_compute_bus_definitions()` that reads live panel rects. Deferred because it's the same responsive layout concern as panel collapse/ultrawide, not a connector-specific issue.

---

## 7. Test Plan

### 7.1 Unit Tests: Data Classes

**`test/unit/test_hud_port.gd`** (~60 lines)
- `test_create_default`: Verify all fields set by `HudPort.create()`
- `test_dir_for_side_all_directions`: N->Up, E->Right, S->Down, W->Left
- `test_dir_for_side_unknown`: Returns Vector2.ZERO for invalid side
- `test_port_ids_are_strings`: Type safety check

**`test/unit/test_hud_net.gd`** (~50 lines)
- `test_create_direct_arc`: Verify fields, default routing_mode
- `test_create_bus_branch`: Verify bus_id is stored
- `test_visual_class_primary`: importance=90 -> "primary"
- `test_visual_class_secondary`: importance=60 -> "secondary"
- `test_visual_class_tertiary`: importance=30 -> "tertiary"
- `test_visual_class_boundary`: importance=80 -> "primary", importance=50 -> "secondary"

**`test/unit/test_hud_bus.gd`** (~70 lines)
- `test_create`: Verify all fields
- `test_lane_offset_center`: lane 0 -> offset 0.0
- `test_lane_offset_spread`: lane 1 -> +1u, lane 2 -> -1u
- `test_lane_position_vert`: Returns correct X for vertical bus
- `test_lane_position_horiz`: Returns correct Y for horizontal bus
- `test_project_to_lane_vert`: Projects point onto vertical lane, clamps Y
- `test_project_to_lane_horiz`: Projects point onto horizontal lane, clamps X
- `test_project_clamps_to_bus_range`: Position outside bus range clamps to edge
- `test_pick_lane_deterministic`: Same net_id -> same lane, always

### 7.2 Unit Tests: ConnectorRouter

**`test/unit/test_connector_router.gd`** (~200 lines)
- **Determinism**: Route same inputs twice, assert identical output (point-by-point comparison)
- **Direct arc — basic**: Two ports (E to W), verify arc has >2 points, starts near from_pos, ends near to_pos
- **Direct arc — control distance**: Short distance -> min_t used, long distance -> max_t capped
- **Direct arc — all side combinations**: E-to-W, N-to-S, S-to-N, E-to-N (each pair produces valid arcs)
- **Net sorting**: 3 nets with different importance, verify route order matches sort
- **Bus-branch routing**: From E-side port through vertical bus to W-side port, verify 3 segments (arc + bus + arc)
- **Bus segment endpoints**: Bus segment starts/ends on the bus lane line
- **Missing port graceful**: Net referencing non-existent port -> empty result (no crash)
- **Empty nets**: No nets -> empty results array
- **Role propagation**: RouteResult carries from_port's role

### 7.3 Integration Tests: ConnectorManager

**`test/unit/test_connector_manager.gd`** (~150 lines)

Setup: Create a ConnectorManager, configure with 2 ports + 1 net, provide mock positions.

- **test_configure**: After configure(), _ports and _nets populated (verify via route output)
- **test_update_port_positions_triggers_route**: Call `update_port_positions()`, wait 1 frame, verify SplineConnector children spawned
- **test_connector_count_matches_segments**: 1 direct arc net -> 1 SplineConnector child. 1 bus-branch net -> 3 SplineConnector children.
- **test_connector_role_matches_net**: Spawned SplineConnector has correct role
- **test_connector_importance_matches_net**: Spawned SplineConnector has correct importance
- **test_connector_updated_on_position_change**: Change port position, wait 1 frame, verify SplineConnector baked points changed
- **test_orphan_connectors_freed**: Remove a net, re-route, verify SplineConnector count decreases
- **test_connectors_updated_signal**: Signal emitted after route pass
- **test_deferred_batch**: Two `update_port_positions()` calls in same frame -> only one route pass (count via signal emissions)

**Frame-wait pattern**: GUT tests can use `await get_tree().process_frame` to wait for `_process()` to fire. This is how we verify the deferred routing behavior.

### 7.4 What Not to Test in Branch 4

- Collision detection (not implemented)
- Detour candidates (not implemented)
- Cap rendering (visual, not yet wired)
- Port position quantization (deferred)

---

## 8. Build Sequence

Ordered steps for `godot-dev`. Each step is independently testable.

### Step 1: Data Classes (~2 hours)

1. Create `hud/connectors/` directory
2. Implement `hud/connectors/hud_port.gd` — HudPort RefCounted class with `create()` and `dir_for_side()`
3. Implement `hud/connectors/hud_net.gd` — HudNet RefCounted class with `create()` and `get_visual_class()`
4. Implement `hud/connectors/hud_bus.gd` — HudBus RefCounted class with `create()`, lane methods, `pick_lane()`
5. Write `test/unit/test_hud_port.gd`, `test_hud_net.gd`, `test_hud_bus.gd`
6. Run GUT: all data class tests pass
7. Run `gdscript-formatter --safe` on all new files
8. Commit: "Add connector data classes (HudPort, HudNet, HudBus)"

### Step 2: ConnectorRouter (~3 hours)

1. Implement `hud/connectors/connector_router.gd` — `route_all()`, `_sort_nets()`, `_route_direct_arc()`, `_route_bus_branch()`, `_build_arc()`, `_bus_side_facing()`
2. Write `test/unit/test_connector_router.gd` — determinism, arc construction, bus-branch, edge cases
3. Run GUT: all router tests pass
4. Run `gdscript-formatter --safe`
5. Commit: "Add ConnectorRouter with direct arc and bus-branch routing"

### Step 3: ConnectorManager (~2 hours)

1. Implement `hud/connector_manager.gd` — Node class with `configure()`, `update_port_positions()`, `_process()` deferred route, `_execute_route()`, orphan cleanup
2. Write `test/unit/test_connector_manager.gd` — lifecycle, signal wiring, connector spawning, batching
3. Run GUT: all manager tests pass
4. Run `gdscript-formatter --safe`
5. Commit: "Add ConnectorManager for SplineConnector lifecycle"

### Step 4: WorkbenchHUD Integration (~1.5 hours)

1. Add ConnectorManager node to `workbench_hud.tscn` as child of ConnectorLayer
2. Update `workbench_hud.gd`:
   - Add `_connector_manager` member
   - Add `_build_port_definitions()`, `_build_net_definitions()`, `_build_bus_definitions()`
   - Wire in `_ready()`: configure manager, update initial positions, connect signal
3. Update `test/unit/test_workbench_hud.gd` with new tests:
   - `test_connector_manager_exists`: Verify node in scene tree
   - `test_connector_manager_configured`: After ready, connectors exist
   - `test_port_position_change_updates_connectors`: Simulate resize, verify connectors updated
4. Run GUT: all tests pass (existing + new)
5. Run `gdscript-formatter --safe`
6. Run `godot-lsp` `scan_workspace_diagnostics` — zero errors
7. Commit: "Wire ConnectorManager into WorkbenchHUD"

### Step 5: Visual Verification (~1 hour)

1. Launch project in Godot editor, open WorkbenchHUD scene
2. Verify 5 nets render visually:
   - `active_to_focus`: thick primary connector through center bus (3 segments visible)
   - `selection_to_status`: medium arc from stack right side to status top
   - `stack_telem_to_status`: thin arc from stack bottom to status top
   - `inspector_telem_to_status`: thin arc from inspector bottom to status top
   - `mode_to_inspector`: medium arc from status top to inspector left
3. Resize window — verify connectors update (deferred, one frame lag is acceptable)
4. Screenshot for PR

---

## 9. Decision Log

### D1: Scope split — basic routing now, collision later

**Choice:** Branch 4 = data classes + basic routing. Branch 5 = collision + detours.

**Considered:**
- (a) Full router in one branch — all features including obstacle avoidance
- (b) Split: basic routing now, collision later

**Rationale for (b):** The 5 demo nets route through open space between well-separated panels. Collision detection adds ~300 lines of geometry code that cannot trigger. Shipping basic routing first validates the end-to-end signal pipeline. Collision detection is independently testable and adds no visual value until connectors can actually collide (which requires more panels or a denser layout).

### D2: ConnectorManager as Node, not Control

**Choice:** `extends Node`

**Considered:**
- `extends Control` — could use `_draw()` for debug overlay
- `extends Node` — pure management, no visual representation

**Rationale:** ConnectorManager doesn't draw anything itself. SplineConnectors are its children and handle their own rendering. Debug overlay (Branch 5+) can be a separate Control child added when debug mode is toggled, rather than baked into the manager.

### D3: One SplineConnector per segment, not per net

**Choice:** Bus-branch net gets 3 SplineConnector instances (arc + bus + arc).

**Considered:**
- One SplineConnector per net with concatenated points
- One per segment

**Rationale:** SplineConnector's `set_bus_segment()` and `set_points_from_bake()` serve different rendering paths. A bus segment is a straight line (2 points), while arcs are curved polylines (16-64 points). Mixing them in one SplineConnector would require the connector to internally distinguish segment types, which it currently doesn't. One-per-segment keeps the primitive simple and each connector's bounding rect accurate for hit detection.

### D4: Pre-baked points via `set_points_from_bake()`, not `set_curve_points()`

**Choice:** Router bakes Curve2D, passes `PackedVector2Array` to SplineConnector via `set_points_from_bake()`.

**Considered:**
- Pass (p0, p1, p2, p3) to `set_curve_points()` and let SplineConnector bake
- Pass pre-baked points

**Rationale:** The router already creates a Curve2D for arc construction and calls `get_baked_points()`. Passing those points directly avoids double-baking. It also allows the router to potentially modify baked points in future (e.g., collision avoidance nudges individual points) without changing the SplineConnector API.

### D5: Deferred routing via `_dirty` flag in `_process()`

**Choice:** Mark dirty on position change, batch route in `_process()`.

**Considered:**
- Route immediately on every `update_port_positions()` call
- Deferred via `call_deferred()`
- Deferred via `_process()` dirty flag

**Rationale:** Multiple panels can resize in the same frame (window resize triggers all panels). Routing once per frame avoids redundant computation. `_process()` is the standard Godot pattern for per-frame deferred work. `call_deferred()` also works but doesn't naturally deduplicate multiple calls in the same frame.

### D6: Static bus rect (hardcoded for 3240x2160)

**Choice:** Bus rect is hardcoded to native resolution coordinates.

**Considered:**
- Dynamic computation based on panel positions
- Hardcoded for native res

**Rationale:** Branch 4 targets the native-res demo. Dynamic bus positioning requires reading live panel rects after layout, which ties into the responsiveness work (Branch 5 / Phase 3). Hardcoding now avoids premature optimization and keeps the integration straightforward.

### D7: Port position quantization deferred

**Choice:** No quantization in Branch 4.

**Considered:**
- Quantize to nearest 0.5u (12px) immediately
- Defer quantization

**Rationale:** Quantization prevents sub-pixel jitter when panel positions land on non-integer coordinates. At native res with anchor-based layout, positions are already stable within a frame. Quantization becomes important when collision detection compares positions across frames, which is a Branch 5 concern.

### D8: `sort_custom` instability mitigation

**Choice:** Use composite sort key where the key tuple is guaranteed unique.

**Godot API fact (from Context7):** `Array.sort_custom()` is **not guaranteed stable**. Equal-keyed elements may swap positions across calls.

**Mitigation:** The sort key for nets is `(-importance, net_id)`. Since `net_id` is unique across all nets, no two elements share the same key, making instability irrelevant. Same pattern for port sorting: `(-priority, node_id, port_id)` is unique per port. This is explicitly called out in the router implementation so future developers don't assume stability.

### D9: connector_manager.gd lives flat at `hud/`, not in `hud/connectors/`

**Choice:** `hud/connector_manager.gd` alongside `hud/workbench_hud.gd`.

**Considered:**
- `hud/connectors/connector_manager.gd` — grouped with data classes
- `hud/connector_manager.gd` — flat, alongside workbench_hud

**Rationale:** ConnectorManager is a scene-tree Node that orchestrates SplineConnectors. It's a workbench-level component, not a data class. The data classes (HudPort, HudNet, HudBus) and the pure-computation router live in `hud/connectors/` because they have no scene tree dependencies. The manager, like WorkbenchHUD itself, lives flat in `hud/`.

---

## 10. Open Items for Branch 5+

- [ ] Collision detection: tube-vs-rect against panel rects and reserved zones
- [ ] Detour candidates: lane shift, control point push, elbow fallback
- [ ] Port position quantization (0.5u grid)
- [ ] Dynamic bus rect computation (responsive layout)
- [ ] Cap rendering (DOT/ENDCAP/BRACKET glyphs on SplineConnector endpoints)
- [ ] Debug overlay: toggleable Control that draws port markers, net polylines, bus rects
- [ ] Reserved zone generation from panel children
- [ ] Curvature clamp validation
- [ ] Multi-drop nets (one-to-many fan-out)
- [ ] Connector interaction: highlight on hover, tap-to-select wiring to selection state
