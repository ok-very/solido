extends GutTest

func test_create_sets_all_fields():
	var port := HudPort.create(
		"stack.active",
		"stack",
		"E",
		0.2,
		HudRole.BLEND,
		90,
		"ARC",
		"ENDCAP",
		2,
	)
	assert_eq(port.port_id, "stack.active")
	assert_eq(port.node_id, "stack")
	assert_eq(port.side, "E")
	assert_almost_eq(port.pos_u, 0.2, 0.001)
	assert_eq(port.role, HudRole.BLEND)
	assert_eq(port.priority, 90)
	assert_eq(port.style, "ARC")
	assert_eq(port.cap, "ENDCAP")
	assert_eq(port.capacity, 2)


func test_create_defaults():
	var port := HudPort.create(
		"test.port",
		"test",
		"N",
		0.5,
		HudRole.NAV,
		50,
	)
	assert_eq(port.style, "ARC")
	assert_eq(port.cap, "NONE")
	assert_eq(port.capacity, 1)


func test_dir_for_side_north():
	assert_eq(HudPort.dir_for_side("N"), Vector2(0, -1))


func test_dir_for_side_east():
	assert_eq(HudPort.dir_for_side("E"), Vector2(1, 0))


func test_dir_for_side_south():
	assert_eq(HudPort.dir_for_side("S"), Vector2(0, 1))


func test_dir_for_side_west():
	assert_eq(HudPort.dir_for_side("W"), Vector2(-1, 0))


func test_dir_for_side_unknown():
	assert_eq(HudPort.dir_for_side("X"), Vector2.ZERO)
	assert_eq(HudPort.dir_for_side(""), Vector2.ZERO)
