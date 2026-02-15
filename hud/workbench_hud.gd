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
var _connector_manager: ConnectorManager
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
	_connector_manager = $ConnectorLayer/ConnectorManager
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

	panel_port_positions_changed.connect(
		func(_panel_name: String, ports_dict: Dictionary):
			_connector_manager.update_port_positions(_panel_name, ports_dict)
			_connector_manager.update_buses(_build_bus_definitions())
	)

	call_deferred("_init_connectors")
	call_deferred("_update_inter_pane_buffers")


func _init_connectors() -> void:
	_connector_manager.configure(
		_build_port_definitions(),
		_build_net_definitions(),
		_build_bus_definitions(),
	)
	_connector_manager.update_all_port_positions(get_all_port_positions())


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


func get_connector_manager() -> ConnectorManager:
	return _connector_manager


func _build_port_definitions() -> Dictionary:
	var ports := { }
	ports["stack.active"] = HudPort.create(
		"stack.active",
		"stack",
		"E",
		0.2,
		HudRole.BLEND,
		90,
		"ARC",
		"ENDCAP",
	)
	ports["stack.selection"] = HudPort.create(
		"stack.selection",
		"stack",
		"E",
		0.5,
		HudRole.NAV,
		70,
		"ARC",
		"DOT",
	)
	ports["stack.telemetry"] = HudPort.create(
		"stack.telemetry",
		"stack",
		"S",
		0.5,
		HudRole.TELEMETRY,
		40,
	)
	ports["inspector.focus"] = HudPort.create(
		"inspector.focus",
		"inspector",
		"W",
		0.35,
		HudRole.BLEND,
		90,
		"ARC",
		"BRACKET",
	)
	ports["inspector.shader"] = HudPort.create(
		"inspector.shader",
		"inspector",
		"W",
		0.6,
		HudRole.EDIT,
		60,
		"ARC",
		"DOT",
	)
	ports["inspector.telemetry"] = HudPort.create(
		"inspector.telemetry",
		"inspector",
		"S",
		0.5,
		HudRole.TELEMETRY,
		40,
	)
	ports["status.mode"] = HudPort.create(
		"status.mode",
		"status",
		"N",
		0.15,
		HudRole.NAV,
		50,
	)
	ports["status.selection"] = HudPort.create(
		"status.selection",
		"status",
		"N",
		0.4,
		HudRole.NAV,
		60,
		"ARC",
		"BRACKET",
	)
	ports["status.perf"] = HudPort.create(
		"status.perf",
		"status",
		"N",
		0.7,
		HudRole.TELEMETRY,
		30,
	)
	return ports


func _build_net_definitions() -> Array[HudNet]:
	return [
		HudNet.create(
			"active_to_focus",
			HudNet.KIND_HIGHLIGHT,
			90,
			"stack.active",
			"inspector.focus",
			"BUS_BRANCH",
			"center_bus",
		),
		HudNet.create(
			"selection_to_status",
			HudNet.KIND_DATAFLOW,
			70,
			"stack.selection",
			"status.selection",
		),
		HudNet.create(
			"stack_telem_to_status",
			HudNet.KIND_DATAFLOW,
			40,
			"stack.telemetry",
			"status.perf",
		),
		HudNet.create(
			"inspector_telem_to_status",
			HudNet.KIND_DATAFLOW,
			40,
			"inspector.telemetry",
			"status.perf",
		),
		HudNet.create(
			"mode_to_inspector",
			HudNet.KIND_HIGHLIGHT,
			60,
			"status.mode",
			"inspector.shader",
		),
	]


func _build_bus_definitions() -> Dictionary:
	var stack_r := _stack_panel.get_global_rect()
	var inspector_r := _inspector_panel.get_global_rect()
	var status_r := _status_strip.get_global_rect()
	var unit := HudTheme.unit_px
	var bus_w := 3.0 * unit
	var gap_center_x := (stack_r.end.x + inspector_r.position.x) * 0.5
	var bus_top := minf(stack_r.position.y, inspector_r.position.y)
	var bus_bottom := status_r.position.y
	return {
		"center_bus": HudBus.create(
			"center_bus",
			HudBus.ORIENT_VERT,
			3,
			Rect2(gap_center_x - bus_w * 0.5, bus_top, bus_w, bus_bottom - bus_top),
			HudRole.NAV,
			2.5,
		),
	}
