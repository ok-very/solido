@tool
extends Resource

class_name LSystemGenerator

# L-System Generator Resource
# String rewriting + turtle graphics to produce line geometry
# Presets define axiom + rules; turtle interprets the result in XY plane

# === PRESETS ===

const PRESETS: Dictionary = {
	"tree": {
		"axiom": "F",
		"rules": { "F": "FF+[+F-F-F]-[-F+F+F]" },
	},
	"bush": {
		"axiom": "F",
		"rules": { "F": "F[+F]F[-F][F]" },
	},
	"koch_curve": {
		"axiom": "F",
		"rules": { "F": "F+F-F-F+F" },
	},
	"sierpinski": {
		"axiom": "F-G-G",
		"rules": { "F": "F-G+F+G-F", "G": "GG" },
	},
	"dragon_curve": {
		"axiom": "F",
		"rules": { "F": "F+G", "G": "F-G" },
	},
}

# === PARAMETERS ===
# Match schema.toml definitions

@export var rule_preset: String = "tree"
@export var iterations: int = 4
@export var angle: float = 25.0
@export var segment_length: float = 1.0
@export var length_decay: float = 1.0
@export var line_color: Color = Color(0.4, 0.8, 0.3, 1.0)
@export var seed_value: int = 0

# Generated data
var mesh_data: Dictionary = { }


func _init() -> void:
	pass


func generate() -> Dictionary:
	var preset = PRESETS.get(rule_preset, PRESETS["tree"])
	var axiom: String = preset["axiom"]
	var rules: Dictionary = preset["rules"]

	# Step 1: String rewriting
	var sentence = _rewrite(axiom, rules, iterations)

	# Step 2: Turtle interpretation
	var vertices = _interpret(sentence)

	# Step 3: Compute bounds
	var bounds = _compute_bounds(vertices)

	mesh_data = {
		"vertices": vertices,
		"segment_count": vertices.size() / 2,
		"bounds_min": bounds[0],
		"bounds_max": bounds[1],
		"line_color": line_color,
	}

	return mesh_data


func _rewrite(axiom: String, rules: Dictionary, n: int) -> String:
	var current = axiom
	for i in range(n):
		var next = ""
		for c_idx in range(current.length()):
			var c = current[c_idx]
			if rules.has(c):
				next += rules[c]
			else:
				next += c
		current = next
	return current


func _interpret(sentence: String) -> PackedVector3Array:
	var vertices = PackedVector3Array()
	var pos = Vector3.ZERO
	var dir = Vector3.UP
	var current_length = segment_length
	var stack: Array = []
	var angle_rad = deg_to_rad(angle)

	for c_idx in range(sentence.length()):
		var c = sentence[c_idx]
		match c:
			"F", "G":
				var new_pos = pos + dir * current_length
				vertices.append(pos)
				vertices.append(new_pos)
				pos = new_pos
			"+":
				dir = dir.rotated(Vector3.BACK, angle_rad)
			"-":
				dir = dir.rotated(Vector3.BACK, -angle_rad)
			"[":
				stack.append([pos, dir, current_length])
				current_length *= length_decay
			"]":
				if stack.size() > 0:
					var state = stack.pop_back()
					pos = state[0]
					dir = state[1]
					current_length = state[2]

	return vertices


func _compute_bounds(vertices: PackedVector3Array) -> Array:
	if vertices.size() == 0:
		return [Vector3.ZERO, Vector3.ZERO]

	var bmin = Vector3(vertices[0].x, vertices[0].y, vertices[0].z)
	var bmax = Vector3(vertices[0].x, vertices[0].y, vertices[0].z)
	for v in vertices:
		bmin.x = min(bmin.x, v.x)
		bmin.y = min(bmin.y, v.y)
		bmin.z = min(bmin.z, v.z)
		bmax.x = max(bmax.x, v.x)
		bmax.y = max(bmax.y, v.y)
		bmax.z = max(bmax.z, v.z)
	return [bmin, bmax]


func get_mesh() -> ArrayMesh:
	if mesh_data.is_empty():
		generate()

	var arrays = []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = mesh_data["vertices"]

	var array_mesh = ArrayMesh.new()
	array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)

	return array_mesh
