class_name HudPort
extends RefCounted

var port_id: String
var node_id: String
var side: String
var pos_u: float
var role: int
var priority: int
var capacity: int
var style: String
var cap: String


static func create(
		p_port_id: String,
		p_node_id: String,
		p_side: String,
		p_pos_u: float,
		p_role: int,
		p_priority: int,
		p_style: String = "ARC",
		p_cap: String = "NONE",
		p_capacity: int = 1,
) -> HudPort:
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


static func dir_for_side(p_side: String) -> Vector2:
	match p_side:
		"N":
			return Vector2(0, -1)
		"E":
			return Vector2(1, 0)
		"S":
			return Vector2(0, 1)
		"W":
			return Vector2(-1, 0)
		_:
			return Vector2.ZERO
