extends GutTest

var _manager: ConnectorManager


func before_each():
	_manager = ConnectorManager.new()
	add_child_autofree(_manager)


func _wait_for_route() -> void:
	await get_tree().process_frame
	await get_tree().process_frame


func _make_ports() -> Dictionary:
	return {
		"a": HudPort.create("a", "stack", "E", 0.2, HudRole.BLEND, 90),
		"b": HudPort.create("b", "inspector", "W", 0.35, HudRole.BLEND, 90),
	}


func _make_positions() -> Dictionary:
	return {
		"a": Vector2(480, 432),
		"b": Vector2(2400, 756),
	}


func _make_direct_net() -> Array[HudNet]:
	return [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]


func _make_bus_net() -> Array[HudNet]:
	return [
		HudNet.create(
			"bus_net",
			HudNet.KIND_HIGHLIGHT,
			90,
			"a",
			"b",
			"BUS_BRANCH",
			"center_bus",
		),
	]


func _make_bus() -> Dictionary:
	return {
		"center_bus": HudBus.create(
			"center_bus",
			HudBus.ORIENT_VERT,
			3,
			Rect2(1519, 130, 72, 1900),
			HudRole.NAV,
			2.5,
		),
	}

# === Configuration ===


func test_configure_sets_dirty():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_true(_manager.get_connector_count() > 0)

# === Direct Arc Spawning ===


func test_direct_arc_spawns_one_connector():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_eq(_manager.get_connector_count(), 1)


func test_connector_is_spline_connector():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var children := _manager.get_children()
	var connector_count := 0
	for child in children:
		if child is SplineConnector:
			connector_count += 1
	assert_eq(connector_count, 1)

# === Bus-Branch Spawning ===


func test_bus_branch_spawns_two_connectors():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	# Bus segment skipped — only 2 branch connectors rendered
	assert_eq(_manager.get_connector_count(), 2)

# === Role and Importance ===


func test_connector_role_matches_from_port():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	for child in _manager.get_children():
		if child is SplineConnector:
			assert_eq(child.role, HudRole.BLEND)


func test_connector_importance_matches_net():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	for child in _manager.get_children():
		if child is SplineConnector:
			assert_eq(child.importance, 70)

# === Position Updates ===


func test_position_update_re_routes():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var first_child: SplineConnector = null
	for child in _manager.get_children():
		if child is SplineConnector:
			first_child = child
			break
	assert_not_null(first_child)
	var old_start := first_child.get_start_position()
	# Move port a
	_manager.update_port_positions("stack", { "a": Vector2(500, 500) })
	await _wait_for_route()
	var new_start := first_child.get_start_position()
	assert_ne(old_start, new_start)

# === Orphan Cleanup ===


func test_orphan_cleanup():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_eq(_manager.get_connector_count(), 1)
	# Reconfigure with no nets
	var empty_nets: Array[HudNet] = []
	_manager.configure(_make_ports(), empty_nets, { })
	_manager._dirty = true
	await _wait_for_route()
	assert_eq(_manager.get_connector_count(), 0)

# === Signal ===


func test_connectors_updated_signal():
	watch_signals(_manager)
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_signal_emitted(_manager, "connectors_updated")

# === Batching ===


func test_deferred_batching():
	watch_signals(_manager)
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	# Two updates in same frame
	_manager.update_port_positions("stack", { "a": Vector2(490, 440) })
	_manager.update_port_positions("inspector", { "b": Vector2(2410, 760) })
	await _wait_for_route()
	# Should only emit once (one route pass)
	assert_signal_emit_count(_manager, "connectors_updated", 1)

# === Bus Rail Lifecycle ===


func test_bus_rail_created_for_bus():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_eq(_manager.get_bus_rail_count(), 1)


func test_bus_rail_is_lcars_rail():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var rail := _manager.get_bus_rail("center_bus")
	assert_not_null(rail)
	assert_true(rail is LcarsRail)


func test_bus_rail_role_matches_bus():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var rail := _manager.get_bus_rail("center_bus")
	assert_eq(rail.role, HudRole.NAV)


func test_bus_rail_positioned_from_bus_rect():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var rail := _manager.get_bus_rail("center_bus")
	assert_eq(rail.position, Vector2(1519, 130))
	assert_eq(rail.size, Vector2(72, 1900))


func test_bus_rail_alpha():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	var rail := _manager.get_bus_rail("center_bus")
	assert_almost_eq(rail.modulate.a, 0.7, 0.01)


func test_bus_rail_cleanup_on_reconfigure():
	_manager.configure(_make_ports(), _make_bus_net(), _make_bus())
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_eq(_manager.get_bus_rail_count(), 1)
	# Reconfigure with no buses
	var empty_nets: Array[HudNet] = []
	_manager.configure(_make_ports(), empty_nets, { })
	_manager._dirty = true
	await _wait_for_route()
	assert_eq(_manager.get_bus_rail_count(), 0)


func test_no_bus_rails_without_buses():
	_manager.configure(_make_ports(), _make_direct_net(), { })
	_manager.update_all_port_positions(_make_positions())
	await _wait_for_route()
	assert_eq(_manager.get_bus_rail_count(), 0)
