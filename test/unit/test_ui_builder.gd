extends GutTest

# Tests for addons/procedural_tools/ui_builder.gd

var builder: RefCounted


func before_each():
	builder = load("res://addons/procedural_tools/ui_builder.gd").new()

# --- Helper: add control to tree so children are accessible ---


func _build(param_name: String, param_def: Dictionary) -> Control:
	var ctrl = builder.create_control(param_name, param_def)
	if ctrl:
		add_child_autofree(ctrl)
	return ctrl

# --- Type Dispatch ---


func test_int_creates_vbox():
	var ctrl = _build("width", { "type": "int", "default": 10, "min": 0, "max": 100 })
	assert_is(ctrl, VBoxContainer)


func test_float_creates_vbox():
	var ctrl = _build("scale", { "type": "float", "default": 0.5 })
	assert_is(ctrl, VBoxContainer)


func test_bool_creates_checkbox():
	var ctrl = _build("enabled", { "type": "bool", "default": true })
	assert_is(ctrl, CheckBox)


func test_string_creates_vbox():
	var ctrl = _build("name", { "type": "string", "default": "hello" })
	assert_is(ctrl, VBoxContainer)


func test_enum_creates_vbox():
	var ctrl = _build("mode", { "type": "enum", "options": ["a", "b"], "default": "a" })
	assert_is(ctrl, VBoxContainer)


func test_color_creates_vbox():
	var ctrl = _build("tint", { "type": "color", "default": [1.0, 0.0, 0.0, 1.0] })
	assert_is(ctrl, VBoxContainer)


func test_vector2_creates_vbox():
	var ctrl = _build("offset", { "type": "vector2", "default": [1.0, 2.0] })
	assert_is(ctrl, VBoxContainer)


func test_vector3_creates_vbox():
	var ctrl = _build("position", { "type": "vector3", "default": [1.0, 2.0, 3.0] })
	assert_is(ctrl, VBoxContainer)


func test_unknown_type_returns_null():
	var ctrl = builder.create_control("bad", { "type": "quaternion" })
	assert_null(ctrl)
	assert_push_error("Unknown type")


func test_missing_type_defaults_to_float():
	var ctrl = _build("value", { "default": 0.5 })
	assert_not_null(ctrl)
	# Default type is "float", so should create float control (VBox with slider)
	assert_is(ctrl, VBoxContainer)

# --- Int Control ---


func test_int_has_label():
	var ctrl = _build("width", { "type": "int", "default": 10 })
	var label = ctrl.get_child(0)
	assert_is(label, Label)
	assert_eq(label.text, "Width")


func test_int_has_spinbox():
	var ctrl = _build("width", { "type": "int", "default": 10, "min": 2, "max": 50 })
	var spinbox = ctrl.get_child(1)
	assert_is(spinbox, SpinBox)
	assert_eq(spinbox.value, 10.0)
	assert_eq(spinbox.min_value, 2.0)
	assert_eq(spinbox.max_value, 50.0)
	assert_eq(spinbox.step, 1.0)


func test_int_default_range():
	var ctrl = _build("count", { "type": "int" })
	var spinbox = ctrl.get_child(1)
	assert_eq(spinbox.min_value, 0.0)
	assert_eq(spinbox.max_value, 100.0)
	assert_eq(spinbox.value, 0.0)

# --- Float Control ---


func test_float_has_slider_and_spinbox():
	var ctrl = _build("scale", { "type": "float", "default": 0.5, "min": 0.0, "max": 1.0, "step": 0.1 })
	# VBox > Label, HBox > (Slider, SpinBox)
	var hbox = ctrl.get_child(1)
	assert_is(hbox, HBoxContainer)
	var slider = hbox.get_child(0)
	var spinbox = hbox.get_child(1)
	assert_is(slider, HSlider)
	assert_is(spinbox, SpinBox)


func test_float_values():
	var ctrl = _build("scale", { "type": "float", "default": 0.7, "min": 0.0, "max": 2.0, "step": 0.05 })
	var hbox = ctrl.get_child(1)
	var slider = hbox.get_child(0)
	assert_almost_eq(slider.value, 0.7, 0.001)
	assert_eq(slider.min_value, 0.0)
	assert_eq(slider.max_value, 2.0)
	assert_eq(slider.step, 0.05)


