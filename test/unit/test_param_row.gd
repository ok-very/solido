extends GutTest

var _scene := preload("res://hud/composed/param_row.tscn")


func _create_row(p_name: String = "Test", p_value: String = "0.0", p_role: int = HudRole.NEUTRAL) -> ParamRow:
	var row := _scene.instantiate()
	row.param_name = p_name
	row.value_text = p_value
	row.role = p_role
	add_child_autofree(row)
	return row


func test_instantiates():
	var row := _create_row()
	assert_not_null(row)
	assert_true(row is HBoxContainer)


func test_has_label_child():
	var row := _create_row()
	var label: Label = row.get_node("ParamLabel")
	assert_not_null(label)


func test_has_value_child():
	var row := _create_row()
	var value: Label = row.get_node("ParamValue")
	assert_not_null(value)


func test_label_text_matches_param_name():
	var row := _create_row("Position X")
	var label: Label = row.get_node("ParamLabel")
	assert_eq(label.text, "Position X")


func test_value_text_matches():
	var row := _create_row("Test", "42.0")
	var value: Label = row.get_node("ParamValue")
	assert_eq(value.text, "42.0")


func test_minimum_height():
	var row := _create_row()
	var min_h := HudTheme.tokens.touch_min_u * HudTheme.unit_px
	assert_true(row.custom_minimum_size.y >= min_h)


func test_label_stretch_ratio():
	var row := _create_row()
	var label: Label = row.get_node("ParamLabel")
	assert_almost_eq(label.size_flags_stretch_ratio, 0.4, 0.01)


func test_value_stretch_ratio():
	var row := _create_row()
	var value: Label = row.get_node("ParamValue")
	assert_almost_eq(value.size_flags_stretch_ratio, 0.6, 0.01)


func test_set_param_name_updates_label():
	var row := _create_row("Old")
	row.set_param_name("New")
	assert_eq(row.param_name, "New")
	var label: Label = row.get_node("ParamLabel")
	assert_eq(label.text, "New")


func test_set_value_text_updates_value():
	var row := _create_row("Test", "0.0")
	row.set_value_text("99.9")
	assert_eq(row.value_text, "99.9")
	var value: Label = row.get_node("ParamValue")
	assert_eq(value.text, "99.9")


func test_label_theme_variation():
	var row := _create_row()
	var label: Label = row.get_node("ParamLabel")
	assert_eq(label.theme_type_variation, &"HudLabel")


func test_value_theme_variation():
	var row := _create_row()
	var value: Label = row.get_node("ParamValue")
	assert_eq(value.theme_type_variation, &"HudValue")
