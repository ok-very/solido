extends GutTest

func test_create_sets_all_fields():
	var bus := HudBus.create(
		"center_bus",
		HudBus.ORIENT_VERT,
		3,
		Rect2(1519, 130, 72, 1900),
		HudRole.NAV,
		2.5,
	)
	assert_eq(bus.bus_id, "center_bus")
	assert_eq(bus.orientation, HudBus.ORIENT_VERT)
	assert_eq(bus.lane_count, 3)
	assert_eq(bus.rect, Rect2(1519, 130, 72, 1900))
	assert_eq(bus.role, HudRole.NAV)
	assert_almost_eq(bus.thickness_u, 2.5, 0.001)


func test_lane_offset_center():
	var bus := _make_bus()
	assert_almost_eq(bus.get_lane_offset(0, 24.0), 0.0, 0.001)


func test_lane_offset_spread():
	var bus := _make_bus()
	assert_almost_eq(bus.get_lane_offset(1, 24.0), 24.0, 0.001)
	assert_almost_eq(bus.get_lane_offset(2, 24.0), -24.0, 0.001)


func test_lane_position_vert():
	var bus := HudBus.create(
		"b",
		HudBus.ORIENT_VERT,
		3,
		Rect2(100, 0, 60, 500),
		HudRole.NAV,
	)
	# Center X = 100 + 30 = 130
	assert_almost_eq(bus.get_lane_position(0, 24.0), 130.0, 0.001)
	assert_almost_eq(bus.get_lane_position(1, 24.0), 154.0, 0.001)
	assert_almost_eq(bus.get_lane_position(2, 24.0), 106.0, 0.001)


func test_lane_position_horiz():
	var bus := HudBus.create(
		"b",
		HudBus.ORIENT_HORIZ,
		3,
		Rect2(0, 100, 500, 60),
		HudRole.NAV,
	)
	# Center Y = 100 + 30 = 130
	assert_almost_eq(bus.get_lane_position(0, 24.0), 130.0, 0.001)


func test_project_to_lane_vert():
	var bus := HudBus.create(
		"b",
		HudBus.ORIENT_VERT,
		3,
		Rect2(100, 50, 60, 400),
		HudRole.NAV,
	)
	var pos := bus.project_to_lane(0, Vector2(200, 200), 24.0)
	# Lane 0 center X = 130, Y clamped to [50, 450]
	assert_almost_eq(pos.x, 130.0, 0.001)
	assert_almost_eq(pos.y, 200.0, 0.001)


func test_project_to_lane_horiz():
	var bus := HudBus.create(
		"b",
		HudBus.ORIENT_HORIZ,
		3,
		Rect2(50, 100, 400, 60),
		HudRole.NAV,
	)
	var pos := bus.project_to_lane(0, Vector2(200, 200), 24.0)
	# Lane 0 center Y = 130, X clamped to [50, 450]
	assert_almost_eq(pos.x, 200.0, 0.001)
	assert_almost_eq(pos.y, 130.0, 0.001)


func test_project_clamps_to_bus_range():
	var bus := HudBus.create(
		"b",
		HudBus.ORIENT_VERT,
		1,
		Rect2(100, 50, 60, 400),
		HudRole.NAV,
	)
	# Source Y above bus top
	var pos_above := bus.project_to_lane(0, Vector2(200, 10), 24.0)
	assert_almost_eq(pos_above.y, 50.0, 0.001)
	# Source Y below bus bottom
	var pos_below := bus.project_to_lane(0, Vector2(200, 999), 24.0)
	assert_almost_eq(pos_below.y, 450.0, 0.001)


func test_pick_lane_deterministic():
	var bus := _make_bus()
	var lane_a := bus.pick_lane("active_to_focus")
	var lane_b := bus.pick_lane("active_to_focus")
	assert_eq(lane_a, lane_b)


func test_pick_lane_within_range():
	var bus := _make_bus()
	var lane := bus.pick_lane("some_net")
	assert_true(lane >= 0 and lane < bus.lane_count)


func test_pick_lane_zero_lanes():
	var bus := HudBus.create("b", HudBus.ORIENT_VERT, 0, Rect2(), HudRole.NAV)
	assert_eq(bus.pick_lane("any"), 0)


func _make_bus() -> HudBus:
	return HudBus.create(
		"center_bus",
		HudBus.ORIENT_VERT,
		3,
		Rect2(1519, 130, 72, 1900),
		HudRole.NAV,
		2.5,
	)
