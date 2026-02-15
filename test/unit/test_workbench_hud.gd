extends GutTest

var _scene := preload("res://hud/workbench_hud.tscn")


func _create_hud() -> WorkbenchHUD:
	var hud: WorkbenchHUD = _scene.instantiate()
	add_child_autofree(hud)
	return hud

# === A. Root ===


func test_instantiates():
	var hud := _create_hud()
	assert_not_null(hud)
	assert_true(hud is CanvasLayer)


func test_canvas_layer():
	var hud := _create_hud()
	assert_eq(hud.layer, 10)

# === B. WorldFeed ===


func test_world_feed_exists():
	var hud := _create_hud()
	var world_feed: SubViewportContainer = hud.get_node("WorldFeed")
	assert_not_null(world_feed)
	assert_true(world_feed is SubViewportContainer)
	assert_true(world_feed.stretch)


func test_world_viewport_exists():
	var hud := _create_hud()
	var world_viewport: SubViewport = hud.get_node("WorldFeed/WorldViewport")
	assert_not_null(world_viewport)
	assert_true(world_viewport is SubViewport)


func test_world_camera_exists():
	var hud := _create_hud()
	var camera: Camera3D = hud.get_node("WorldFeed/WorldViewport/WorldCamera")
	assert_not_null(camera)
	assert_true(camera is Camera3D)
	assert_true(camera.current)


func test_world_environment_exists():
	var hud := _create_hud()
	var env: WorldEnvironment = hud.get_node("WorldFeed/WorldViewport/WorldEnvironment")
	assert_not_null(env)
	assert_true(env is WorldEnvironment)

# === C. GlassLayer ===


func test_glass_layer_exists():
	var hud := _create_hud()
	var glass_layer: Control = hud.get_node("GlassLayer")
	assert_not_null(glass_layer)
	assert_true(glass_layer is Control)


func test_glass_layer_z_index():
	var hud := _create_hud()
	var glass_layer: Control = hud.get_node("GlassLayer")
	assert_eq(glass_layer.z_index, 0)


func test_stack_glass():
	var hud := _create_hud()
	var stack_glass: GlassPane = hud.get_node("GlassLayer/StackGlass")
	assert_not_null(stack_glass)
	assert_true(stack_glass is GlassPane)
	assert_eq(stack_glass.role, HudRole.NAV)


func test_stack_glass_anchors():
	var hud := _create_hud()
	var stack_glass: Control = hud.get_node("GlassLayer/StackGlass")
	assert_almost_eq(stack_glass.anchor_left, 0.0, 0.001)
	assert_almost_eq(stack_glass.anchor_right, 0.22, 0.001)
	assert_almost_eq(stack_glass.anchor_top, 0.06, 0.001)
	assert_almost_eq(stack_glass.anchor_bottom, 0.94, 0.001)


func test_inspector_glass():
	var hud := _create_hud()
	var inspector_glass: GlassPane = hud.get_node("GlassLayer/InspectorGlass")
	assert_not_null(inspector_glass)
	assert_true(inspector_glass is GlassPane)
	assert_eq(inspector_glass.role, HudRole.EDIT)


func test_inspector_glass_anchors():
	var hud := _create_hud()
	var inspector_glass: Control = hud.get_node("GlassLayer/InspectorGlass")
	assert_almost_eq(inspector_glass.anchor_left, 0.74, 0.001)
	assert_almost_eq(inspector_glass.anchor_right, 1.0, 0.001)
	assert_almost_eq(inspector_glass.anchor_top, 0.04, 0.001)
	assert_almost_eq(inspector_glass.anchor_bottom, 0.94, 0.001)


func test_status_glass():
	var hud := _create_hud()
	var status_glass: GlassPane = hud.get_node("GlassLayer/StatusGlass")
	assert_not_null(status_glass)
	assert_true(status_glass is GlassPane)
	assert_eq(status_glass.role, HudRole.TELEMETRY)


func test_status_glass_anchors():
	var hud := _create_hud()
	var status_glass: Control = hud.get_node("GlassLayer/StatusGlass")
	assert_almost_eq(status_glass.anchor_left, 0.0, 0.001)
	assert_almost_eq(status_glass.anchor_right, 1.0, 0.001)
	assert_almost_eq(status_glass.anchor_top, 0.94, 0.001)
	assert_almost_eq(status_glass.anchor_bottom, 1.0, 0.001)

