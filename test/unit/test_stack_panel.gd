extends GutTest

var _scene := preload("res://hud/composed/stack_panel.tscn")


func _create_panel() -> Control:
	var panel := _scene.instantiate()
	panel.size = Vector2(720, 1800)
	add_child_autofree(panel)
	return panel


func test_instantiates():
	var panel := _create_panel()
	assert_not_null(panel)
	assert_true(panel is Control)


func test_has_elbow():
	var panel := _create_panel()
	var elbow: LcarsElbow = panel.get_node("StackElbow")
	assert_not_null(elbow)
	assert_eq(elbow.rotation_index, LcarsElbow.TL)
	assert_eq(elbow.role, HudRole.NAV)
	assert_eq(elbow.outer_radius_u, 4.0)
	assert_eq(elbow.inner_radius_u, 2.0)


func test_has_rail():
	var panel := _create_panel()
	var rail: LcarsRail = panel.get_node("StackRail")
	assert_not_null(rail)
	assert_eq(rail.orientation, 1)
	assert_eq(rail.role, HudRole.NAV)
	assert_eq(rail.thickness_u, 3.0)
	assert_eq(rail.segment_count, 3)
	assert_eq(rail.segment_ratios.size(), 3)
	assert_eq(rail.corner_radius_u, 0.0)


func test_has_endcap():
	var panel := _create_panel()
	var endcap: LcarsEndcap = panel.get_node("StackEndcap")
	assert_not_null(endcap)
	assert_eq(endcap.style, LcarsEndcap.HALF_PILL)
	assert_eq(endcap.direction, LcarsEndcap.DOWN)
	assert_eq(endcap.role, HudRole.NAV)
	assert_eq(endcap.thickness_u, 3.0)


func test_has_chip_bar():
	var panel := _create_panel()
	var chip_bar: HBoxContainer = panel.get_node("StackContentArea/StackChipBar")
	assert_not_null(chip_bar)
	assert_eq(chip_bar.get_child_count(), 3)


func test_chip_labels():
	var panel := _create_panel()
	var chip_bar: HBoxContainer = panel.get_node("StackContentArea/StackChipBar")
	var labels: Array[String] = []
	for i in chip_bar.get_child_count():
		var child := chip_bar.get_child(i)
		if child is Chip:
			labels.append(child.label_text)
	assert_eq(labels, ["LAYERS", "NODES", "HISTORY"])


func test_chip_roles():
	var panel := _create_panel()
	var chip_bar: HBoxContainer = panel.get_node("StackContentArea/StackChipBar")
	for i in chip_bar.get_child_count():
		var child := chip_bar.get_child(i)
		if child is Chip:
			assert_eq(child.role, HudRole.NAV)


func test_default_segment_is_zero():
	var panel := _create_panel()
	assert_eq(panel.get_active_segment(), 0)


func test_select_segment_updates_chips():
	var panel := _create_panel()
	panel.select_segment(2)
	assert_eq(panel.get_active_segment(), 2)
	var chip_bar: HBoxContainer = panel.get_node("StackContentArea/StackChipBar")
	var chip_0: Chip = chip_bar.get_child(0)
	var chip_2: Chip = chip_bar.get_child(2)
	assert_false(chip_0.get_toggled())
	assert_true(chip_2.get_toggled())


func test_select_segment_emits_signal():
	var panel := _create_panel()
	watch_signals(panel)
	panel.select_segment(1)
	assert_signal_emitted(panel, "segment_selected")


func test_select_out_of_range_ignored():
	var panel := _create_panel()
	panel.select_segment(99)
	assert_eq(panel.get_active_segment(), 0)
	panel.select_segment(-1)
	assert_eq(panel.get_active_segment(), 0)


func test_port_positions_keys():
	var panel := _create_panel()
	var ports: Dictionary = panel.get_port_positions()
	assert_true(ports.has("stack.active"))
	assert_true(ports.has("stack.selection"))
	assert_true(ports.has("stack.telemetry"))


func test_port_position_single():
	var panel := _create_panel()
	var pos: Vector2 = panel.get_port_position("stack.active")
	assert_true(pos is Vector2)


func test_port_position_missing_returns_global():
	var panel := _create_panel()
	var pos: Vector2 = panel.get_port_position("nonexistent")
	assert_eq(pos, panel.global_position)


func test_anti_metro_varied_radii():
	var panel := _create_panel()
	var elbow: LcarsElbow = panel.get_node("StackElbow")
	assert_eq(elbow.outer_radius_u, 4.0)
	assert_eq(elbow.inner_radius_u, 2.0)


func test_anti_metro_endcap_style():
	var panel := _create_panel()
	var endcap: LcarsEndcap = panel.get_node("StackEndcap")
	assert_eq(endcap.style, LcarsEndcap.HALF_PILL)


func test_touch_target_rail():
	var panel := _create_panel()
	var rail: LcarsRail = panel.get_node("StackRail")
	var min_touch := HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(rail.custom_minimum_size.x >= min_touch)


# === Stack List ===

func test_has_stack_list():
	var panel := _create_panel()
	var stack_list: VBoxContainer = panel.get_node("StackContentArea/StackList")
	assert_not_null(stack_list)
	assert_true(stack_list is VBoxContainer)


func test_stack_list_has_placeholder_items():
	var panel := _create_panel()
	var stack_list: VBoxContainer = panel.get_node("StackContentArea/StackList")
	var label_count := 0
	for i in stack_list.get_child_count():
		var child := stack_list.get_child(i)
		if child is Label:
			label_count += 1
	assert_true(label_count >= 3)


func test_stack_footer_bracket_exists():
	var panel := _create_panel()
	var bracket: Bracket = panel.get_node("StackContentArea/StackList/StackFooterBracket")
	assert_not_null(bracket)
	assert_eq(bracket.style, Bracket.TICK_GROUP)


func test_stack_footer_bracket_orientation():
	var panel := _create_panel()
	var bracket: Bracket = panel.get_node("StackContentArea/StackList/StackFooterBracket")
	assert_eq(bracket.orientation, Bracket.BOTTOM)


func test_stack_footer_bracket_role():
	var panel := _create_panel()
	var bracket: Bracket = panel.get_node("StackContentArea/StackList/StackFooterBracket")
	assert_eq(bracket.role, HudRole.NAV)


func test_stack_footer_bracket_tick_count():
	var panel := _create_panel()
	var bracket: Bracket = panel.get_node("StackContentArea/StackList/StackFooterBracket")
	assert_eq(bracket.tick_count, 5)
