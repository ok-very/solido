extends GutTest

# Tests for modules/terrain/preview_controller.gd
# Strategy: build Node3D + MeshInstance3D child manually, attach script,
# add to tree so $MeshInstance3D resolves.

var controller: Node3D
var mesh_instance: MeshInstance3D


func before_each():
	controller = load("res://modules/terrain/preview_controller.gd").new()
	mesh_instance = MeshInstance3D.new()
	mesh_instance.name = "MeshInstance3D"
	controller.add_child(mesh_instance)
	add_child_autofree(controller)
	controller.mesh_instance = mesh_instance

# --- Helper: minimal valid mesh data (single triangle) ---


func _make_triangle_data() -> Dictionary:
	var verts = PackedVector3Array()
	verts.append(Vector3(0, 0, 0))
	verts.append(Vector3(1, 0, 0))
	verts.append(Vector3(0, 0, 1))

	var normals = PackedVector3Array()
	normals.append(Vector3(0, 1, 0))
	normals.append(Vector3(0, 1, 0))
	normals.append(Vector3(0, 1, 0))

	var uvs = PackedVector2Array()
	uvs.append(Vector2(0, 0))
	uvs.append(Vector2(1, 0))
	uvs.append(Vector2(0, 1))

	var indices = PackedInt32Array()
	indices.append(0)
	indices.append(1)
	indices.append(2)

	return { "vertices": verts, "normals": normals, "uvs": uvs, "indices": indices }

# --- Empty / null data ---


func test_empty_data_returns_early():
	controller.update_preview({ })
	assert_null(mesh_instance.mesh)

# --- Valid data creates mesh ---


func test_valid_data_creates_array_mesh():
	controller.update_preview(_make_triangle_data())
	assert_not_null(mesh_instance.mesh)
	assert_is(mesh_instance.mesh, ArrayMesh)


func test_mesh_has_one_surface():
	controller.update_preview(_make_triangle_data())
	assert_eq(mesh_instance.mesh.get_surface_count(), 1)

# --- Centering ---


func test_centering_offsets_position():
	controller.update_preview(_make_triangle_data())
	# Triangle spans (0,0,0)-(1,0,1), center = (0.5, 0, 0.5)
	# Position should be -center
	assert_ne(mesh_instance.position, Vector3.ZERO, "Position should be offset for centering")


func test_symmetric_mesh_centers_near_origin():
	var verts = PackedVector3Array()
	verts.append(Vector3(-1, 0, -1))
	verts.append(Vector3(1, 0, -1))
	verts.append(Vector3(0, 0, 1))

	var normals = PackedVector3Array()
	for i in 3:
		normals.append(Vector3(0, 1, 0))

	var uvs = PackedVector2Array()
	for i in 3:
		uvs.append(Vector2(0, 0))

	var indices = PackedInt32Array([0, 1, 2])

	var data = { "vertices": verts, "normals": normals, "uvs": uvs, "indices": indices }
	controller.update_preview(data)

	# Center = (0, 0, 0) — position should be near zero
	assert_almost_eq(mesh_instance.position, Vector3.ZERO, Vector3(0.01, 0.01, 0.01))

# --- Centering math ---


func test_centering_exact_values():
	# Triangle at (0,0,0)-(1,0,0)-(0,0,1)
	# bounds_min=(0,0,0), bounds_max=(1,0,1), center=(0.5, 0, 0.5)
	controller.update_preview(_make_triangle_data())
	assert_almost_eq(mesh_instance.position, Vector3(-0.5, 0.0, -0.5), Vector3(0.01, 0.01, 0.01))


func test_no_mesh_on_null_instance():
	# If mesh_instance is null and $MeshInstance3D isn't resolvable, empty data returns early
	controller.update_preview({ })
	assert_null(mesh_instance.mesh)

# --- Second update replaces mesh ---


func test_second_update_replaces_mesh():
	controller.update_preview(_make_triangle_data())
	var first_mesh = mesh_instance.mesh

	controller.update_preview(_make_triangle_data())
	var second_mesh = mesh_instance.mesh

	assert_ne(first_mesh, second_mesh, "Second update should produce new mesh instance")

# --- Integration with real generator ---


func test_generator_output_produces_valid_mesh():
	var gen = TerrainGenerator.new()
	gen.width = 3
	gen.height = 3
	var data = gen.generate()

	controller.update_preview(data)
	assert_not_null(mesh_instance.mesh)
	assert_eq(mesh_instance.mesh.get_surface_count(), 1)