# === D. Inter-Pane BackBufferCopy ===


func test_stack_buffer_copy():
	var hud := _create_hud()
	var buffer_copy: BackBufferCopy = hud.get_node("GlassLayer/StackBufferCopy")
	assert_not_null(buffer_copy)
	assert_true(buffer_copy is BackBufferCopy)
	assert_eq(buffer_copy.copy_mode, BackBufferCopy.COPY_MODE_RECT)


func test_inspector_buffer_copy():
	var hud := _create_hud()
	var buffer_copy: BackBufferCopy = hud.get_node("GlassLayer/InspectorBufferCopy")
	assert_not_null(buffer_copy)
	assert_true(buffer_copy is BackBufferCopy)
	assert_eq(buffer_copy.copy_mode, BackBufferCopy.COPY_MODE_RECT)


func test_glass_sibling_order():
	var hud := _create_hud()
	var glass_layer: Control = hud.get_node("GlassLayer")
	var stack_glass := glass_layer.get_node("StackGlass")
	var stack_buffer := glass_layer.get_node("StackBufferCopy")
	var inspector_glass := glass_layer.get_node("InspectorGlass")
	var inspector_buffer := glass_layer.get_node("InspectorBufferCopy")
	var status_glass := glass_layer.get_node("StatusGlass")

	assert_true(stack_glass.get_index() < stack_buffer.get_index())
	assert_true(stack_buffer.get_index() < inspector_glass.get_index())
	assert_true(inspector_glass.get_index() < inspector_buffer.get_index())
	assert_true(inspector_buffer.get_index() < status_glass.get_index())

# === E. StructureLayer ===


func test_structure_layer_exists():
	var hud := _create_hud()
	var structure_layer: Control = hud.get_node("StructureLayer")
	assert_not_null(structure_layer)
	assert_true(structure_layer is Control)


func test_structure_layer_z_index():
	var hud := _create_hud()
	var structure_layer: Control = hud.get_node("StructureLayer")
	assert_eq(structure_layer.z_index, 1)


func test_stack_panel_exists():
	var hud := _create_hud()
	var stack_panel: Control = hud.get_node("StructureLayer/StackPanel")
	assert_not_null(stack_panel)


func test_inspector_panel_exists():
	var hud := _create_hud()
	var inspector_panel: Control = hud.get_node("StructureLayer/InspectorPanel")
	assert_not_null(inspector_panel)


func test_status_strip_exists():
	var hud := _create_hud()
	var status_strip: Control = hud.get_node("StructureLayer/StatusStrip")
	assert_not_null(status_strip)

# === F. Panel Anchors Match Glass ===


func test_stack_panel_anchors():
	var hud := _create_hud()
	var panel: Control = hud.get_node("StructureLayer/StackPanel")
	assert_almost_eq(panel.anchor_left, 0.0, 0.001)
	assert_almost_eq(panel.anchor_right, 0.22, 0.001)
	assert_almost_eq(panel.anchor_top, 0.06, 0.001)
	assert_almost_eq(panel.anchor_bottom, 0.94, 0.001)


func test_inspector_panel_anchors():
	var hud := _create_hud()
	var panel: Control = hud.get_node("StructureLayer/InspectorPanel")
	assert_almost_eq(panel.anchor_left, 0.74, 0.001)
	assert_almost_eq(panel.anchor_right, 1.0, 0.001)
	assert_almost_eq(panel.anchor_top, 0.04, 0.001)
	assert_almost_eq(panel.anchor_bottom, 0.94, 0.001)


func test_status_strip_anchors():
	var hud := _create_hud()
	var panel: Control = hud.get_node("StructureLayer/StatusStrip")
	assert_almost_eq(panel.anchor_left, 0.0, 0.001)
	assert_almost_eq(panel.anchor_right, 1.0, 0.001)
	assert_almost_eq(panel.anchor_top, 0.94, 0.001)
	assert_almost_eq(panel.anchor_bottom, 1.0, 0.001)

# === G. ConnectorLayer ===


func test_connector_layer_exists():
	var hud := _create_hud()
	var connector_layer: Control = hud.get_node("ConnectorLayer")
	assert_not_null(connector_layer)
	assert_true(connector_layer is Control)


func test_connector_layer_z_index():
	var hud := _create_hud()
	var connector_layer: Control = hud.get_node("ConnectorLayer")
	assert_eq(connector_layer.z_index, 2)


