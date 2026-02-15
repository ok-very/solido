extends GutTest

# Tests for modules/terrain/generator.gd

var gen: TerrainGenerator


func before_each():
	gen = TerrainGenerator.new()

# --- Default State ---


func test_default_width():
	assert_eq(gen.width, 10)


func test_default_height():
	assert_eq(gen.height, 10)


func test_default_scale():
	assert_eq(gen.scale, 1.0)


func test_default_height_multiplier():
	assert_eq(gen.height_multiplier, 2.0)


func test_default_terrain_type():
	assert_eq(gen.terrain_type, "plains")


func test_mesh_data_starts_empty():
	assert_true(gen.mesh_data.is_empty())

# --- Generate Output Structure ---


func test_generate_returns_dict():
	var result = gen.generate()
	assert_typeof(result, TYPE_DICTIONARY)


func test_generate_has_vertices():
	var result = gen.generate()
	assert_has(result, "vertices")


func test_generate_has_indices():
	var result = gen.generate()
	assert_has(result, "indices")


func test_generate_has_normals():
	var result = gen.generate()
	assert_has(result, "normals")


func test_generate_has_uvs():
	var result = gen.generate()
	assert_has(result, "uvs")

# --- Vertex Count ---


func test_vertex_count_matches_grid():
	gen.width = 5
	gen.height = 5
	var result = gen.generate()
	assert_eq(result["vertices"].size(), 25) # 5 * 5


func test_vertex_count_rectangular():
	gen.width = 4
	gen.height = 3
	var result = gen.generate()
	assert_eq(result["vertices"].size(), 12) # 4 * 3

# --- Index Count ---


func test_index_count():
	gen.width = 5
	gen.height = 5
	var result = gen.generate()
	# (width-1) * (height-1) * 2 triangles * 3 indices
	var expected = 4 * 4 * 2 * 3
	assert_eq(result["indices"].size(), expected)

# --- Normal Count ---


func test_normals_match_vertices():
	gen.width = 5
	gen.height = 5
	var result = gen.generate()
	assert_eq(result["normals"].size(), result["vertices"].size())

# --- UV Count ---


func test_uvs_match_vertices():
	gen.width = 5
	gen.height = 5
	var result = gen.generate()
	assert_eq(result["uvs"].size(), result["vertices"].size())

# --- UV Bounds ---


func test_uv_corners():
	gen.width = 3
	gen.height = 3
	var result = gen.generate()
	var uvs = result["uvs"]
	# First vertex (0,0) should have UV (0, 0)
	assert_almost_eq(uvs[0], Vector2(0, 0), Vector2(0.01, 0.01))
	# Last vertex (2,2) should have UV (1, 1)
	assert_almost_eq(uvs[8], Vector2(1, 1), Vector2(0.01, 0.01))

# --- Height Values ---


func test_heights_within_range():
	gen.width = 10
	gen.height = 10
	gen.height_multiplier = 2.0
	var result = gen.generate()
	for v in result["vertices"]:
		# Heights normalized to [0,1] then multiplied by height_multiplier
		assert_true(v.y >= 0.0, "Height should be >= 0")
		assert_true(v.y <= gen.height_multiplier, "Height should be <= multiplier")

# --- Terrain Types ---


func test_plains_generates():
	gen.terrain_type = "plains"
	var result = gen.generate()
	assert_false(result.is_empty())


func test_hills_generates():
	gen.terrain_type = "hills"
	var result = gen.generate()
	assert_false(result.is_empty())


func test_mountains_generates():
	gen.terrain_type = "mountains"
	var result = gen.generate()
	assert_false(result.is_empty())


func test_canyon_generates():
	gen.terrain_type = "canyon"
	var result = gen.generate()
	assert_false(result.is_empty())

# --- Determinism ---


func test_same_seed_same_output():
	gen.seed_value = 42
	gen.width = 5
	gen.height = 5
	var result1 = gen.generate()

	var gen2 = TerrainGenerator.new()
	gen2.seed_value = 42
	gen2.width = 5
	gen2.height = 5
	var result2 = gen2.generate()

	assert_eq(result1["vertices"], result2["vertices"])


func test_different_seed_different_output():
	gen.seed_value = 42
	gen.width = 5
	gen.height = 5
	var result1 = gen.generate()

	gen.seed_value = 99
	var result2 = gen.generate()

	assert_ne(result1["vertices"], result2["vertices"])

# --- ArrayMesh ---


func test_get_mesh_returns_array_mesh():
	gen.width = 3
	gen.height = 3
	var mesh = gen.get_mesh()
	assert_not_null(mesh)
	assert_is(mesh, ArrayMesh)


func test_get_mesh_auto_generates():
	gen.width = 3
	gen.height = 3
	# Don't call generate() first — get_mesh should call it
	var mesh = gen.get_mesh()
	assert_not_null(mesh)
	assert_false(gen.mesh_data.is_empty())


func test_get_mesh_has_surface():
	gen.width = 3
	gen.height = 3
	var mesh = gen.get_mesh()
	assert_eq(mesh.get_surface_count(), 1)

# --- Minimum Grid ---


func test_minimum_grid_2x2():
	gen.width = 2
	gen.height = 2
	var result = gen.generate()
	assert_eq(result["vertices"].size(), 4)
	assert_eq(result["indices"].size(), 6) # 1*1*2*3

# --- Normal Validity ---


func test_normals_are_normalized():
	gen.width = 5
	gen.height = 5
	var result = gen.generate()
	for n in result["normals"]:
		# Normalized vectors have length ~1.0
		assert_almost_eq(n.length(), 1.0, 0.01, "Normal should be unit length")
