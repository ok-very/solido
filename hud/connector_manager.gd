class_name ConnectorManager
extends Node

const CONNECTOR_SCENE := preload("res://hud/primitives/spline_connector.tscn")
const RAIL_SCENE := preload("res://hud/primitives/lcars_rail.tscn")

var _ports: Dictionary = { }
var _nets: Array[HudNet] = []
var _buses: Dictionary = { }
var _router: ConnectorRouter

var _port_positions: Dictionary = { }
var _connectors: Dictionary = { }
var _bus_rails: Dictionary = { }
var _dirty: bool = false

signal connectors_updated


func _ready() -> void:
	_router = ConnectorRouter.new()


func configure(
		ports: Dictionary,
		nets: Array[HudNet],
		buses: Dictionary,
) -> void:
	_ports = ports
	_nets = nets
	_buses = buses
	_dirty = true


func update_port_positions(panel_name: String, ports: Dictionary) -> void:
	for port_id in ports:
		_port_positions[port_id] = ports[port_id]
	_dirty = true


func update_all_port_positions(all_ports: Dictionary) -> void:
	_port_positions = all_ports.duplicate()
	_dirty = true


func update_buses(buses: Dictionary) -> void:
	_buses = buses
	_dirty = true


func _process(_delta: float) -> void:
	if _dirty:
		_dirty = false
		_update_bus_rails()
		_execute_route()


func _execute_route() -> void:
	var results := _router.route_all(
		_ports,
		_port_positions,
		_nets,
		_buses,
		HudTheme.unit_px,
	)

	var active_keys: Dictionary = { }

	for result in results:
		var net_id: String = result["net_id"]
		var segments: Array = result["segments"]
		var role_val: int = result["role"]
		var importance: int = result["importance"]

		for seg_idx in segments.size():
			var seg: Dictionary = segments[seg_idx]
			var seg_type: String = seg["type"]
			if seg_type == "bus":
				continue
			var key := net_id + ":" + str(seg_idx)
			active_keys[key] = true

			var connector: SplineConnector
			if _connectors.has(key):
				connector = _connectors[key]
			else:
				connector = CONNECTOR_SCENE.instantiate()
				add_child(connector)
				_connectors[key] = connector

			connector.set_role(role_val)
			connector.set_importance(importance)

			var points: PackedVector2Array = seg["points"]
			connector.set_points_from_bake(points)

	var orphan_keys: Array = []
	for key in _connectors:
		if not active_keys.has(key):
			orphan_keys.append(key)
	for key in orphan_keys:
		var connector: SplineConnector = _connectors[key]
		connector.queue_free()
		_connectors.erase(key)

	connectors_updated.emit()


func _update_bus_rails() -> void:
	var active_ids: Dictionary = { }
	for bus_id in _buses:
		active_ids[bus_id] = true
		var bus: HudBus = _buses[bus_id]
		var rail: LcarsRail
		if _bus_rails.has(bus_id):
			rail = _bus_rails[bus_id]
		else:
			rail = RAIL_SCENE.instantiate()
			rail.orientation = 1 if bus.orientation == HudBus.ORIENT_VERT else 0
			rail.thickness_u = 1.0
			rail.segment_count = 1
			rail.corner_radius_u = 0.5
			rail.role = bus.role
			rail.mouse_filter = Control.MOUSE_FILTER_IGNORE
			add_child(rail)
			_bus_rails[bus_id] = rail
		rail.position = bus.rect.position
		rail.size = bus.rect.size
		rail.modulate.a = 0.7

	var orphan_ids: Array = []
	for bus_id in _bus_rails:
		if not active_ids.has(bus_id):
			orphan_ids.append(bus_id)
	for bus_id in orphan_ids:
		_bus_rails[bus_id].queue_free()
		_bus_rails.erase(bus_id)


func get_connector_count() -> int:
	return _connectors.size()


func get_bus_rail_count() -> int:
	return _bus_rails.size()


func get_bus_rail(bus_id: String) -> LcarsRail:
	return _bus_rails.get(bus_id)
