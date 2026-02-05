@tool
extends Resource
class_name TerrainGenerator

# Terrain Generator Resource
# Procedurally generates terrain mesh data based on parameters
# Exports as Resource for serialization to library

# === PARAMETERS ===
# These match the schema.toml definitions

@export var width: int = 10
@export var height: int = 10
@export var scale: float = 1.0
@export var height_multiplier: float = 2.0
@export var octaves: int = 4
@export var persistence: float = 0.5
@export var lacunarity: float = 2.0
@export var seed_value: int = 0
@export var terrain_type: String = "plains"

# Generated data
var mesh_data: Dictionary = {}

func _init() -> void:
	print("[Terrain Generator] Initialized")

func generate() -> Dictionary:
	print("[Terrain Generator] Generating terrain...")
	print("  Size: ", width, "x", height)
	print("  Scale: ", scale)
	print("  Height Multiplier: ", height_multiplier)
	print("  Terrain Type: ", terrain_type)
	
	# Generate height map
	var heights = _generate_height_map()
	
	# Build mesh data
	mesh_data = _build_mesh(heights)
	
	print("[Terrain Generator] Generation complete")
	return mesh_data

func _generate_height_map() -> Array:
	var heights = []
	var noise = FastNoiseLite.new()
	
	# Configure noise
	noise.seed = seed_value
	noise.noise_type = FastNoiseLite.TYPE_PERLIN
	noise.frequency = 0.1 / scale
	noise.fractal_octaves = octaves
	noise.fractal_lacunarity = lacunarity
	noise.fractal_gain = persistence
	
	# Apply terrain type modifications
	match terrain_type:
		"mountains":
			noise.fractal_type = FastNoiseLite.FRACTAL_RIDGED
		"hills":
			noise.fractal_type = FastNoiseLite.FRACTAL_FBM
			noise.frequency = 0.05 / scale
		"plains":
			noise.fractal_type = FastNoiseLite.FRACTAL_FBM
			noise.frequency = 0.15 / scale
		"canyon":
			noise.fractal_type = FastNoiseLite.FRACTAL_PING_PONG
	
	# Generate height values
	for y in range(height):
		var row = []
		for x in range(width):
			var height_value = noise.get_noise_2d(x, y)
			
			# Normalize from [-1, 1] to [0, 1]
			height_value = (height_value + 1.0) / 2.0
			
			# Apply height multiplier
			height_value *= height_multiplier
			
			row.append(height_value)
		heights.append(row)
	
	return heights

func _build_mesh(heights: Array) -> Dictionary:
	var vertices = PackedVector3Array()
	var indices = PackedInt32Array()
	var normals = PackedVector3Array()
	var uvs = PackedVector2Array()
	
	# Generate vertices
	for y in range(height):
		for x in range(width):
			var h = heights[y][x]
			vertices.append(Vector3(x, h, y))
			
			# UV coordinates
			var u = float(x) / float(width - 1)
			var v = float(y) / float(height - 1)
			uvs.append(Vector2(u, v))
	
	# Generate indices (triangles)
	for y in range(height - 1):
		for x in range(width - 1):
			var top_left = y * width + x
			var top_right = top_left + 1
			var bottom_left = (y + 1) * width + x
			var bottom_right = bottom_left + 1
			
			# First triangle
			indices.append(top_left)
			indices.append(bottom_left)
			indices.append(top_right)
			
			# Second triangle
			indices.append(top_right)
			indices.append(bottom_left)
			indices.append(bottom_right)
	
	# Calculate normals
	normals = _calculate_normals(vertices, indices)
	
	return {
		"vertices": vertices,
		"indices": indices,
		"normals": normals,
		"uvs": uvs
	}

func _calculate_normals(vertices: PackedVector3Array, indices: PackedInt32Array) -> PackedVector3Array:
	var normals = PackedVector3Array()
	normals.resize(vertices.size())
	
	# Initialize to zero
	for i in range(vertices.size()):
		normals[i] = Vector3.ZERO
	
	# Accumulate face normals
	for i in range(0, indices.size(), 3):
		var i0 = indices[i]
		var i1 = indices[i + 1]
		var i2 = indices[i + 2]
		
		var v0 = vertices[i0]
		var v1 = vertices[i1]
		var v2 = vertices[i2]
		
		var edge1 = v1 - v0
		var edge2 = v2 - v0
		var face_normal = edge1.cross(edge2).normalized()
		
		normals[i0] += face_normal
		normals[i1] += face_normal
		normals[i2] += face_normal
	
	# Normalize
	for i in range(normals.size()):
		normals[i] = normals[i].normalized()
	
	return normals

func get_mesh() -> ArrayMesh:
	# Convert mesh_data to ArrayMesh for rendering
	if mesh_data.is_empty():
		generate()
	
	var arrays = []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = mesh_data["vertices"]
	arrays[Mesh.ARRAY_NORMAL] = mesh_data["normals"]
	arrays[Mesh.ARRAY_TEX_UV] = mesh_data["uvs"]
	arrays[Mesh.ARRAY_INDEX] = mesh_data["indices"]
	
	var array_mesh = ArrayMesh.new()
	array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	
	return array_mesh
