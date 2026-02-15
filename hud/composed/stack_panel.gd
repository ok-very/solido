extends Control

signal segment_selected(index: int)
signal port_positions_changed(ports: Dictionary)

const CHIP_LABELS := ["LAYERS", "NODES", "HISTORY"]

var _elbow: LcarsElbow
var _rail: LcarsRail
var _endcap: LcarsEndcap
var _content_area: Control
var _chip_bar: HBoxContainer
var _chips: Array[Chip] = []
var _stack_list: VBoxContainer
var _active_segment: int = 0


func _ready() -> void:
	_elbow = $StackElbow
	_rail = $StackRail
	_endcap = $StackEndcap
	_content_area = $StackContentArea
	_chip_bar = $StackContentArea/StackChipBar
	_stack_list = $StackContentArea/StackList
	_setup_chips()
	_rail.segment_pressed.connect(_on_rail_segment_pressed)
	_sync_chip_to_segment(_active_segment)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_layout()
		port_positions_changed.emit(get_port_positions())


func get_port_position(side: String) -> Vector2:
	return get_port_positions().get(side, global_position)


func get_port_positions() -> Dictionary:
	var r := get_global_rect()
	return {
		"stack.active": Vector2(r.position.x + r.size.x, r.position.y + r.size.y * 0.2),
		"stack.selection": Vector2(r.position.x + r.size.x, r.position.y + r.size.y * 0.5),
		"stack.telemetry": Vector2(r.position.x + r.size.x * 0.5, r.position.y + r.size.y),
	}


func get_active_segment() -> int:
	return _active_segment


func select_segment(index: int) -> void:
	if index < 0 or index >= CHIP_LABELS.size():
		return
	_active_segment = index
	_sync_chip_to_segment(index)
	segment_selected.emit(index)


func _setup_chips() -> void:
	for i in CHIP_LABELS.size():
		var chip: Chip = _chip_bar.get_child(i)
		_chips.append(chip)
		var idx: int = i
		chip.pressed.connect(func(): _on_chip_pressed(idx))


func _on_rail_segment_pressed(index: int) -> void:
	select_segment(index)


func _on_chip_pressed(index: int) -> void:
	select_segment(index)


func _sync_chip_to_segment(index: int) -> void:
	for i in _chips.size():
		_chips[i].set_toggled(i == index)


func _update_layout() -> void:
	if not _elbow or not _rail or not _endcap:
		return
	var unit_px: float = HudTheme.unit_px
	var elbow_h := _elbow.custom_minimum_size.y
	var endcap_h := _endcap.custom_minimum_size.y
	var gap := unit_px
	var rail_y := elbow_h
	var available_h := size.y - elbow_h - endcap_h - gap
	if available_h < 0.0:
		available_h = 0.0
	_elbow.position = Vector2.ZERO
	_elbow.size = _elbow.custom_minimum_size
	_rail.position = Vector2(0.0, rail_y)
	_rail.size = Vector2(_rail.custom_minimum_size.x, available_h)
	_endcap.position = Vector2(0.0, rail_y + available_h + gap)
	_endcap.size = _endcap.custom_minimum_size
	var content_x := _rail.custom_minimum_size.x + unit_px
	_content_area.position = Vector2(content_x, elbow_h)
	_content_area.size = Vector2(size.x - content_x, available_h)
