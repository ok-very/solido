class_name WorkbenchHUD
extends CanvasLayer

signal panel_segment_selected(panel_name: String, index: int)
signal panel_port_positions_changed(panel_name: String, ports: Dictionary)

var _world_feed: SubViewportContainer
var _world_viewport: SubViewport

var _glass_layer: Control
var _stack_glass: GlassPane
var _inspector_glass: GlassPane
var _status_glass: GlassPane
var _stack_buffer_copy: BackBufferCopy
var _inspector_buffer_copy: BackBufferCopy

var _structure_layer: Control
var _stack_panel: Control
var _inspector_panel: Control
var _status_strip: Control

var _connector_layer: Control
var _text_layer: Control
var _stack_labels: Control
var _inspector_labels: Control
var _status_labels: Control


func _ready() -> void:
	_world_feed = $WorldFeed
	_world_viewport = $WorldFeed/WorldViewport

	_glass_layer = $GlassLayer
	_stack_glass = $GlassLayer/StackGlass
	_inspector_glass = $GlassLayer/InspectorGlass
	_status_glass = $GlassLayer/StatusGlass
	_stack_buffer_copy = $GlassLayer/StackBufferCopy
	_inspector_buffer_copy = $GlassLayer/InspectorBufferCopy

	_structure_layer = $StructureLayer
	_stack_panel = $StructureLayer/StackPanel
	_inspector_panel = $StructureLayer/InspectorPanel
	_status_strip = $StructureLayer/StatusStrip

	_connector_layer = $ConnectorLayer
	_text_layer = $TextLayer
	_stack_labels = $TextLayer/StackLabels
	_inspector_labels = $TextLayer/InspectorLabels
	_status_labels = $TextLayer/StatusLabels

	_stack_panel.segment_selected.connect(func(idx: int): panel_segment_selected.emit("stack", idx))
	_inspector_panel.segment_selected.connect(func(idx: int): panel_segment_selected.emit("inspector", idx))
	_status_strip.segment_selected.connect(func(idx: int): panel_segment_selected.emit("status", idx))

	_stack_panel.port_positions_changed.connect(func(ports: Dictionary): panel_port_positions_changed.emit("stack", ports))
	_inspector_panel.port_positions_changed.connect(func(ports: Dictionary): panel_port_positions_changed.emit("inspector", ports))
	_status_strip.port_positions_changed.connect(func(ports: Dictionary): panel_port_positions_changed.emit("status", ports))

	_stack_glass.resized.connect(_update_inter_pane_buffers)
	_inspector_glass.resized.connect(_update_inter_pane_buffers)

	call_deferred("_update_inter_pane_buffers")


func _update_inter_pane_buffers() -> void:
	_stack_buffer_copy.rect = _stack_glass.get_rect()
	_inspector_buffer_copy.rect = _inspector_glass.get_rect()


func get_glass_layer() -> Control:
	return _glass_layer


func get_structure_layer() -> Control:
	return _structure_layer


func get_connector_layer() -> Control:
	return _connector_layer


func get_text_layer() -> Control:
	return _text_layer


func get_stack_panel() -> Control:
	return _stack_panel


func get_inspector_panel() -> Control:
	return _inspector_panel


func get_status_strip() -> Control:
	return _status_strip


func get_stack_glass() -> GlassPane:
	return _stack_glass


func get_inspector_glass() -> GlassPane:
	return _inspector_glass


func get_status_glass() -> GlassPane:
	return _status_glass


func get_world_viewport() -> SubViewport:
	return _world_viewport


func get_all_port_positions() -> Dictionary:
	var all_ports := { }
	all_ports.merge(_stack_panel.get_port_positions())
	all_ports.merge(_inspector_panel.get_port_positions())
	all_ports.merge(_status_strip.get_port_positions())
	return all_ports
