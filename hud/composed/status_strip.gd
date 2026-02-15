extends Control

signal segment_selected(index: int)
signal port_positions_changed(ports: Dictionary)

const MODE_LABELS := ["SELECT", "MOVE", "ROTATE", "SCULPT"]

var _rail: LcarsRail
var _endcap_left: LcarsEndcap
var _endcap_right: LcarsEndcap
var _content_area: Control
var _mode_chips: HBoxContainer
var _chips: Array[Chip] = []
var _active_mode: int = 0


func _ready() -> void:
	_rail = $StatusRail
	_endcap_left = $StatusEndcapLeft
	_endcap_right = $StatusEndcapRight
	_content_area = $StatusContentArea
	_mode_chips = $StatusContentArea/StatusModeChips
	_setup_chips()
	_rail.segment_pressed.connect(_on_rail_segment_pressed)
	_sync_chip_to_mode(_active_mode)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_layout()
		port_positions_changed.emit(get_port_positions())


func get_port_position(side: String) -> Vector2:
	return get_port_positions().get(side, global_position)


func get_port_positions() -> Dictionary:
	var r := get_global_rect()
	return {
		"status.mode": Vector2(r.position.x + r.size.x * 0.15, r.position.y),
		"status.selection": Vector2(r.position.x + r.size.x * 0.4, r.position.y),
		"status.perf": Vector2(r.position.x + r.size.x * 0.7, r.position.y),
	}


func get_active_mode() -> int:
	return _active_mode


func select_mode(index: int) -> void:
	if index < 0 or index >= MODE_LABELS.size():
		return
	_active_mode = index
	_sync_chip_to_mode(index)


func _setup_chips() -> void:
	for i in _mode_chips.get_child_count():
		var child := _mode_chips.get_child(i)
		if child is Chip:
			_chips.append(child)
			var idx: int = i
			child.pressed.connect(func(): _on_mode_chip_pressed(idx))


func _on_rail_segment_pressed(index: int) -> void:
	segment_selected.emit(index)


func _on_mode_chip_pressed(index: int) -> void:
	select_mode(index)


func _sync_chip_to_mode(index: int) -> void:
	for i in _chips.size():
		_chips[i].set_toggled(i == index)


func _update_layout() -> void:
	if not _rail or not _endcap_left or not _endcap_right:
		return
	var unit_px: float = HudTheme.unit_px
	var endcap_l_w := _endcap_left.custom_minimum_size.x
	var endcap_r_w := _endcap_right.custom_minimum_size.x
	var rail_h := _rail.custom_minimum_size.y
	var gap := unit_px * 0.5
	var rail_y := size.y - rail_h
	var rail_x := endcap_l_w + gap
	var rail_w := size.x - endcap_l_w - endcap_r_w - gap * 2.0
	if rail_w < 0.0:
		rail_w = 0.0
	_endcap_left.position = Vector2(0.0, rail_y)
	_endcap_left.size = Vector2(endcap_l_w, rail_h)
	_rail.position = Vector2(rail_x, rail_y)
	_rail.size = Vector2(rail_w, rail_h)
	_endcap_right.position = Vector2(rail_x + rail_w + gap, rail_y)
	_endcap_right.size = Vector2(endcap_r_w, rail_h)
	_content_area.position = Vector2(rail_x, 0.0)
	_content_area.size = Vector2(rail_w, rail_y - unit_px)
