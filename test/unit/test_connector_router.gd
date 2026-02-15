extends GutTest

var _router: ConnectorRouter


func before_each():
	_router = ConnectorRouter.new()

# === Determinism ===


func test_deterministic_same_inputs():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var result_1 := _router.route_all(ports, positions, nets, { }, 24.0)
	var result_2 := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(result_1.size(), result_2.size())
	var pts_1: PackedVector2Array = result_1[0]["segments"][0]["points"]
	var pts_2: PackedVector2Array = result_2[0]["segments"][0]["points"]
	assert_eq(pts_1.size(), pts_2.size())
	for i in pts_1.size():
		assert_eq(pts_1[i], pts_2[i])

# === Direct Arc ===


func test_direct_arc_basic():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results.size(), 1)
	var segs: Array = results[0]["segments"]
	assert_eq(segs.size(), 1)
	assert_eq(segs[0]["type"], "arc")
	var points: PackedVector2Array = segs[0]["points"]
	assert_true(points.size() > 2)


func test_direct_arc_starts_near_from():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	var points: PackedVector2Array = results[0]["segments"][0]["points"]
	assert_almost_eq(points[0].x, 480.0, 1.0)
	assert_almost_eq(points[0].y, 432.0, 1.0)


func test_direct_arc_ends_near_to():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	var points: PackedVector2Array = results[0]["segments"][0]["points"]
	var last: Vector2 = points[points.size() - 1]
	assert_almost_eq(last.x, 2400.0, 1.0)
	assert_almost_eq(last.y, 756.0, 1.0)


func test_direct_arc_all_side_combos():
	var side_pairs := [["E", "W"], ["N", "S"], ["S", "N"], ["E", "N"]]
	for pair in side_pairs:
		var ports := {
			"a": HudPort.create("a", "n1", pair[0], 0.5, HudRole.NAV, 50),
			"b": HudPort.create("b", "n2", pair[1], 0.5, HudRole.NAV, 50),
		}
		var positions := { "a": Vector2(100, 100), "b": Vector2(500, 500) }
		var nets: Array[HudNet] = [
			HudNet.create("n", HudNet.KIND_DATAFLOW, 50, "a", "b"),
		]
		var results := _router.route_all(ports, positions, nets, { }, 24.0)
		assert_eq(results.size(), 1, "Failed for sides %s->%s" % [pair[0], pair[1]])
		var pts: PackedVector2Array = results[0]["segments"][0]["points"]
		assert_true(pts.size() > 2, "Too few points for %s->%s" % [pair[0], pair[1]])

# === Net Sorting ===


func test_net_sort_by_importance():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("low", HudNet.KIND_DATAFLOW, 30, "a", "b"),
		HudNet.create("high", HudNet.KIND_HIGHLIGHT, 90, "a", "b"),
		HudNet.create("mid", HudNet.KIND_DATAFLOW, 60, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results[0]["net_id"], "high")
	assert_eq(results[1]["net_id"], "mid")
	assert_eq(results[2]["net_id"], "low")


func test_net_sort_tiebreak_by_id():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("zebra", HudNet.KIND_DATAFLOW, 50, "a", "b"),
		HudNet.create("alpha", HudNet.KIND_DATAFLOW, 50, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results[0]["net_id"], "alpha")
	assert_eq(results[1]["net_id"], "zebra")

# === Bus-Branch Routing ===


func test_bus_branch_produces_three_segments():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var bus := _make_center_bus()
	var nets: Array[HudNet] = [
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
	var results := _router.route_all(ports, positions, nets, { "center_bus": bus }, 24.0)
	assert_eq(results.size(), 1)
	var segs: Array = results[0]["segments"]
	assert_eq(segs.size(), 3)
	assert_eq(segs[0]["type"], "arc")
	assert_eq(segs[1]["type"], "bus")
	assert_eq(segs[2]["type"], "arc")


func test_bus_segment_on_bus_lane():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var bus := _make_center_bus()
	var nets: Array[HudNet] = [
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
	var results := _router.route_all(ports, positions, nets, { "center_bus": bus }, 24.0)
	var bus_points: PackedVector2Array = results[0]["segments"][1]["points"]
	assert_eq(bus_points.size(), 2)
	# Bus is vertical at x ~1555 (center of rect 1519, width 72)
	var bus_center_x := 1519.0 + 36.0
	# Lane offset varies, but point X should be within bus rect
	assert_true(bus_points[0].x >= 1519.0 and bus_points[0].x <= 1591.0)
	assert_true(bus_points[1].x >= 1519.0 and bus_points[1].x <= 1591.0)


func test_bus_branch_fallback_to_direct_arc():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create(
			"missing_bus",
			HudNet.KIND_HIGHLIGHT,
			90,
			"a",
			"b",
			"BUS_BRANCH",
			"nonexistent",
		),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results.size(), 1)
	var segs: Array = results[0]["segments"]
	assert_eq(segs.size(), 1)
	assert_eq(segs[0]["type"], "arc")

# === Edge Cases ===


func test_missing_port_returns_empty():
	var ports := { "a": HudPort.create("a", "n1", "E", 0.5, HudRole.NAV, 50) }
	var positions := { "a": Vector2(100, 100) }
	var nets: Array[HudNet] = [
		HudNet.create("broken", HudNet.KIND_DATAFLOW, 50, "a", "missing"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results.size(), 0)


func test_empty_nets_returns_empty():
	var nets: Array[HudNet] = []
	var results := _router.route_all({ }, { }, nets, { }, 24.0)
	assert_eq(results.size(), 0)


func test_role_propagation():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results[0]["role"], HudRole.BLEND)


func test_importance_propagation():
	var ports := _make_two_ports()
	var positions := _make_two_positions()
	var nets: Array[HudNet] = [
		HudNet.create("net_a", HudNet.KIND_DATAFLOW, 70, "a", "b"),
	]
	var results := _router.route_all(ports, positions, nets, { }, 24.0)
	assert_eq(results[0]["importance"], 70)

# === Helpers ===


func _make_two_ports() -> Dictionary:
	return {
		"a": HudPort.create("a", "stack", "E", 0.2, HudRole.BLEND, 90),
		"b": HudPort.create("b", "inspector", "W", 0.35, HudRole.BLEND, 90),
	}


func _make_two_positions() -> Dictionary:
	return {
		"a": Vector2(480, 432),
		"b": Vector2(2400, 756),
	}


func _make_center_bus() -> HudBus:
	return HudBus.create(
		"center_bus",
		HudBus.ORIENT_VERT,
		3,
		Rect2(1519, 130, 72, 1900),
		HudRole.NAV,
		2.5,
	)