func test_float_spinbox_synced_to_slider():
	var ctrl = _build("x", { "type": "float", "default": 0.5, "min": 0.0, "max": 1.0, "step": 0.01 })
	var hbox = ctrl.get_child(1)
	var slider = hbox.get_child(0)
	var spinbox = hbox.get_child(1)
	assert_eq(slider.value, spinbox.value)
	assert_eq(slider.min_value, spinbox.min_value)
	assert_eq(slider.max_value, spinbox.max_value)

# --- Bool Control ---


func test_bool_default_true():
	var ctrl = _build("enabled", { "type": "bool", "default": true })
	assert_true(ctrl.button_pressed)


func test_bool_default_false():
	var ctrl = _build("enabled", { "type": "bool", "default": false })
	assert_false(ctrl.button_pressed)


func test_bool_label():
	var ctrl = _build("use_normals", { "type": "bool", "default": false })
	assert_eq(ctrl.text, "Use Normals")

# --- String Control ---


func test_string_has_lineedit():
	var ctrl = _build("title", { "type": "string", "default": "hello" })
	var lineedit = ctrl.get_child(1)
	assert_is(lineedit, LineEdit)
	assert_eq(lineedit.text, "hello")


func test_string_empty_default():
	var ctrl = _build("name", { "type": "string" })
	var lineedit = ctrl.get_child(1)
	assert_eq(lineedit.text, "")

# --- Enum Control ---


func test_enum_has_option_button():
	var ctrl = _build("terrain_type", { "type": "enum", "options": ["plains", "hills", "mountains"], "default": "hills" })
	var option_btn = ctrl.get_child(1)
	assert_is(option_btn, OptionButton)


func test_enum_items():
	var ctrl = _build("mode", { "type": "enum", "options": ["a", "b", "c"], "default": "a" })
	var option_btn = ctrl.get_child(1)
	assert_eq(option_btn.item_count, 3)
	assert_eq(option_btn.get_item_text(0), "a")
	assert_eq(option_btn.get_item_text(1), "b")
	assert_eq(option_btn.get_item_text(2), "c")


func test_enum_default_selection():
	var ctrl = _build("mode", { "type": "enum", "options": ["x", "y", "z"], "default": "y" })
	var option_btn = ctrl.get_child(1)
	assert_eq(option_btn.selected, 1)


func test_enum_no_options():
	var ctrl = _build("mode", { "type": "enum" })
	var option_btn = ctrl.get_child(1)
	assert_eq(option_btn.item_count, 0)

# --- Color Control ---


func test_color_has_picker():
	var ctrl = _build("tint", { "type": "color", "default": [1.0, 0.0, 0.0, 1.0] })
	var picker = ctrl.get_child(1)
	assert_is(picker, ColorPickerButton)


func test_color_value():
	var ctrl = _build("tint", { "type": "color", "default": [0.5, 0.6, 0.7, 1.0] })
	var picker = ctrl.get_child(1)
	assert_almost_eq(picker.color.r, 0.5, 0.01)
	assert_almost_eq(picker.color.g, 0.6, 0.01)
	assert_almost_eq(picker.color.b, 0.7, 0.01)
	assert_almost_eq(picker.color.a, 1.0, 0.01)


func test_color_rgb_only():
	var ctrl = _build("tint", { "type": "color", "default": [0.2, 0.3, 0.4] })
	var picker = ctrl.get_child(1)
	assert_almost_eq(picker.color.a, 1.0, 0.01)

# --- Vector2 Control ---


func test_vector2_has_two_spinboxes():
	var ctrl = _build("offset", { "type": "vector2", "default": [1.0, 2.0] })
	var hbox = ctrl.get_child(1)
	var spinboxes = []
	for child in hbox.get_children():
		if child is SpinBox:
			spinboxes.append(child)
	assert_eq(spinboxes.size(), 2)


func test_vector2_values():
	var ctrl = _build("offset", { "type": "vector2", "default": [3.0, 7.0] })
	var hbox = ctrl.get_child(1)
	var spinboxes = []
	for child in hbox.get_children():
		if child is SpinBox:
			spinboxes.append(child)
	assert_eq(spinboxes[0].value, 3.0)
	assert_eq(spinboxes[1].value, 7.0)


