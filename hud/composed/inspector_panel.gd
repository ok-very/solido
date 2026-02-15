extends Control

signal segment_selected(index: int)
signal port_positions_changed(ports: Dictionary)
signal section_collapsed(index: int, is_collapsed: bool)

const SECTION_LABELS := ["HEADER", "TRANSFORM", "MATERIAL", "RENDER"]

var _elbow: LcarsElbow
var _rail: LcarsRail
var _endcap: LcarsEndcap
var _content_area: Control
var _header: HBoxContainer
var _sections_container: VBoxContainer
var _sections: Array[CollapsibleSection] = []
var _active_segment: int = 0


func _ready() -> void:
	_elbow = $InspectorElbow
	_rail = $InspectorRail
	_endcap = $InspectorEndcap
	_content_area = $InspectorContentArea
	_header = $InspectorContentArea/InspectorHeader
	_sections_container = $InspectorContentArea/InspectorSections
	_rail.segment_pressed.connect(_on_rail_segment_pressed)
	_cache_sections()


func _cache_sections() -> void:
	for i in _sections_container.get_child_count():
		var child := _sections_container.get_child(i)
		if child is CollapsibleSection:
			_sections.append(child)
			var idx: int = _sections.size() - 1
			child.collapsed_changed.connect(func(is_collapsed: bool): _on_section_collapsed(idx, is_collapsed))


func _on_section_collapsed(index: int, is_collapsed: bool) -> void:
	section_collapsed.emit(index, is_collapsed)
	port_positions_changed.emit(get_port_positions())


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_layout()
		port_positions_changed.emit(get_port_positions())


func get_port_position(side: String) -> Vector2:
	return get_port_positions().get(side, global_position)


func get_port_positions() -> Dictionary:
	var r := get_global_rect()
	return {
		"inspector.focus": Vector2(r.position.x, r.position.y + r.size.y * 0.35),
		"inspector.shader": Vector2(r.position.x, r.position.y + r.size.y * 0.6),
		"inspector.telemetry": Vector2(r.position.x + r.size.x * 0.5, r.position.y + r.size.y),
	}


func get_active_segment() -> int:
	return _active_segment


func select_segment(index: int) -> void:
	if index < 0 or index >= SECTION_LABELS.size():
		return
	_active_segment = index
	segment_selected.emit(index)


func _on_rail_segment_pressed(index: int) -> void:
	select_segment(index)


func _update_layout() -> void:
	if not _elbow or not _rail or not _endcap:
		return
	var unit_px: float = HudTheme.unit_px
	var elbow_w := _elbow.custom_minimum_size.x
	var rail_thickness := _rail.custom_minimum_size.x
	var elbow_h := _elbow.custom_minimum_size.y
	var endcap_h := _endcap.custom_minimum_size.y
	var gap := unit_px
	var available_h := size.y - elbow_h - endcap_h - gap
	if available_h < 0.0:
		available_h = 0.0
	var rail_x := size.x - rail_thickness
	_elbow.position = Vector2(size.x - elbow_w, 0.0)
	_elbow.size = _elbow.custom_minimum_size
	_rail.position = Vector2(rail_x, elbow_h)
	_rail.size = Vector2(rail_thickness, available_h)
	_endcap.position = Vector2(rail_x, elbow_h + available_h + gap)
	_endcap.size = _endcap.custom_minimum_size
	_content_area.position = Vector2(0.0, elbow_h)
	_content_area.size = Vector2(rail_x - unit_px, available_h)
