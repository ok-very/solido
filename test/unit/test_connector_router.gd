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

# === Elbow Radius ===


func test_elbow_radius_primary():
	assert_eq(_router._elbow_radius_for_importance(80, 24.0), 24.0)
	assert_eq(_router._elbow_radius_for_importance(90, 24.0), 24.0)
	assert_eq(_router._elbow_radius_for_importance(100, 24.0), 24.0)


func test_elbow_radius_secondary():
	assert_eq(_router._elbow_radius_for_importance(50, 24.0), 18.0)
	assert_eq(_router._elbow_radius_for_importance(70, 24.0), 18.0)
	assert_eq(_router._elbow_radius_for_importance(79, 24.0), 18.0)


func test_elbow_radius_tertiary():
	assert_eq(_router._elbow_radius_for_importance(0, 24.0), 12.0)
	assert_eq(_router._elbow_radius_for_importance(49, 24.0), 12.0)

# === Elbow Arc Quality ===


func test_elbow_points_form_quarter_arc():
	var corner := Vector2(200, 100)
	var d_in := Vector2(1, 0)
	var d_out := Vector2(0, 1)
	var radius := 24.0
	var pts := _router._build_elbow_points(corner, d_in, d_out, radius)

	assert_eq(pts.size(), 8, "Default point_count is 8")

	var center := corner + (-d_in + d_out) * radius
	for i in pts.size():
		var dist := pts[i].distance_to(center)
		assert_almost_eq(dist, radius, 0.01, "Point %d not on arc" % i)

	var arc_start := corner - d_in * radius
	var arc_end := corner + d_out * radius
	assert_almost_eq(pts[0].x, arc_start.x, 0.01)
	assert_almost_eq(pts[0].y, arc_start.y, 0.01)
	assert_almost_eq(pts[pts.size() - 1].x, arc_end.x, 0.01)
	assert_almost_eq(pts[pts.size() - 1].y, arc_end.y, 0.01)


func test_elbow_points_all_turn_directions():
	var turns := [
		[Vector2(1, 0), Vector2(0, 1)], # E -> S
		[Vector2(1, 0), Vector2(0, -1)], # E -> N
		[Vector2(-1, 0), Vector2(0, 1)], # W -> S
		[Vector2(-1, 0), Vector2(0, -1)], # W -> N
		[Vector2(0, 1), Vector2(1, 0)], # S -> E
		[Vector2(0, 1), Vector2(-1, 0)], # S -> W
		[Vector2(0, -1), Vector2(1, 0)], # N -> E
		[Vector2(0, -1), Vector2(-1, 0)], # N -> W
	]
	var corner := Vector2(300, 300)
	var radius := 18.0

	for turn in turns:
		var d_in: Vector2 = turn[0]
		var d_out: Vector2 = turn[1]
		var pts := _router._build_elbow_points(corner, d_in, d_out, radius)
		assert_eq(pts.size(), 8, "Turn %s->%s" % [d_in, d_out])
		var center := corner + (-d_in + d_out) * radius
		for i in pts.size():
			var dist := pts[i].distance_to(center)
			assert_almost_eq(
				dist,
				radius,
				0.1,
				"Turn %s->%s point %d off-arc" % [d_in, d_out, i],
			)

# === Route Type Classification ===


func test_rectilinear_opposite_sides_z_route():
	# E->W: should produce Z-route (H-V-H)
	var pts := _router._build_rectilinear(
		Vector2(100, 200),
		"E",
		Vector2(500, 400),
		"W",
		70,
		24.0,
	)
	assert_true(pts.size() > 2)
	assert_almost_eq(pts[0].x, 100.0, 1.0)
	assert_almost_eq(pts[0].y, 200.0, 1.0)
	var last := pts[pts.size() - 1]
	assert_almost_eq(last.x, 500.0, 1.0)
	assert_almost_eq(last.y, 400.0, 1.0)
	# Mid-x should be around (100+500)/2 = 300
	var found_mid := false
	for p in pts:
		if absf(p.x - 300.0) < 20.0:
			found_mid = true
			break
	assert_true(found_mid, "Z-route should pass through midpoint x")


func test_rectilinear_perpendicular_sides_l_route():
	# E->S: perpendicular, corner at (500, 200)
	var pts := _router._build_rectilinear(
		Vector2(100, 200),
		"E",
		Vector2(500, 500),
		"S",
		70,
		24.0,
	)
	assert_true(pts.size() > 2)
	# L-route: goes right then down. Max y should be near p3.y
	var max_y := 0.0
	for p in pts:
		max_y = maxf(max_y, p.y)
	assert_almost_eq(max_y, 500.0, 1.0, "L-route should reach dest y")


