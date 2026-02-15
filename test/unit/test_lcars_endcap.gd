extends GutTest

var _endcap_scene = preload("res://hud/primitives/lcars_endcap.tscn")


func _create_endcap(dir: int = LcarsEndcap.LEFT, sty: int = LcarsEndcap.HALF_PILL) -> LcarsEndcap:
	var endcap = _endcap_scene.instantiate()
	endcap.direction = dir
	endcap.style = sty
	endcap.size = Vector2(96, 72)
	add_child_autofree(endcap)
	return endcap


func test_endcap_instantiates():
	var endcap = _create_endcap()
	assert_not_null(endcap)
	assert_true(endcap is Control)


func test_endcap_non_interactive():
	var endcap = _create_endcap()
	assert_eq(endcap.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_endcap_default_exports():
	var endcap = _create_endcap()
	assert_eq(endcap.role, HudRole.NEUTRAL)
	assert_eq(endcap.style, LcarsEndcap.HALF_PILL)
	assert_eq(endcap.direction, LcarsEndcap.LEFT)
	assert_eq(endcap.thickness_u, 3.0)
	assert_eq(endcap.length_u, 4.0)
	assert_eq(endcap.pill_radius_u, 0.0)


func test_endcap_has_shader_material():
	var endcap = _create_endcap()
	var body = endcap.get_node("EndcapBody")
	assert_not_null(body)
	assert_not_null(body.material)
	assert_true(body.material is ShaderMaterial)


func test_endcap_direction_left():
	var endcap = _create_endcap(LcarsEndcap.LEFT)
	var body = endcap.get_node("EndcapBody")
	var mat: ShaderMaterial = body.material
	assert_eq(mat.get_shader_parameter("u_direction"), LcarsEndcap.LEFT)


func test_endcap_direction_right():
	var endcap = _create_endcap(LcarsEndcap.RIGHT)
	var body = endcap.get_node("EndcapBody")
	var mat: ShaderMaterial = body.material
	assert_eq(mat.get_shader_parameter("u_direction"), LcarsEndcap.RIGHT)


func test_endcap_stepped_style():
	var endcap = _create_endcap(LcarsEndcap.LEFT, LcarsEndcap.STEPPED)
	var body = endcap.get_node("EndcapBody")
	var mat: ShaderMaterial = body.material
	assert_eq(mat.get_shader_parameter("u_style"), LcarsEndcap.STEPPED)


func test_endcap_role_change():
	var endcap = _create_endcap()
	endcap.set_role(HudRole.TELEMETRY)
	assert_eq(endcap.role, HudRole.TELEMETRY)


func test_endcap_port_positions():
	var endcap = _create_endcap()
	var ports = endcap.get_port_positions()
	assert_true(ports.has("N"))
	assert_true(ports.has("S"))
	assert_true(ports.has("E"))
	assert_true(ports.has("W"))


func test_endcap_attachment_edge_left():
	var endcap = _create_endcap(LcarsEndcap.LEFT)
	var edge = endcap.get_attachment_edge()
	# For LEFT direction, attachment (flat) edge is on the right
	assert_true(edge.size.x == 0.0 or edge.size.y > 0.0)


func test_endcap_minimum_size():
	var endcap = _create_endcap()
	var unit_px: float = HudTheme.unit_px
	var expected_len := endcap.length_u * unit_px
	var expected_thick := endcap.thickness_u * unit_px
	# LEFT direction: width = length, height = thickness
	assert_eq(endcap.custom_minimum_size.x, expected_len)
	assert_eq(endcap.custom_minimum_size.y, expected_thick)
