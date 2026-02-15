class_name LcarsRail
extends Control

@export var role: int = 0
@export var orientation: int = 0 # 0=HORIZONTAL, 1=VERTICAL
@export var thickness_u: float = 3.0
@export var segment_count: int = 1
@export var segment_gap_u: float = 0.25
@export var segment_ratios: Array[float] = []
@export var corner_radius_u: float = 0.0

signal port_positions_changed(ports: Dictionary)
signal segment_pressed(index: int)
signal segment_long_pressed(index: int)

var _segments: Array[Panel] = []
var _segment_container: BoxContainer
var _port_markers: Control
var _press_timer: Timer
var _active_segment: int = -1
var _press_position: Vector2


func _ready() -> void:
	_segment_container = _get_or_create_container()
	_port_markers = $PortMarkers
	_press_timer = Timer.new()
	_press_timer.one_shot = true
	_press_timer.wait_time = HudTheme.tokens.long_press_ms / 1000.0
	_press_timer.timeout.connect(_on_long_press_timeout)
	add_child(_press_timer)
	var unit_px: float = HudTheme.unit_px
	var min_touch := HudTheme.tokens.touch_min_u * unit_px
	if orientation == 0:
		custom_minimum_size.y = max(thickness_u * unit_px, min_touch)
	else:
		custom_minimum_size.x = max(thickness_u * unit_px, min_touch)
	_rebuild_segments()
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_port_positions()
		port_positions_changed.emit(get_port_positions())


func get_port_position(side: String) -> Vector2:
	return get_port_positions().get(side, global_position)


func get_port_positions() -> Dictionary:
	var r := get_global_rect()
	return {
		"N": Vector2(r.position.x + r.size.x * 0.5, r.position.y),
		"S": Vector2(r.position.x + r.size.x * 0.5, r.position.y + r.size.y),
		"E": Vector2(r.position.x + r.size.x, r.position.y + r.size.y * 0.5),
		"W": Vector2(r.position.x, r.position.y + r.size.y * 0.5),
	}


func set_role(new_role: int) -> void:
	role = new_role
	_apply_role_colors()


func get_segment_rects() -> Array[Rect2]:
	var rects: Array[Rect2] = []
	for seg in _segments:
		rects.append(seg.get_global_rect())
	return rects


func get_thickness_px() -> float:
	return thickness_u * HudTheme.unit_px


func _get_or_create_container() -> BoxContainer:
	if has_node("SegmentContainer"):
		return $SegmentContainer
	return null


func _rebuild_segments() -> void:
	if not _segment_container:
		return
	for child in _segment_container.get_children():
		child.queue_free()
	_segments.clear()
	var unit_px: float = HudTheme.unit_px
	var gap_px := int(segment_gap_u * unit_px)
	_segment_container.add_theme_constant_override("separation", gap_px)
	var count: int = max(segment_count, 1)
	var ratios := segment_ratios.duplicate()
	if ratios.size() < count:
		ratios.resize(count)
		for i in ratios.size():
			if ratios[i] == 0.0:
				ratios[i] = 1.0
	var total_ratio := 0.0
	for r in ratios:
		total_ratio += r
	for i in count:
		var panel := Panel.new()
		var sb := StyleBoxFlat.new()
		sb.bg_color = HudTheme.palette.get_solid(role)
		var radius_px := corner_radius_u * unit_px
		sb.corner_radius_top_left = int(radius_px)
		sb.corner_radius_top_right = int(radius_px)
		sb.corner_radius_bottom_left = int(radius_px)
		sb.corner_radius_bottom_right = int(radius_px)
		panel.add_theme_stylebox_override("panel", sb)
		panel.mouse_filter = Control.MOUSE_FILTER_STOP
		if orientation == 0:
			panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			panel.size_flags_stretch_ratio = ratios[i] / total_ratio
		else:
			panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
			panel.size_flags_stretch_ratio = ratios[i] / total_ratio
		var idx: int = i
		panel.gui_input.connect(func(event: InputEvent): _on_segment_gui_input(event, idx))
		_segment_container.add_child(panel)
		_segments.append(panel)


func _apply_role_colors() -> void:
	var solid := HudTheme.palette.get_solid(role)
	for seg in _segments:
		var sb: StyleBoxFlat = seg.get_theme_stylebox("panel")
		if sb:
			sb.bg_color = solid


func _update_port_positions() -> void:
	if not _port_markers:
		return
	var r := get_rect()
	$PortMarkers/PortN.position = Vector2(r.size.x * 0.5, 0.0)
	$PortMarkers/PortS.position = Vector2(r.size.x * 0.5, r.size.y)
	$PortMarkers/PortE.position = Vector2(r.size.x, r.size.y * 0.5)
	$PortMarkers/PortW.position = Vector2(0.0, r.size.y * 0.5)


func _on_segment_gui_input(event: InputEvent, index: int) -> void:
	if event is InputEventMouseButton:
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			_active_segment = index
			_press_position = event.position
			_press_timer.start()
			_set_segment_active(index, true)
		elif not event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			_press_timer.stop()
			if _active_segment == index:
				if not _press_timer.is_stopped() or _press_timer.time_left <= 0.0:
					segment_pressed.emit(index)
			_set_segment_active(index, false)
			_active_segment = -1
	elif event is InputEventMouseMotion and _active_segment == index:
		if _press_position.distance_to(event.position) > HudTheme.unit_px:
			_press_timer.stop()
			_set_segment_active(index, false)
			_active_segment = -1


func _set_segment_active(index: int, active: bool) -> void:
	if index < 0 or index >= _segments.size():
		return
	var sb: StyleBoxFlat = _segments[index].get_theme_stylebox("panel")
	if not sb:
		return
	if active:
		sb.bg_color = HudTheme.palette.get_solid(role).darkened(0.2)
		sb.border_width_top = 2
		sb.border_width_bottom = 2
		sb.border_width_left = 2
		sb.border_width_right = 2
		sb.border_color = HudTheme.palette.get_rim(role)
	else:
		sb.bg_color = HudTheme.palette.get_solid(role)
		sb.border_width_top = 0
		sb.border_width_bottom = 0
		sb.border_width_left = 0
		sb.border_width_right = 0


func _on_long_press_timeout() -> void:
	if _active_segment >= 0:
		segment_long_pressed.emit(_active_segment)


func _on_palette_changed() -> void:
	_apply_role_colors()
