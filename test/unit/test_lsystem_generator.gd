extends GutTest

# Tests for modules/lsystem/generator.gd

var gen: LSystemGenerator


func before_each():
	gen = LSystemGenerator.new()

# --- Default State ---


func test_default_rule_preset():
	assert_eq(gen.rule_preset, "tree")


func test_default_iterations():
	assert_eq(gen.iterations, 4)


func test_default_angle():
	assert_eq(gen.angle, 25.0)


func test_default_segment_length():
	assert_eq(gen.segment_length, 1.0)


func test_default_length_decay():
	assert_eq(gen.length_decay, 1.0)


func test_default_line_color():
	assert_eq(gen.line_color, Color(0.4, 0.8, 0.3, 1.0))


func test_default_seed_value():
	assert_eq(gen.seed_value, 0)


func test_mesh_data_starts_empty():
	assert_true(gen.mesh_data.is_empty())

# --- Generate Output Structure ---


func test_generate_returns_dict():
	gen.iterations = 1
	var result = gen.generate()
	assert_typeof(result, TYPE_DICTIONARY)


func test_generate_has_vertices():
	gen.iterations = 1
	var result = gen.generate()
	assert_has(result, "vertices")


func test_generate_has_segment_count():
	gen.iterations = 1
	var result = gen.generate()
	assert_has(result, "segment_count")


func test_generate_has_bounds_min():
	gen.iterations = 1
	var result = gen.generate()
	assert_has(result, "bounds_min")


func test_generate_has_bounds_max():
	gen.iterations = 1
	var result = gen.generate()
	assert_has(result, "bounds_max")


func test_generate_has_line_color():
	gen.iterations = 1
	var result = gen.generate()
	assert_has(result, "line_color")

# --- String Rewriting ---


func test_rewrite_zero_iterations():
	var result = gen._rewrite("F", { "F": "F+F" }, 0)
	assert_eq(result, "F")


func test_rewrite_one_iteration():
	var result = gen._rewrite("F", { "F": "F+F" }, 1)
	assert_eq(result, "F+F")


func test_rewrite_two_iterations():
	var result = gen._rewrite("F", { "F": "F+F" }, 2)
	assert_eq(result, "F+F+F+F")


func test_rewrite_preserves_unknown_chars():
	var result = gen._rewrite("F+G", { "F": "FF" }, 1)
	assert_eq(result, "FF+G")


func test_rewrite_multiple_rules():
	var result = gen._rewrite("F-G", { "F": "FG", "G": "GF" }, 1)
	assert_eq(result, "FG-GF")

# --- Turtle Interpretation ---


func test_single_F_produces_two_vertices():
	gen.iterations = 0
	gen.rule_preset = "tree"
	var result = gen.generate()
	# Axiom "F" at 0 iterations = single segment = 2 vertices
	assert_eq(result["vertices"].size(), 2)


func test_single_F_starts_at_origin():
	gen.iterations = 0
	var result = gen.generate()
	assert_eq(result["vertices"][0], Vector3.ZERO)


func test_single_F_goes_up():
	gen.iterations = 0
	gen.segment_length = 1.0
	var result = gen.generate()
	var end = result["vertices"][1]
	assert_almost_eq(end.y, 1.0, 0.001, "Should move up by segment_length")
	assert_almost_eq(end.x, 0.0, 0.001, "Should not move horizontally")


func test_angle_turns_direction():
	# "F+F" with 90 degree angle: first segment up, second segment left
	gen.angle = 90.0
	gen.iterations = 0
	# Manually test with known sentence
	var verts = gen._interpret("F+F")
	# First segment: (0,0,0) -> (0,1,0)
	assert_almost_eq(verts[0], Vector3.ZERO, Vector3(0.001, 0.001, 0.001))
	assert_almost_eq(verts[1], Vector3(0, 1, 0), Vector3(0.001, 0.001, 0.001))
	# After +90 around BACK axis, direction rotates left in XY plane
	# Second segment starts where first ended
	assert_almost_eq(verts[2], Vector3(0, 1, 0), Vector3(0.001, 0.001, 0.001))
	# Endpoint should be to the left (negative X)
	assert_true(verts[3].x < -0.5, "Should turn left after + rotation")


func test_bracket_push_pop():
	# "F[+F]F" should produce 3 segments
	# First F: up from origin
	# [+F]: branch left from top of first F
	# ]F: return to top of first F, continue up
	gen.angle = 90.0
	var verts = gen._interpret("F[+F]F")
	assert_eq(verts.size(), 6, "Should have 3 segments (6 vertices)")
	# Third segment should start where first ended (pop restores position)
	assert_almost_eq(verts[4], verts[1], Vector3(0.001, 0.001, 0.001))


func test_both_F_and_G_draw():
	var verts = gen._interpret("FG")
	assert_eq(verts.size(), 4, "Both F and G should produce segments")

