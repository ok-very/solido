class_name ParamRow
extends HBoxContainer

@export var param_name: String = ""
@export var value_text: String = ""
@export var role: int = HudRole.NEUTRAL

var _label: Label
var _value: Label


func _ready() -> void:
	_label = $ParamLabel
	_value = $ParamValue
	_label.text = param_name
	_value.text = value_text
	_label.size_flags_stretch_ratio = 0.4
	_value.size_flags_stretch_ratio = 0.6
	var unit_px: float = HudTheme.unit_px
	custom_minimum_size.y = HudTheme.tokens.touch_min_u * unit_px
	_apply_role_colors()
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func set_param_name(text: String) -> void:
	param_name = text
	if _label:
		_label.text = text


func set_value_text(text: String) -> void:
	value_text = text
	if _value:
		_value.text = text


func _apply_role_colors() -> void:
	if not _label or not _value:
		return
	var text_color := HudTheme.palette.get_text_on_glass(role)
	_label.add_theme_color_override("font_color", text_color)
	_value.add_theme_color_override("font_color", text_color)


func _on_palette_changed() -> void:
	_apply_role_colors()
