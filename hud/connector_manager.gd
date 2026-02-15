class_name ConnectorManager
extends Node

const CONNECTOR_SCENE := preload("res://hud/primitives/spline_connector.tscn")

var _ports: Dictionary = { }
var _nets: Array[HudNet] = []
var _buses: Dictionary = { }
var _router: ConnectorRouter

var _port_positions: Dictionary = { }
var _connectors: Dictionary = { }
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


func _process(_delta: float) -> void:
	if _dirty:
		_dirty = false
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
			var seg_type: String = seg["type"]

			if seg_type == "bus":
				connector.set_bus_segment(points[0], points[points.size() - 1])
			else:
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


func get_connector_count() -> int:
	return _connectors.size()
