@tool
extends Node3D

# L-System Preview Controller
# Builds line geometry from generator output and applies to MeshInstance3D

var mesh_instance: MeshInstance3D


func _ready() -> void:
	mesh_instance = $MeshInstance3D
	print("[LSystem Preview] Ready")


func update_preview(mesh_data: Dictionary) -> void:
	if not mesh_instance:
		mesh_instance = $MeshInstance3D

	if mesh_data.is_empty():
		print("[LSystem Preview] Empty mesh data")
		return

	var vertices = mesh_data.get("vertices", PackedVector3Array())
	if vertices.size() < 2:
		print("[LSystem Preview] Not enough vertices")
		return

	print("[LSystem Preview] Updating preview...")

	# Build ArrayMesh with PRIMITIVE_LINES
	var arrays = []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices

	var array_mesh = ArrayMesh.new()
	array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)

	# Unshaded material — lines have zero surface area for lighting
	var material = StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = mesh_data.get("line_color", Color(0.4, 0.8, 0.3, 1.0))
	array_mesh.surface_set_material(0, material)

	mesh_instance.mesh = array_mesh

	# Center geometry using bounds
	var bounds_min = mesh_data.get("bounds_min", Vector3.ZERO)
	var bounds_max = mesh_data.get("bounds_max", Vector3.ZERO)
	var center = (bounds_min + bounds_max) / 2.0
	mesh_instance.position = -center

	print("[LSystem Preview] Updated - ", mesh_data.get("segment_count", 0), " segments")