# --- Segment Math ---


func test_segment_count_matches_vertices():
	gen.iterations = 2
	var result = gen.generate()
	assert_eq(result["segment_count"], result["vertices"].size() / 2)


func test_vertices_always_even():
	gen.iterations = 3
	var result = gen.generate()
	assert_eq(result["vertices"].size() % 2, 0, "Vertex count must be even for line pairs")


func test_more_iterations_more_segments():
	gen.iterations = 1
	var result1 = gen.generate()
	gen.iterations = 3
	var result2 = gen.generate()
	assert_true(
		result2["segment_count"] > result1["segment_count"],
		"More iterations should produce more segments",
	)

# --- Preset Coverage ---


func test_tree_generates():
	gen.rule_preset = "tree"
	gen.iterations = 2
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)


func test_bush_generates():
	gen.rule_preset = "bush"
	gen.iterations = 2
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)


func test_koch_curve_generates():
	gen.rule_preset = "koch_curve"
	gen.iterations = 2
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)


func test_sierpinski_generates():
	gen.rule_preset = "sierpinski"
	gen.iterations = 2
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)


func test_dragon_curve_generates():
	gen.rule_preset = "dragon_curve"
	gen.iterations = 2
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)

# --- Bounds ---


func test_bounds_contain_all_vertices():
	gen.iterations = 3
	var result = gen.generate()
	var bmin = result["bounds_min"]
	var bmax = result["bounds_max"]
	for v in result["vertices"]:
		assert_true(v.x >= bmin.x - 0.001, "Vertex x below bounds_min")
		assert_true(v.y >= bmin.y - 0.001, "Vertex y below bounds_min")
		assert_true(v.z >= bmin.z - 0.001, "Vertex z below bounds_min")
		assert_true(v.x <= bmax.x + 0.001, "Vertex x above bounds_max")
		assert_true(v.y <= bmax.y + 0.001, "Vertex y above bounds_max")
		assert_true(v.z <= bmax.z + 0.001, "Vertex z above bounds_max")


func test_bounds_min_less_than_max():
	gen.iterations = 3
	var result = gen.generate()
	assert_true(result["bounds_min"].x <= result["bounds_max"].x)
	assert_true(result["bounds_min"].y <= result["bounds_max"].y)
	assert_true(result["bounds_min"].z <= result["bounds_max"].z)


func test_single_segment_bounds():
	gen.iterations = 0
	gen.segment_length = 2.0
	var result = gen.generate()
	assert_almost_eq(result["bounds_min"], Vector3.ZERO, Vector3(0.001, 0.001, 0.001))
	assert_almost_eq(
		result["bounds_max"],
		Vector3(0, 2.0, 0),
		Vector3(0.001, 0.001, 0.001),
	)

# --- Determinism ---


func test_same_params_same_output():
	gen.iterations = 3
	gen.rule_preset = "tree"
	gen.angle = 30.0
	var result1 = gen.generate()

	var gen2 = LSystemGenerator.new()
	gen2.iterations = 3
	gen2.rule_preset = "tree"
	gen2.angle = 30.0
	var result2 = gen2.generate()

	assert_eq(result1["vertices"], result2["vertices"])


func test_different_preset_different_output():
	gen.iterations = 3
	gen.rule_preset = "tree"
	var result1 = gen.generate()

	gen.rule_preset = "koch_curve"
	gen.angle = 90.0
	var result2 = gen.generate()

	assert_ne(result1["vertices"], result2["vertices"])

# --- Edge Cases ---


func test_min_iterations():
	gen.iterations = 1
	var result = gen.generate()
	assert_true(result["vertices"].size() > 0)


func test_invalid_preset_falls_back():
	gen.rule_preset = "nonexistent"
	gen.iterations = 1
	var result = gen.generate()
	# Should fall back to "tree" preset and still generate
	assert_true(result["vertices"].size() > 0)


func test_length_decay_shrinks_branches():
	gen.rule_preset = "tree"
	gen.iterations = 2
	gen.length_decay = 0.5
	gen.segment_length = 2.0
	var result = gen.generate()
	# Should still produce valid output
	assert_true(result["vertices"].size() > 0)
	assert_true(result["segment_count"] > 0)

# --- ArrayMesh ---


func test_get_mesh_returns_array_mesh():
	gen.iterations = 2
	var mesh = gen.get_mesh()
	assert_not_null(mesh)
	assert_is(mesh, ArrayMesh)


func test_get_mesh_auto_generates():
	gen.iterations = 2
	var mesh = gen.get_mesh()
	assert_not_null(mesh)
	assert_false(gen.mesh_data.is_empty())


func test_get_mesh_has_surface():
	gen.iterations = 2
	var mesh = gen.get_mesh()
	assert_eq(mesh.get_surface_count(), 1)
