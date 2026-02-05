@tool
extends Node3D

# Preview Controller
# Updates the preview scene when terrain is regenerated

var mesh_instance: MeshInstance3D

func _ready() -> void:
	mesh_instance = $MeshInstance3D
	print("[Preview Controller] Ready")

func update_preview(mesh_data: Dictionary) -> void:
	if not mesh_instance:
		mesh_instance = $MeshInstance3D
	
	if mesh_data.is_empty():
		print("[Preview Controller] Empty mesh data")
		return
	
	print("[Preview Controller] Updating preview mesh...")
	
	# Build ArrayMesh from mesh_data
	var arrays = []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = mesh_data.get("vertices", PackedVector3Array())
	arrays[Mesh.ARRAY_NORMAL] = mesh_data.get("normals", PackedVector3Array())
	arrays[Mesh.ARRAY_TEX_UV] = mesh_data.get("uvs", PackedVector2Array())
	arrays[Mesh.ARRAY_INDEX] = mesh_data.get("indices", PackedInt32Array())
	
	var array_mesh = ArrayMesh.new()
	array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	
	# Apply to mesh instance
	mesh_instance.mesh = array_mesh
	
	# Center the terrain
	var vertices = mesh_data["vertices"]
	if vertices.size() > 0:
		var bounds_min = vertices[0]
		var bounds_max = vertices[0]
		
		for v in vertices:
			bounds_min.x = min(bounds_min.x, v.x)
			bounds_min.y = min(bounds_min.y, v.y)
			bounds_min.z = min(bounds_min.z, v.z)
			bounds_max.x = max(bounds_max.x, v.x)
			bounds_max.y = max(bounds_max.y, v.y)
			bounds_max.z = max(bounds_max.z, v.z)
		
		var center = (bounds_min + bounds_max) / 2.0
		mesh_instance.position = -center
	
	print("[Preview Controller] Preview updated")
