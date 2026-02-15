extends GutTest

var _scene := preload("res://hud/composed/collapsible_section.tscn")


func _create_section(p_name: String = "TEST", p_role: int = HudRole.NEUTRAL) -> CollapsibleSection:
	var section := _scene.instantiate()
	section.section_name = p_name
	section.role = p_role
	section.size = Vector2(400, 300)
	add_child_autofree(section)
	return section


func test_instantiates():
	var section := _create_section()
	assert_not_null(section)
	assert_true(section is Control)


func test_has_section_header():
	var section := _create_section()
	var header: HBoxContainer = section.get_node("SectionHeader")
	assert_not_null(header)


func test_has_header_chip():
	var section := _create_section()
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	assert_not_null(chip)


func test_has_header_bracket():
	var section := _create_section()
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_not_null(bracket)


func test_has_section_content():
	var section := _create_section()
	var content: VBoxContainer = section.get_node("SectionContent")
	assert_not_null(content)


func test_chip_label_matches_section_name():
	var section := _create_section("TRANSFORM")
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	assert_eq(chip.label_text, "TRANSFORM")


func test_chip_role_matches_exported_role():
	var section := _create_section("TEST", HudRole.EDIT)
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	assert_eq(chip.role, HudRole.EDIT)


func test_bracket_style_matches_export():
	var section := _scene.instantiate()
	section.section_name = "TEST"
	section.bracket_style = Bracket.TICK_GROUP
	section.size = Vector2(400, 300)
	add_child_autofree(section)
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_eq(bracket.style, Bracket.TICK_GROUP)


func test_bracket_orientation_matches_export():
	var section := _scene.instantiate()
	section.section_name = "TEST"
	section.bracket_orientation = Bracket.LEFT
	section.size = Vector2(400, 300)
	add_child_autofree(section)
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_eq(bracket.orientation, Bracket.LEFT)


func test_bracket_role_matches_section_role():
	var section := _create_section("TEST", HudRole.BLEND)
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_eq(bracket.role, HudRole.BLEND)


func test_bracket_tick_count():
	var section := _scene.instantiate()
	section.section_name = "TEST"
	section.tick_count = 5
	section.size = Vector2(400, 300)
	add_child_autofree(section)
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_eq(bracket.tick_count, 5)


func test_initial_state_expanded():
	var section := _create_section()
	assert_false(section.collapsed)
	var content: VBoxContainer = section.get_node("SectionContent")
	assert_true(content.visible)


func test_initial_state_collapsed():
	var section := _scene.instantiate()
	section.section_name = "TEST"
	section.collapsed = true
	section.size = Vector2(400, 300)
	add_child_autofree(section)
	assert_true(section.collapsed)
	var content: VBoxContainer = section.get_node("SectionContent")
	assert_false(content.visible)


func test_toggle_emits_collapsed_changed():
	var section := _create_section()
	watch_signals(section)
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	chip.pressed.emit()
	assert_signal_emitted(section, "collapsed_changed")


func test_toggle_flips_content_visibility():
	var section := _create_section()
	var content: VBoxContainer = section.get_node("SectionContent")
	assert_true(content.visible)
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	chip.pressed.emit()
	assert_false(content.visible)


func test_programmatic_set_collapsed():
	var section := _create_section()
	section.set_collapsed(true)
	assert_true(section.collapsed)
	var content: VBoxContainer = section.get_node("SectionContent")
	assert_false(content.visible)


func test_programmatic_set_collapsed_emits_signal():
	var section := _create_section()
	watch_signals(section)
	section.set_collapsed(true)
	assert_signal_emitted(section, "collapsed_changed")


func test_get_content_returns_vbox():
	var section := _create_section()
	var content := section.get_content()
	assert_not_null(content)
	assert_true(content is VBoxContainer)


func test_multiple_toggles():
	var section := _create_section()
	var content: VBoxContainer = section.get_node("SectionContent")
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	chip.pressed.emit()
	assert_false(content.visible)
	chip.pressed.emit()
	assert_true(content.visible)
	chip.pressed.emit()
	assert_false(content.visible)


func test_bracket_mouse_filter_ignore():
	var section := _create_section()
	var bracket: Bracket = section.get_node("SectionHeader/HeaderBracket")
	assert_eq(bracket.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_chip_is_interactive():
	var section := _create_section()
	var chip: Chip = section.get_node("SectionHeader/HeaderChip")
	assert_true(chip.interactive)