func test_connector_layer_mouse_filter():
	var hud := _create_hud()
	var connector_layer: Control = hud.get_node("ConnectorLayer")
	assert_eq(connector_layer.mouse_filter, Control.MOUSE_FILTER_IGNORE)

# === H. TextLayer ===


func test_text_layer_exists():
	var hud := _create_hud()
	var text_layer: Control = hud.get_node("TextLayer")
	assert_not_null(text_layer)
	assert_true(text_layer is Control)


func test_text_layer_z_index():
	var hud := _create_hud()
	var text_layer: Control = hud.get_node("TextLayer")
	assert_eq(text_layer.z_index, 3)


func test_text_layer_mouse_filter():
	var hud := _create_hud()
	var text_layer: Control = hud.get_node("TextLayer")
	assert_eq(text_layer.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_text_layer_children():
	var hud := _create_hud()
	var text_layer: Control = hud.get_node("TextLayer")
	var stack_labels: Control = text_layer.get_node("StackLabels")
	var inspector_labels: Control = text_layer.get_node("InspectorLabels")
	var status_labels: Control = text_layer.get_node("StatusLabels")
	assert_not_null(stack_labels)
	assert_not_null(inspector_labels)
	assert_not_null(status_labels)

# === I. Z-Index Ordering ===


func test_z_index_ordering():
	var hud := _create_hud()
	var glass_layer: Control = hud.get_node("GlassLayer")
	var structure_layer: Control = hud.get_node("StructureLayer")
	var connector_layer: Control = hud.get_node("ConnectorLayer")
	var text_layer: Control = hud.get_node("TextLayer")

	assert_true(glass_layer.z_index < structure_layer.z_index)
	assert_true(structure_layer.z_index < connector_layer.z_index)
	assert_true(connector_layer.z_index < text_layer.z_index)

# === J. Public API ===


func test_get_layers():
	var hud := _create_hud()
	var glass_layer := hud.get_glass_layer()
	var structure_layer := hud.get_structure_layer()
	var connector_layer := hud.get_connector_layer()
	var text_layer := hud.get_text_layer()

	assert_not_null(glass_layer)
	assert_not_null(structure_layer)
	assert_not_null(connector_layer)
	assert_not_null(text_layer)
	assert_true(glass_layer is Control)
	assert_true(structure_layer is Control)
	assert_true(connector_layer is Control)
	assert_true(text_layer is Control)


func test_get_panels():
	var hud := _create_hud()
	var stack_panel := hud.get_stack_panel()
	var inspector_panel := hud.get_inspector_panel()
	var status_strip := hud.get_status_strip()

	assert_not_null(stack_panel)
	assert_not_null(inspector_panel)
	assert_not_null(status_strip)
	assert_true(stack_panel is Control)
	assert_true(inspector_panel is Control)
	assert_true(status_strip is Control)


func test_get_glass_panes():
	var hud := _create_hud()
	var stack_glass := hud.get_stack_glass()
	var inspector_glass := hud.get_inspector_glass()
	var status_glass := hud.get_status_glass()

	assert_not_null(stack_glass)
	assert_not_null(inspector_glass)
	assert_not_null(status_glass)
	assert_true(stack_glass is GlassPane)
	assert_true(inspector_glass is GlassPane)
	assert_true(status_glass is GlassPane)


func test_get_world_viewport():
	var hud := _create_hud()
	var viewport := hud.get_world_viewport()
	assert_not_null(viewport)
	assert_true(viewport is SubViewport)


func test_get_all_port_positions():
	var hud := _create_hud()
	var ports: Dictionary = hud.get_all_port_positions()
	assert_not_null(ports)
	assert_true(ports.has("stack.active"))
	assert_true(ports.has("inspector.focus"))
	assert_true(ports.has("status.mode"))

# === K. Signal Forwarding ===


func test_panel_segment_selected_forwarding():
	var hud := _create_hud()
	watch_signals(hud)
	hud.get_node("StructureLayer/StackPanel").select_segment(1)
	assert_signal_emitted(hud, "panel_segment_selected")


func test_panel_port_positions_changed_forwarding():
	var hud := _create_hud()
	watch_signals(hud)
	var sections: VBoxContainer = hud.get_node("StructureLayer/InspectorPanel/InspectorContentArea/InspectorSections")
	var section: CollapsibleSection = sections.get_child(0)
	section.set_collapsed(true)
	assert_signal_emitted(hud, "panel_port_positions_changed")
