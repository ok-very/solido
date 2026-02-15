class_name Chip
extends Control

@export var role: int = 0
@export var label_text: String = ""
@export var interactive: bool = true
@export var toggled: bool = false
@export var radius_u: float = 1.0
@export var padding_u: float = 0.5

signal pressed
signal toggled_changed(is_on: bool)
signal long_pressed

var _chip_body: Panel
var _chip_label: Label
var _long_press_timer: Timer
var _rim_pulse_timer: Timer
var _is_pressing: bool = false
var _press_position: Vector2
var _long_press_fired: bool = false


func _ready() -> void:
	_chip_body = $ChipBody
	_chip_label = $ChipLabel
	_long_press_timer = $LongPressTimer
	_rim_pulse_timer = $RimPulseTimer
	_long_press_timer.timeout.connect(_on_long_press_timeout)
	_rim_pulse_timer.timeout.connect(_on_rim_pulse_timeout)
	var unit_px: float = HudTheme.unit_px
	var min_touch := HudTheme.tokens.touch_min_u * unit_px
	custom_minimum_size = Vector2(min_touch, min_touch)
	_chip_body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_chip_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_chip_label.theme_type_variation = "HudChipLabel"
	_chip_label.uppercase = true
	_chip_label.text = label_text
	if interactive:
		mouse_filter = Control.MOUSE_FILTER_STOP
	else:
		mouse_filter = Control.MOUSE_FILTER_IGNORE
	_apply_role_style()
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func set_role(new_role: int) -> void:
	role = new_role
	_apply_role_style()
	var text_color = HudTheme.get_text_on_solid_color(role)
	_chip_label.add_theme_color_override("font_color", text_color)


func set_label(text: String) -> void:
	label_text = text
	if _chip_label:
		_chip_label.text = text


func set_toggled(state: bool) -> void:
	toggled = state
	_apply_role_style()


func get_toggled() -> bool:
	return toggled


func _gui_input(event: InputEvent) -> void:
	if not interactive:
		return
	if event is InputEventMouseButton:
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			_is_pressing = true
			_press_position = event.position
			_long_press_fired = false
			_apply_active_style()
			_rim_pulse_timer.start()
			_long_press_timer.start()
		elif not event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if _is_pressing:
				_long_press_timer.stop()
				_rim_pulse_timer.stop()
				if not _long_press_fired:
					pressed.emit()
					toggled = not toggled
					toggled_changed.emit(toggled)
				_is_pressing = false
				_apply_role_style()
	elif event is InputEventMouseMotion and _is_pressing:
		if _press_position.distance_to(event.position) > HudTheme.unit_px:
			_long_press_timer.stop()
			_rim_pulse_timer.stop()
			_is_pressing = false
			_apply_role_style()


func _apply_role_style() -> void:
	if not _chip_body:
		return
	var unit_px: float = HudTheme.unit_px
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudTheme.palette.get_solid(role)
	var r := int(radius_u * unit_px)
	sb.corner_radius_top_left = r
	sb.corner_radius_top_right = r
	sb.corner_radius_bottom_left = r
	sb.corner_radius_bottom_right = r
	var pad := int(padding_u * unit_px)
	sb.content_margin_left = pad
	sb.content_margin_right = pad
	sb.content_margin_top = pad
	sb.content_margin_bottom = pad
	if toggled:
		sb.border_width_top = 2
		sb.border_width_bottom = 2
		sb.border_width_left = 2
		sb.border_width_right = 2
		sb.border_color = HudTheme.palette.get_rim(role)
	else:
		sb.border_width_top = 1
		sb.border_width_bottom = 1
		sb.border_width_left = 1
		sb.border_width_right = 1
		sb.border_color = HudTheme.palette.get_rim(role)
		sb.border_color.a = 0.3
	_chip_body.add_theme_stylebox_override("panel", sb)


func _apply_active_style() -> void:
	if not _chip_body:
		return
	var sb: StyleBoxFlat = _chip_body.get_theme_stylebox("panel")
	if sb:
		sb = sb.duplicate()
		sb.bg_color = HudTheme.palette.get_solid(role).darkened(0.2)
		sb.border_width_top = 3
		sb.border_width_bottom = 3
		sb.border_width_left = 3
		sb.border_width_right = 3
		sb.border_color = HudTheme.palette.get_rim(role)
		_chip_body.add_theme_stylebox_override("panel", sb)


func _on_long_press_timeout() -> void:
	if _is_pressing:
		_long_press_fired = true
		long_pressed.emit()
		if _chip_body:
			var sb: StyleBoxFlat = _chip_body.get_theme_stylebox("panel")
			if sb:
				sb = sb.duplicate()
				sb.border_color = HudTheme.palette.get_rim(HudRole.ALERT)
				_chip_body.add_theme_stylebox_override("panel", sb)


func _on_rim_pulse_timeout() -> void:
	if _is_pressing and _chip_body:
		var sb: StyleBoxFlat = _chip_body.get_theme_stylebox("panel")
		if sb:
			sb = sb.duplicate()
			sb.border_color = HudTheme.palette.get_rim(role)
			sb.border_color.a = 1.0
			_chip_body.add_theme_stylebox_override("panel", sb)


func _on_palette_changed() -> void:
	_apply_role_style()
	if _chip_label:
		_chip_label.add_theme_color_override("font_color", HudTheme.get_text_on_solid_color(role))