func test_rectilinear_same_sides_u_route():
	# E->E: extends right beyond both ports
	var pts := _router._build_rectilinear(
		Vector2(100, 200),
		"E",
		Vector2(300, 400),
		"E",
		70,
		24.0,
	)
	assert_true(pts.size() > 2)
	# U-route extends beyond max x (300 + 2*24 = 348)
	var max_x := 0.0
	for p in pts:
		max_x = maxf(max_x, p.x)
	assert_true(max_x > 300.0, "U-route should extend beyond both ports")


func test_rectilinear_perpendicular_fallback_to_z():
	# E->N where dest is to the LEFT of source -> L corner unreachable -> Z fallback
	var pts := _router._build_rectilinear(
		Vector2(500, 100),
		"E",
		Vector2(100, 500),
		"N",
		50,
		24.0,
	)
	assert_true(pts.size() > 2, "Fallback Z should produce valid path")
	assert_almost_eq(pts[0].x, 500.0, 1.0)
	assert_almost_eq(pts[0].y, 100.0, 1.0)
	var last := pts[pts.size() - 1]
	assert_almost_eq(last.x, 100.0, 1.0)
	assert_almost_eq(last.y, 500.0, 1.0)

# === Bounding Corridor ===


func test_z_route_stays_in_bounding_corridor():
	# E->W from (480,432) to (2400,756)
	var pts := _router._build_rectilinear(
		Vector2(480, 432),
		"E",
		Vector2(2400, 756),
		"W",
		70,
		24.0,
	)
	var epsilon := 2.0
	for i in pts.size():
		assert_true(
			pts[i].x >= 480.0 - epsilon and pts[i].x <= 2400.0 + epsilon,
			"Point %d x=%.1f out of corridor" % [i, pts[i].x],
		)
		assert_true(
			pts[i].y >= 432.0 - epsilon and pts[i].y <= 756.0 + epsilon,
			"Point %d y=%.1f out of corridor" % [i, pts[i].y],
		)

# === Degenerate Cases ===


func test_z_route_degenerate_same_axis():
	# E->W with same Y -> should produce straight line
	var pts := _router._build_rectilinear(
		Vector2(100, 300),
		"E",
		Vector2(500, 300),
		"W",
		50,
		24.0,
	)
	assert_eq(pts.size(), 2, "Same-axis should be straight line")
	assert_almost_eq(pts[0].x, 100.0, 1.0)
	assert_almost_eq(pts[1].x, 500.0, 1.0)


func test_coincident_points():
	var pts := _router._build_rectilinear(
		Vector2(200, 200),
		"E",
		Vector2(200, 200),
		"W",
		50,
		24.0,
	)
	assert_eq(pts.size(), 2, "Coincident points produce 2-point path")


func test_very_close_points():
	# Distance = 5, much less than default radius (18 for secondary)
	var pts := _router._build_rectilinear(
		Vector2(100, 100),
		"E",
		Vector2(105, 103),
		"W",
		50,
		24.0,
	)
	assert_true(pts.size() >= 2, "Close points should not crash")

# === All 16 Side Combos ===


func test_all_16_side_combos():
	var sides := ["N", "E", "S", "W"]
	for s0 in sides:
		for s3 in sides:
			var ports := {
				"a": HudPort.create("a", "n1", s0, 0.5, HudRole.NAV, 50),
				"b": HudPort.create("b", "n2", s3, 0.5, HudRole.NAV, 50),
			}
			var positions := { "a": Vector2(100, 100), "b": Vector2(500, 500) }
			var nets: Array[HudNet] = [
				HudNet.create("n", HudNet.KIND_DATAFLOW, 50, "a", "b"),
			]
			var results := _router.route_all(ports, positions, nets, { }, 24.0)
			assert_eq(results.size(), 1, "Sides %s->%s no result" % [s0, s3])
			var pts: PackedVector2Array = results[0]["segments"][0]["points"]
			assert_true(
				pts.size() >= 2,
				"Sides %s->%s too few points (%d)" % [s0, s3, pts.size()],
			)

# === Bus Branch Rectilinear ===


func test_bus_branch_rectilinear_segments():
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
	var segs: Array = results[0]["segments"]
	var branch1_pts: PackedVector2Array = segs[0]["points"]
	var branch2_pts: PackedVector2Array = segs[2]["points"]
	# Branches are E->W with same Y (port Y == bus entry Y for vert bus)
	# so they produce straight 2-point lines. Verify endpoints connect.
	assert_true(branch1_pts.size() >= 2, "Branch 1 needs at least 2 points")
	assert_true(branch2_pts.size() >= 2, "Branch 2 needs at least 2 points")
	assert_almost_eq(branch1_pts[0].x, 480.0, 1.0, "Branch 1 starts at port a")
	var b2_last := branch2_pts[branch2_pts.size() - 1]
	assert_almost_eq(b2_last.x, 2400.0, 1.0, "Branch 2 ends at port b")

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
