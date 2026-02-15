extends GutTest

var _elbow_scene = preload("res://hud/primitives/lcars_elbow.tscn")


func _create_elbow(rot: int = LcarsElbow.TL) -> LcarsElbow:
	var elbow = _elbow_scene.instantiate()
	elbow.rotation_index = rot
	elbow.size = Vector2(288, 240)
	add_child_autofree(elbow)
	return elbow


func test_elbow_instantiates():
	var elbow = _create_elbow()
	assert_not_null(elbow)
	assert_true(elbow is Control)


func test_elbow_non_interactive():
	var elbow = _create_elbow()
	assert_eq(elbow.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_elbow_default_exports():
	var elbow = _create_elbow()
	assert_eq(elbow.role, HudRole.NEUTRAL)
	assert_eq(elbow.rotation_index, LcarsElbow.TL)
	assert_eq(elbow.outer_radius_u, 4.0)
	assert_eq(elbow.inner_radius_u, 2.0)
	assert_eq(elbow.arm_h_thickness_u, 3.0)
	assert_eq(elbow.arm_v_thickness_u, 3.0)
	assert_eq(elbow.arm_h_length_u, 8.0)
	assert_eq(elbow.arm_v_length_u, 6.0)


func test_elbow_has_shader_material():
	var elbow = _create_elbow()
	var body = elbow.get_node("ElbowBody")
	assert_not_null(body)
	assert_not_null(body.material)
	assert_true(body.material is ShaderMaterial)


func test_elbow_rotation_tl():
	var elbow = _create_elbow(LcarsElbow.TL)
	var body = elbow.get_node("ElbowBody")
	var mat: ShaderMaterial = body.material
	assert_eq(mat.get_shader_parameter("u_rotation"), LcarsElbow.TL)


func test_elbow_rotation_br():
	var elbow = _create_elbow(LcarsElbow.BR)
	var body = elbow.get_node("ElbowBody")
	var mat: ShaderMaterial = body.material
	assert_eq(mat.get_shader_parameter("u_rotation"), LcarsElbow.BR)


func test_elbow_role_change():
	var elbow = _create_elbow()
	elbow.set_role(HudRole.EDIT)
	assert_eq(elbow.role, HudRole.EDIT)


func test_elbow_port_positions():
	var elbow = _create_elbow(LcarsElbow.TL)
	var ports = elbow.get_port_positions()
	assert_true(ports.has("H"))
	assert_true(ports.has("V"))


func test_elbow_h_attachment_edge():
	var elbow = _create_elbow(LcarsElbow.TL)
	var edge = elbow.get_h_attachment_edge()
	assert_true(edge.size.y > 0.0)


func test_elbow_v_attachment_edge():
	var elbow = _create_elbow(LcarsElbow.TL)
	var edge = elbow.get_v_attachment_edge()
	assert_true(edge.size.x > 0.0)


func test_elbow_all_rotations():
	for rot in [LcarsElbow.TL, LcarsElbow.TR, LcarsElbow.BR, LcarsElbow.BL]:
		var elbow = _elbow_scene.instantiate()
		elbow.rotation_index = rot
		elbow.size = Vector2(288, 240)
		add_child_autofree(elbow)
		var mat: ShaderMaterial = elbow.get_node("ElbowBody").material
		assert_eq(mat.get_shader_parameter("u_rotation"), rot)


func test_elbow_minimum_size():
	var elbow = _create_elbow()
	assert_true(elbow.custom_minimum_size.x > 0.0)
	assert_true(elbow.custom_minimum_size.y > 0.0)
