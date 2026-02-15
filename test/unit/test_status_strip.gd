extends GutTest

var _scene := preload("res://hud/composed/status_strip.tscn")


func _create_strip() -> Control:
	var strip := _scene.instantiate()
	strip.size = Vector2(3240, 192)
	add_child_autofree(strip)
	return strip


func test_instantiates():
	var strip := _create_strip()
	assert_not_null(strip)
	assert_true(strip is Control)


func test_has_rail():
	var strip := _create_strip()
	var rail: LcarsRail = strip.get_node("StatusRail")
	assert_not_null(rail)
	assert_eq(rail.orientation, 0)
	assert_eq(rail.role, HudRole.TELEMETRY)
	assert_eq(rail.thickness_u, 2.0)
	assert_eq(rail.segment_count, 4)
	assert_eq(rail.segment_ratios.size(), 4)
	assert_eq(rail.corner_radius_u, 0.25)


func test_has_endcap_left():
	var strip := _create_strip()
	var endcap: LcarsEndcap = strip.get_node("StatusEndcapLeft")
	assert_not_null(endcap)
	assert_eq(endcap.style, LcarsEndcap.HALF_PILL)
	assert_eq(endcap.direction, LcarsEndcap.LEFT)
	assert_eq(endcap.role, HudRole.TELEMETRY)
	assert_eq(endcap.thickness_u, 2.0)


func test_has_endcap_right():
	var strip := _create_strip()
	var endcap: LcarsEndcap = strip.get_node("StatusEndcapRight")
	assert_not_null(endcap)
	assert_eq(endcap.style, LcarsEndcap.STEPPED)
	assert_eq(endcap.direction, LcarsEndcap.RIGHT)
	assert_eq(endcap.role, HudRole.TELEMETRY)
	assert_eq(endcap.step_depth_u, 0.75)
	assert_eq(endcap.step_offset_u, 0.75)


func test_asymmetric_endcaps():
	var strip := _create_strip()
	var left: LcarsEndcap = strip.get_node("StatusEndcapLeft")
	var right: LcarsEndcap = strip.get_node("StatusEndcapRight")
	assert_ne(left.style, right.style)


func test_has_mode_chips():
	var strip := _create_strip()
	var chips_container: HBoxContainer = strip.get_node("StatusContentArea/StatusModeChips")
	assert_not_null(chips_container)
	assert_eq(chips_container.get_child_count(), 4)


func test_mode_chip_labels():
	var strip := _create_strip()
	var chips_container: HBoxContainer = strip.get_node("StatusContentArea/StatusModeChips")
	var labels: Array[String] = []
	for i in chips_container.get_child_count():
		var child := chips_container.get_child(i)
		if child is Chip:
			labels.append(child.label_text)
	assert_eq(labels, ["SELECT", "MOVE", "ROTATE", "SCULPT"])


func test_mode_chip_roles_varied():
	var strip := _create_strip()
	var chips_container: HBoxContainer = strip.get_node("StatusContentArea/StatusModeChips")
	var roles: Array[int] = []
	for i in chips_container.get_child_count():
		var child := chips_container.get_child(i)
		if child is Chip:
			roles.append(child.role)
	assert_eq(roles[0], HudRole.NAV)
	assert_eq(roles[1], HudRole.EDIT)
	assert_eq(roles[2], HudRole.EDIT)
	assert_eq(roles[3], HudRole.BLEND)


func test_default_mode_is_zero():
	var strip := _create_strip()
	assert_eq(strip.get_active_mode(), 0)


func test_select_mode_updates_chips():
	var strip := _create_strip()
	strip.select_mode(2)
	assert_eq(strip.get_active_mode(), 2)
	var chips_container: HBoxContainer = strip.get_node("StatusContentArea/StatusModeChips")
	var chip_0: Chip = chips_container.get_child(0)
	var chip_2: Chip = chips_container.get_child(2)
	assert_false(chip_0.get_toggled())
	assert_true(chip_2.get_toggled())


func test_select_mode_out_of_range_ignored():
	var strip := _create_strip()
	strip.select_mode(99)
	assert_eq(strip.get_active_mode(), 0)


func test_rail_segment_pressed_emits():
	var strip := _create_strip()
	watch_signals(strip)
	strip.get_node("StatusRail").segment_pressed.emit(1)
	assert_signal_emitted(strip, "segment_selected")


func test_port_positions_keys():
	var strip := _create_strip()
	var ports: Dictionary = strip.get_port_positions()
	assert_true(ports.has("status.mode"))
	assert_true(ports.has("status.selection"))
	assert_true(ports.has("status.perf"))


func test_port_position_single():
	var strip := _create_strip()
	var pos: Vector2 = strip.get_port_position("status.mode")
	assert_true(pos is Vector2)


func test_port_position_missing_returns_global():
	var strip := _create_strip()
	var pos: Vector2 = strip.get_port_position("nonexistent")
	assert_eq(pos, strip.global_position)


func test_touch_target_rail_compensation():
	var strip := _create_strip()
	var rail: LcarsRail = strip.get_node("StatusRail")
	var min_touch := HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(rail.custom_minimum_size.y >= min_touch)


func test_thinner_rail_than_side_panels():
	var strip := _create_strip()
	var rail: LcarsRail = strip.get_node("StatusRail")
	assert_eq(rail.thickness_u, 2.0)
	assert_true(rail.thickness_u < 3.0)
