extends GutTest

var _connector_scene = preload("res://hud/primitives/spline_connector.tscn")


func _create_connector() -> SplineConnector:
	var conn = _connector_scene.instantiate()
	conn.size = Vector2(480, 320)
	add_child_autofree(conn)
	return conn


func test_connector_instantiates():
	var conn = _create_connector()
	assert_not_null(conn)
	assert_true(conn is Control)


func test_connector_default_exports():
	var conn = _create_connector()
	assert_eq(conn.role, HudRole.NEUTRAL)
	assert_eq(conn.importance, 50)
	assert_eq(conn.start_cap_style, SplineConnector.CAP_NONE)
	assert_eq(conn.end_cap_style, SplineConnector.CAP_NONE)
	assert_eq(conn.scanline_speed, 0.0)


func test_connector_has_shader_material():
	var conn = _create_connector()
	var line = conn.get_node("ConnectorLine")
	assert_not_null(line)
	assert_not_null(line.material)
	assert_true(line.material is ShaderMaterial)


func test_connector_set_curve_points():
	var conn = _create_connector()
	conn.set_curve_points(
		Vector2(10, 50),
		Vector2(100, 50),
		Vector2(200, 150),
		Vector2(300, 150)
	)
	var points = conn.get_baked_points()
	assert_true(points.size() >= 2)


func test_connector_start_end_positions():
	var conn = _create_connector()
	conn.set_curve_points(
		Vector2(10, 50),
		Vector2(100, 50),
		Vector2(200, 150),
		Vector2(300, 150)
	)
	var start_pos = conn.get_start_position()
	var end_pos = conn.get_end_position()
	assert_eq(start_pos, Vector2(10, 50))
	assert_true(end_pos.distance_to(Vector2(300, 150)) < 1.0)


func test_connector_set_points_from_bake():
	var conn = _create_connector()
	var points = PackedVector2Array([Vector2(0, 0), Vector2(100, 0), Vector2(200, 50)])
	conn.set_points_from_bake(points)
	assert_eq(conn.get_baked_points().size(), 3)


func test_connector_bus_segment():
	var conn = _create_connector()
	conn.set_bus_segment(Vector2(0, 0), Vector2(200, 0))
	assert_eq(conn.get_baked_points().size(), 2)
	assert_eq(conn.get_start_position(), Vector2(0, 0))
	assert_eq(conn.get_end_position(), Vector2(200, 0))


func test_connector_role_change():
	var conn = _create_connector()
	conn.set_role(HudRole.TELEMETRY)
	assert_eq(conn.role, HudRole.TELEMETRY)


func test_connector_importance_primary():
	var conn = _create_connector()
	conn.set_importance(90)
	assert_eq(conn.importance, 90)
	var unit_px: float = HudTheme.unit_px
	var line = conn.get_node("ConnectorLine")
	assert_eq(line.width, 2.5 * unit_px)


func test_connector_importance_tertiary():
	var conn = _create_connector()
	conn.set_importance(30)
	assert_eq(conn.importance, 30)
	var unit_px: float = HudTheme.unit_px
	var line = conn.get_node("ConnectorLine")
	assert_eq(line.width, 0.75 * unit_px)


func test_connector_bounding_rect():
	var conn = _create_connector()
	conn.set_bus_segment(Vector2(10, 10), Vector2(200, 10))
	var bounds = conn.get_bounding_rect()
	assert_true(bounds.size.x > 0.0)
	assert_true(bounds.size.y > 0.0)


func test_connector_hit_area_exists():
	var conn = _create_connector()
	var hit = conn.get_node("HitArea")
	assert_not_null(hit)
	assert_eq(hit.mouse_filter, Control.MOUSE_FILTER_STOP)
