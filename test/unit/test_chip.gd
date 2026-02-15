extends GutTest

var _chip_scene = preload("res://hud/primitives/chip.tscn")


func _create_chip() -> Chip:
	var chip = _chip_scene.instantiate()
	chip.label_text = "TEST"
	chip.size = Vector2(96, 72)
	add_child_autofree(chip)
	return chip


func test_chip_instantiates():
	var chip = _create_chip()
	assert_not_null(chip)
	assert_true(chip is Control)


func test_chip_default_exports():
	var chip = _create_chip()
	assert_eq(chip.role, 0)
	assert_eq(chip.interactive, true)
	assert_eq(chip.toggled, false)
	assert_eq(chip.radius_u, 1.0)


func test_chip_label_text():
	var chip = _create_chip()
	var label = chip.get_node("ChipLabel")
	assert_not_null(label)
	assert_eq(label.text, "TEST")


func test_chip_label_uppercase():
	var chip = _create_chip()
	var label = chip.get_node("ChipLabel")
	assert_true(label.uppercase)


func test_chip_label_theme_variation():
	var chip = _create_chip()
	var label = chip.get_node("ChipLabel")
	assert_eq(label.theme_type_variation, &"HudChipLabel")


func test_chip_minimum_size():
	var chip = _create_chip()
	var min_touch = HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(chip.custom_minimum_size.x >= min_touch)
	assert_true(chip.custom_minimum_size.y >= min_touch)


func test_chip_role_change():
	var chip = _create_chip()
	chip.set_role(HudRole.BLEND)
	assert_eq(chip.role, HudRole.BLEND)


func test_chip_toggle():
	var chip = _create_chip()
	chip.set_toggled(true)
	assert_true(chip.get_toggled())
	chip.set_toggled(false)
	assert_false(chip.get_toggled())


func test_chip_set_label():
	var chip = _create_chip()
	chip.set_label("NEW")
	assert_eq(chip.label_text, "NEW")
	assert_eq(chip.get_node("ChipLabel").text, "NEW")


func test_chip_non_interactive():
	var chip = _chip_scene.instantiate()
	chip.interactive = false
	chip.size = Vector2(96, 72)
	add_child_autofree(chip)
	assert_eq(chip.mouse_filter, Control.MOUSE_FILTER_IGNORE)
