extends GutTest

var _glass_scene = preload("res://hud/primitives/glass_pane.tscn")


func _create_glass() -> GlassPane:
	var pane = _glass_scene.instantiate()
	pane.size = Vector2(480, 320)
	add_child_autofree(pane)
	return pane


func test_glass_instantiates():
	var pane = _create_glass()
	assert_not_null(pane)
	assert_true(pane is PanelContainer)


func test_glass_pass_through():
	var pane = _create_glass()
	assert_eq(pane.mouse_filter, Control.MOUSE_FILTER_PASS)


func test_glass_default_exports():
	var pane = _create_glass()
	assert_eq(pane.role, HudRole.NEUTRAL)
	assert_eq(pane.blur_lod, 2.0)
	assert_eq(pane.alpha, 0.33)
	assert_eq(pane.grain, 0.03)
	assert_eq(pane.blur_enabled, true)


func test_glass_has_shader_material():
	var pane = _create_glass()
	var body = pane.get_node("GlassBody")
	assert_not_null(body)
	assert_not_null(body.material)
	assert_true(body.material is ShaderMaterial)


func test_glass_content_container():
	var pane = _create_glass()
	var content = pane.get_content_container()
	assert_not_null(content)
	assert_true(content is MarginContainer)


func test_glass_role_change():
	var pane = _create_glass()
	pane.set_role(HudRole.NAV)
	assert_eq(pane.role, HudRole.NAV)


func test_glass_blur_toggle():
	var pane = _create_glass()
	pane.set_blur_enabled(false)
	assert_eq(pane.blur_enabled, false)
	var mat: ShaderMaterial = pane.get_node("GlassBody").material
	assert_eq(mat.get_shader_parameter("u_blur_lod"), 0.0)
	pane.set_blur_enabled(true)
	assert_eq(mat.get_shader_parameter("u_blur_lod"), 2.0)


func test_glass_port_positions():
	var pane = _create_glass()
	var ports = pane.get_port_positions()
	assert_true(ports.has("N"))
	assert_true(ports.has("S"))
	assert_true(ports.has("E"))
	assert_true(ports.has("W"))


func test_glass_set_alpha():
	var pane = _create_glass()
	pane.set_alpha(0.5)
	assert_eq(pane.alpha, 0.5)
	var mat: ShaderMaterial = pane.get_node("GlassBody").material
	assert_eq(mat.get_shader_parameter("u_alpha"), 0.5)


func test_glass_set_grain():
	var pane = _create_glass()
	pane.set_grain(0.1)
	assert_eq(pane.grain, 0.1)
	var mat: ShaderMaterial = pane.get_node("GlassBody").material
	assert_eq(mat.get_shader_parameter("u_grain"), 0.1)


func test_glass_back_buffer_copy():
	var pane = _create_glass()
	var bbc = pane.get_node("BackBufferCopy")
	assert_not_null(bbc)
	assert_true(bbc is BackBufferCopy)
	assert_eq(bbc.copy_mode, BackBufferCopy.COPY_MODE_RECT)
