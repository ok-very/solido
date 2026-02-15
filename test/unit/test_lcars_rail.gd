extends GutTest

var _rail_scene = preload("res://hud/primitives/lcars_rail.tscn")


func _create_rail() -> LcarsRail:
	var rail = _rail_scene.instantiate()
	rail.size = Vector2(480, 72)
	add_child_autofree(rail)
	return rail


func test_rail_instantiates():
	var rail = _create_rail()
	assert_not_null(rail)
	assert_true(rail is Control)


func test_rail_default_exports():
	var rail = _create_rail()
	assert_eq(rail.role, 0)
	assert_eq(rail.orientation, 0)
	assert_eq(rail.thickness_u, 3.0)
	assert_eq(rail.segment_count, 1)


func test_rail_creates_segments():
	var rail = _create_rail()
	var container = rail.get_node("SegmentContainer")
	assert_not_null(container)
	assert_eq(container.get_child_count(), 1)


func test_rail_multi_segments():
	var rail = _rail_scene.instantiate()
	rail.segment_count = 3
	rail.size = Vector2(480, 72)
	add_child_autofree(rail)
	var container = rail.get_node("SegmentContainer")
	assert_eq(container.get_child_count(), 3)


func test_rail_port_positions():
	var rail = _create_rail()
	var ports = rail.get_port_positions()
	assert_true(ports.has("N"))
	assert_true(ports.has("S"))
	assert_true(ports.has("E"))
	assert_true(ports.has("W"))


func test_rail_role_change():
	var rail = _create_rail()
	rail.set_role(HudRole.EDIT)
	assert_eq(rail.role, HudRole.EDIT)


func test_rail_minimum_size():
	var rail = _create_rail()
	var min_touch = HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(rail.custom_minimum_size.y >= min_touch)


func test_rail_thickness_px():
	var rail = _create_rail()
	assert_eq(rail.get_thickness_px(), 3.0 * HudTheme.unit_px)


func test_rail_segment_rects():
	var rail = _create_rail()
	var rects = rail.get_segment_rects()
	assert_eq(rects.size(), 1)
