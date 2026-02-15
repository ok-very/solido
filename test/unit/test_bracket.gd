extends GutTest

var _bracket_scene = preload("res://hud/primitives/bracket.tscn")


func _create_bracket(bracket_style: int = Bracket.SQUARE_BRACKET) -> Bracket:
	var bracket = _bracket_scene.instantiate()
	bracket.style = bracket_style
	bracket.size = Vector2(48, 120)
	add_child_autofree(bracket)
	return bracket


func test_bracket_instantiates():
	var bracket = _create_bracket()
	assert_not_null(bracket)
	assert_true(bracket is Control)


func test_bracket_non_interactive():
	var bracket = _create_bracket()
	assert_eq(bracket.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_bracket_default_exports():
	var bracket = _create_bracket()
	assert_eq(bracket.role, HudRole.NEUTRAL)
	assert_eq(bracket.style, Bracket.SQUARE_BRACKET)
	assert_eq(bracket.orientation, Bracket.LEFT)
	assert_eq(bracket.arm_length_u, 2.0)
	assert_eq(bracket.stroke_width_px, 2.0)


func test_bracket_square_creates_lines():
	var bracket = _create_bracket(Bracket.SQUARE_BRACKET)
	var lines_node = bracket.get_node("BracketLines")
	assert_not_null(lines_node)
	assert_eq(lines_node.get_child_count(), 3)


func test_bracket_angle_creates_lines():
	var bracket = _create_bracket(Bracket.ANGLE_BRACKET)
	var lines_node = bracket.get_node("BracketLines")
	assert_eq(lines_node.get_child_count(), 2)


func test_bracket_tick_group():
	var bracket = _bracket_scene.instantiate()
	bracket.style = Bracket.TICK_GROUP
	bracket.tick_count = 5
	bracket.size = Vector2(48, 120)
	add_child_autofree(bracket)
	var lines_node = bracket.get_node("BracketLines")
	# 1 spine + 5 ticks = 6 lines
	assert_eq(lines_node.get_child_count(), 6)


func test_bracket_role_change():
	var bracket = _create_bracket()
	bracket.set_role(HudRole.TELEMETRY)
	assert_eq(bracket.role, HudRole.TELEMETRY)
	var lines_node = bracket.get_node("BracketLines")
	for child in lines_node.get_children():
		if child is Line2D:
			assert_eq(child.default_color, HudTheme.palette.get_rim(HudRole.TELEMETRY))


func test_bracket_line_properties():
	var bracket = _create_bracket()
	var lines_node = bracket.get_node("BracketLines")
	for child in lines_node.get_children():
		if child is Line2D:
			assert_eq(child.width, 2.0)
			assert_true(child.antialiased)
