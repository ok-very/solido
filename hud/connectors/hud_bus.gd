class_name HudBus
extends RefCounted

const ORIENT_VERT := "VERT"
const ORIENT_HORIZ := "HORIZ"

var bus_id: String
var orientation: String
var lane_count: int
var rect: Rect2
var role: int
var thickness_u: float


static func create(
		p_bus_id: String,
		p_orientation: String,
		p_lane_count: int,
		p_rect: Rect2,
		p_role: int,
		p_thickness_u: float = 2.5,
) -> HudBus:
	var bus := HudBus.new()
	bus.bus_id = p_bus_id
	bus.orientation = p_orientation
	bus.lane_count = p_lane_count
	bus.rect = p_rect
	bus.role = p_role
	bus.thickness_u = p_thickness_u
	return bus


func get_lane_offset(lane_index: int, unit_px: float) -> float:
	if lane_index == 0:
		return 0.0
	var side_sign: int = 1 if lane_index % 2 == 1 else -1
	var rank: int = (lane_index + 1) / 2
	return float(side_sign * rank) * unit_px


func get_lane_position(lane_index: int, unit_px: float) -> float:
	var center: float
	if orientation == ORIENT_VERT:
		center = rect.position.x + rect.size.x * 0.5
	else:
		center = rect.position.y + rect.size.y * 0.5
	return center + get_lane_offset(lane_index, unit_px)


func project_to_lane(
		lane_index: int,
		source_pos: Vector2,
		unit_px: float,
) -> Vector2:
	var lane_coord := get_lane_position(lane_index, unit_px)
	if orientation == ORIENT_VERT:
		var y_clamped := clampf(source_pos.y, rect.position.y, rect.end.y)
		return Vector2(lane_coord, y_clamped)
	else:
		var x_clamped := clampf(source_pos.x, rect.position.x, rect.end.x)
		return Vector2(x_clamped, lane_coord)


func pick_lane(net_id: String) -> int:
	if lane_count <= 0:
		return 0
	return net_id.hash() % lane_count
