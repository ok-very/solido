extends GutTest

var _scene := preload("res://hud/composed/inspector_panel.tscn")


func _create_panel() -> Control:
	var panel := _scene.instantiate()
	panel.size = Vector2(840, 1800)
	add_child_autofree(panel)
	return panel


func test_instantiates():
	var panel := _create_panel()
	assert_not_null(panel)
	assert_true(panel is Control)


func test_has_elbow():
	var panel := _create_panel()
	var elbow: LcarsElbow = panel.get_node("InspectorElbow")
	assert_not_null(elbow)
	assert_eq(elbow.rotation_index, LcarsElbow.TR)
	assert_eq(elbow.role, HudRole.EDIT)
	assert_eq(elbow.outer_radius_u, 3.0)
	assert_eq(elbow.inner_radius_u, 1.5)


func test_has_rail():
	var panel := _create_panel()
	var rail: LcarsRail = panel.get_node("InspectorRail")
	assert_not_null(rail)
	assert_eq(rail.orientation, 1)
	assert_eq(rail.role, HudRole.EDIT)
	assert_eq(rail.thickness_u, 2.5)
	assert_eq(rail.segment_count, 4)
	assert_eq(rail.segment_ratios.size(), 4)
	assert_eq(rail.corner_radius_u, 0.5)


func test_has_endcap():
	var panel := _create_panel()
	var endcap: LcarsEndcap = panel.get_node("InspectorEndcap")
	assert_not_null(endcap)
	assert_eq(endcap.style, LcarsEndcap.STEPPED)
	assert_eq(endcap.direction, LcarsEndcap.DOWN)
	assert_eq(endcap.role, HudRole.EDIT)
	assert_eq(endcap.thickness_u, 2.5)
	assert_eq(endcap.step_depth_u, 1.0)
	assert_eq(endcap.step_offset_u, 1.0)


func test_has_header():
	var panel := _create_panel()
	var header: HBoxContainer = panel.get_node("InspectorContentArea/InspectorHeader")
	assert_not_null(header)


func test_header_label():
	var panel := _create_panel()
	var label: Label = panel.get_node("InspectorContentArea/InspectorHeader/HeaderLabel")
	assert_not_null(label)
	assert_eq(label.text, "INSPECTOR")
	assert_eq(label.theme_type_variation, "HudSectionHeader")


func test_lock_chip():
	var panel := _create_panel()
	var chip: Chip = panel.get_node("InspectorContentArea/InspectorHeader/LockChip")
	assert_not_null(chip)
	assert_eq(chip.label_text, "LOCK")
	assert_eq(chip.role, HudRole.EDIT)
	assert_true(chip.interactive)


func test_default_segment_is_zero():
	var panel := _create_panel()
	assert_eq(panel.get_active_segment(), 0)


func test_select_segment():
	var panel := _create_panel()
	panel.select_segment(2)
	assert_eq(panel.get_active_segment(), 2)


func test_select_segment_emits_signal():
	var panel := _create_panel()
	watch_signals(panel)
	panel.select_segment(1)
	assert_signal_emitted(panel, "segment_selected")


func test_select_out_of_range_ignored():
	var panel := _create_panel()
	panel.select_segment(99)
	assert_eq(panel.get_active_segment(), 0)


func test_port_positions_keys():
	var panel := _create_panel()
	var ports: Dictionary = panel.get_port_positions()
	assert_true(ports.has("inspector.focus"))
	assert_true(ports.has("inspector.shader"))
	assert_true(ports.has("inspector.telemetry"))


func test_port_position_single():
	var panel := _create_panel()
	var pos: Vector2 = panel.get_port_position("inspector.focus")
	assert_true(pos is Vector2)


func test_port_position_missing_returns_global():
	var panel := _create_panel()
	var pos: Vector2 = panel.get_port_position("nonexistent")
	assert_eq(pos, panel.global_position)


func test_anti_metro_varied_radii():
	var panel := _create_panel()
	var elbow: LcarsElbow = panel.get_node("InspectorElbow")
	assert_eq(elbow.outer_radius_u, 3.0)
	assert_ne(elbow.outer_radius_u, 4.0)


func test_anti_metro_endcap_variety():
	var panel := _create_panel()
	var endcap: LcarsEndcap = panel.get_node("InspectorEndcap")
	assert_eq(endcap.style, LcarsEndcap.STEPPED)


func test_anti_metro_corner_radius_differs():
	var panel := _create_panel()
	var rail: LcarsRail = panel.get_node("InspectorRail")
	assert_eq(rail.corner_radius_u, 0.5)


func test_touch_target_rail_compensation():
	var panel := _create_panel()
	var rail: LcarsRail = panel.get_node("InspectorRail")
	var min_touch := HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(rail.custom_minimum_size.x >= min_touch)
