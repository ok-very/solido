extends GutTest

# Tests for tool_dock.gd signal wiring and cleanup
# Strategy: create dock instance without adding to tree (skip _ready),
# manually set ui_builder + parameters_container, exercise signal methods.

var dock: Control
var ui_builder: RefCounted


func before_each():
	dock = load("res://addons/procedural_tools/tool_dock.gd").new()
	autofree(dock)
	ui_builder = load("res://addons/procedural_tools/ui_builder.gd").new()
	dock.ui_builder = ui_builder
	dock.parameters_container = VBoxContainer.new()
	add_child_autofree(dock.parameters_container)

# --- Helper ---


func _build_and_connect(param_name: String, param_def: Dictionary) -> Control:
	var ctrl = ui_builder.create_control(param_name, param_def)
	if ctrl:
		add_child_autofree(ctrl)
		dock._connect_parameter_change(ctrl, param_name)
	return ctrl

# --- Signal connection per type ---


func test_int_connects_signal():
	_build_and_connect("width", { "type": "int", "default": 10, "min": 0, "max": 100 })
	assert_true(dock._connected_signals.size() > 0, "int should connect a signal")


func test_float_connects_signal():
	_build_and_connect("scale", { "type": "float", "default": 0.5, "min": 0.0, "max": 1.0 })
	assert_true(dock._connected_signals.size() > 0, "float should connect a signal")


func test_bool_connects_signal():
	_build_and_connect("enabled", { "type": "bool", "default": true })
	assert_true(dock._connected_signals.size() > 0, "bool should connect a signal")


func test_string_connects_signal():
	_build_and_connect("name", { "type": "string", "default": "hello" })
	assert_true(dock._connected_signals.size() > 0, "string should connect a signal")


func test_enum_connects_signal():
	_build_and_connect("mode", { "type": "enum", "options": ["a", "b"], "default": "a" })
	assert_true(dock._connected_signals.size() > 0, "enum should connect a signal")


func test_color_connects_signal():
	_build_and_connect("tint", { "type": "color", "default": [1.0, 0.0, 0.0, 1.0] })
	assert_true(dock._connected_signals.size() > 0, "color should connect a signal")


func test_vector2_connects_signal():
	_build_and_connect("offset", { "type": "vector2", "default": [1.0, 2.0] })
	assert_true(dock._connected_signals.size() > 0, "vector2 should connect a signal")


func test_vector3_connects_signal():
	_build_and_connect("pos", { "type": "vector3", "default": [1.0, 2.0, 3.0] })
	assert_true(dock._connected_signals.size() > 0, "vector3 should connect a signal")

# --- Signal type verification ---


func test_bool_connects_toggled():
	_build_and_connect("enabled", { "type": "bool", "default": false })
	assert_eq(dock._connected_signals[0].signal_name, "toggled")


func test_int_connects_spinbox():
	_build_and_connect("count", { "type": "int", "default": 5, "min": 0, "max": 100 })
	assert_is(dock._connected_signals[0].obj, SpinBox)


func test_float_connects_value_changed():
	_build_and_connect("scale", { "type": "float", "default": 0.5, "min": 0.0, "max": 1.0 })
	assert_eq(dock._connected_signals[0].signal_name, "value_changed")
	# Float connects the Slider (not SpinBox) — only 1 signal
	assert_eq(dock._connected_signals.size(), 1)


func test_vector2_connects_two_spinboxes():
	_build_and_connect("offset", { "type": "vector2", "default": [1.0, 2.0] })
	assert_eq(dock._connected_signals.size(), 2, "vector2 should connect 2 SpinBoxes")


func test_vector3_connects_three_spinboxes():
	_build_and_connect("pos", { "type": "vector3", "default": [1.0, 2.0, 3.0] })
	assert_eq(dock._connected_signals.size(), 3, "vector3 should connect 3 SpinBoxes")

# --- Cleanup ---


func test_cleanup_disconnects_signals():
	_build_and_connect("width", { "type": "int", "default": 10, "min": 0, "max": 100 })
	assert_true(dock._connected_signals.size() > 0)
	dock._cleanup_parameters()
	assert_eq(dock._connected_signals.size(), 0)


func test_cleanup_clears_parameter_controls():
	var dummy = Control.new()
	autofree(dummy)
	dock.parameter_controls["width"] = dummy
	dock._cleanup_parameters()
	assert_true(dock.parameter_controls.is_empty())


func test_double_cleanup_is_safe():
	_build_and_connect("width", { "type": "int", "default": 10, "min": 0, "max": 100 })
	dock._cleanup_parameters()
	dock._cleanup_parameters()
	assert_eq(dock._connected_signals.size(), 0)