func test_vector2_axis_labels():
	var ctrl = _build("offset", { "type": "vector2", "default": [0.0, 0.0] })
	var hbox = ctrl.get_child(1)
	assert_eq(hbox.get_child(0).text, "X:")
	assert_eq(hbox.get_child(2).text, "Y:")

# --- Vector3 Control ---


func test_vector3_has_three_spinboxes():
	var ctrl = _build("pos", { "type": "vector3", "default": [1.0, 2.0, 3.0] })
	var hbox = ctrl.get_child(1)
	var spinboxes = []
	for child in hbox.get_children():
		if child is SpinBox:
			spinboxes.append(child)
	assert_eq(spinboxes.size(), 3)


func test_vector3_values():
	var ctrl = _build("pos", { "type": "vector3", "default": [4.0, 5.0, 6.0] })
	var hbox = ctrl.get_child(1)
	var spinboxes = []
	for child in hbox.get_children():
		if child is SpinBox:
			spinboxes.append(child)
	assert_eq(spinboxes[0].value, 4.0)
	assert_eq(spinboxes[1].value, 5.0)
	assert_eq(spinboxes[2].value, 6.0)

# --- Descriptions / Tooltips ---


func test_int_tooltip():
	var ctrl = _build("width", { "type": "int", "description": "Grid width" })
	assert_eq(ctrl.get_child(0).tooltip_text, "Grid width")


func test_float_tooltip():
	var ctrl = _build("scale", { "type": "float", "description": "Scale factor" })
	assert_eq(ctrl.get_child(0).tooltip_text, "Scale factor")


func test_bool_tooltip():
	var ctrl = _build("on", { "type": "bool", "description": "Toggle it" })
	assert_eq(ctrl.tooltip_text, "Toggle it")


func test_no_description_no_tooltip():
	var ctrl = _build("x", { "type": "int" })
	assert_eq(ctrl.get_child(0).tooltip_text, "")

# --- get_control_value Round-Trip ---


func test_roundtrip_int():
	var ctrl = _build("width", { "type": "int", "default": 42, "min": 0, "max": 100 })
	var val = builder.get_control_value(ctrl)
	assert_eq(val, 42.0)


func test_roundtrip_bool_true():
	var ctrl = _build("on", { "type": "bool", "default": true })
	assert_eq(builder.get_control_value(ctrl), true)


func test_roundtrip_bool_false():
	var ctrl = _build("on", { "type": "bool", "default": false })
	assert_eq(builder.get_control_value(ctrl), false)


func test_roundtrip_string():
	var ctrl = _build("name", { "type": "string", "default": "test" })
	assert_eq(builder.get_control_value(ctrl), "test")


func test_roundtrip_enum():
	var ctrl = _build("mode", { "type": "enum", "options": ["a", "b", "c"], "default": "b" })
	assert_eq(builder.get_control_value(ctrl), "b")


func test_roundtrip_vector2():
	var ctrl = _build("offset", { "type": "vector2", "default": [3.0, 7.0] })
	var val = builder.get_control_value(ctrl)
	assert_eq(val, Vector2(3.0, 7.0))


func test_roundtrip_vector3():
	var ctrl = _build("pos", { "type": "vector3", "default": [1.0, 2.0, 3.0] })
	var val = builder.get_control_value(ctrl)
	assert_eq(val, Vector3(1.0, 2.0, 3.0))


func test_roundtrip_color():
	var ctrl = _build("tint", { "type": "color", "default": [0.5, 0.6, 0.7, 1.0] })
	var val = builder.get_control_value(ctrl)
	assert_typeof(val, TYPE_COLOR)
	assert_almost_eq(val.r, 0.5, 0.01)

# --- get_control_value on null/unknown ---


func test_get_value_unrecognized_control():
	# Passing a bare Control (not a recognized input type) should return null
	var ctrl = Control.new()
	add_child_autofree(ctrl)
	assert_null(builder.get_control_value(ctrl))

# --- Pipeline Integration: Schema → Builder → get_control_value ---


func test_terrain_schema_full_pipeline():
	var parser = load("res://addons/procedural_tools/schema_parser.gd").new()
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	var params = schema["parameters"]

	for param_name in params:
		var ctrl = _build(param_name, params[param_name])
		assert_not_null(ctrl, "Control for '%s' should not be null" % param_name)
		var val = builder.get_control_value(ctrl)
		assert_not_null(val, "Value for '%s' should not be null" % param_name)
