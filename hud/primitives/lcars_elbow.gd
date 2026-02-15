class_name LcarsElbow
extends Control

const TL := 0
const TR := 1
const BR := 2
const BL := 3

@export var role: int = HudRole.NEUTRAL
@export var rotation_index: int = TL
@export var outer_radius_u: float = 4.0
@export var inner_radius_u: float = 2.0
@export var arm_h_thickness_u: float = 3.0
@export var arm_v_thickness_u: float = 3.0
@export var arm_h_length_u: float = 8.0
@export var arm_v_length_u: float = 6.0

signal port_positions_changed(ports: Dictionary)

var _material: ShaderMaterial
var _elbow_body: ColorRect
var _port_markers: Control

const _SHADER = preload("res://hud/shaders/hud_elbow.gdshader")


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_elbow_body = $ElbowBody
	_port_markers = $PortMarkers
	_material = ShaderMaterial.new()
	_material.shader = _SHADER
	_material.resource_local_to_scene = true
	_elbow_body.material = _material
	_update_minimum_size()
	_update_shader_params()
	HudTheme.register_material(_material, role)
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	HudTheme.unregister_material(_material)
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_shader_params()
		_update_port_positions()
		port_positions_changed.emit(get_port_positions())


func set_role(new_role: int) -> void:
	HudTheme.unregister_material(_material)
	role = new_role
	HudTheme.register_material(_material, role)


func get_port_position(side: String) -> Vector2:
	return get_port_positions().get(side, global_position)


func get_port_positions() -> Dictionary:
	var r := get_global_rect()
	var result := {}
	match rotation_index:
		TL:
			result["H"] = Vector2(r.position.x + r.size.x, r.position.y + arm_h_thickness_u * HudTheme.unit_px * 0.5)
			result["V"] = Vector2(r.position.x + arm_v_thickness_u * HudTheme.unit_px * 0.5, r.position.y + r.size.y)
		TR:
			result["H"] = Vector2(r.position.x, r.position.y + arm_h_thickness_u * HudTheme.unit_px * 0.5)
			result["V"] = Vector2(r.position.x + r.size.x - arm_v_thickness_u * HudTheme.unit_px * 0.5, r.position.y + r.size.y)
		BR:
			result["H"] = Vector2(r.position.x, r.position.y + r.size.y - arm_h_thickness_u * HudTheme.unit_px * 0.5)
			result["V"] = Vector2(r.position.x + r.size.x - arm_v_thickness_u * HudTheme.unit_px * 0.5, r.position.y)
		BL:
			result["H"] = Vector2(r.position.x + r.size.x, r.position.y + r.size.y - arm_h_thickness_u * HudTheme.unit_px * 0.5)
			result["V"] = Vector2(r.position.x + arm_v_thickness_u * HudTheme.unit_px * 0.5, r.position.y)
	return result


func get_h_attachment_edge() -> Rect2:
	var r := get_global_rect()
	var unit_px: float = HudTheme.unit_px
	var h_thick := arm_h_thickness_u * unit_px
	match rotation_index:
		TL:
			return Rect2(r.position.x + r.size.x, r.position.y, 0.0, h_thick)
		TR:
			return Rect2(r.position.x, r.position.y, 0.0, h_thick)
		BR:
			return Rect2(r.position.x, r.position.y + r.size.y - h_thick, 0.0, h_thick)
		BL:
			return Rect2(r.position.x + r.size.x, r.position.y + r.size.y - h_thick, 0.0, h_thick)
	return Rect2()


func get_v_attachment_edge() -> Rect2:
	var r := get_global_rect()
	var unit_px: float = HudTheme.unit_px
	var v_thick := arm_v_thickness_u * unit_px
	match rotation_index:
		TL:
			return Rect2(r.position.x, r.position.y + r.size.y, v_thick, 0.0)
		TR:
			return Rect2(r.position.x + r.size.x - v_thick, r.position.y + r.size.y, v_thick, 0.0)
		BR:
			return Rect2(r.position.x + r.size.x - v_thick, r.position.y, v_thick, 0.0)
		BL:
			return Rect2(r.position.x, r.position.y, v_thick, 0.0)
	return Rect2()


func _update_minimum_size() -> void:
	var unit_px: float = HudTheme.unit_px
	var w: float
	var h: float
	match rotation_index:
		TL, TR:
			w = max(outer_radius_u, arm_v_thickness_u) * unit_px + arm_h_length_u * unit_px
			h = max(outer_radius_u, arm_h_thickness_u) * unit_px + arm_v_length_u * unit_px
		BR, BL:
			w = max(outer_radius_u, arm_v_thickness_u) * unit_px + arm_h_length_u * unit_px
			h = max(outer_radius_u, arm_h_thickness_u) * unit_px + arm_v_length_u * unit_px
		_:
			w = (outer_radius_u + arm_h_length_u) * unit_px
			h = (outer_radius_u + arm_v_length_u) * unit_px
	custom_minimum_size = Vector2(w, h)


func _update_shader_params() -> void:
	if not _material:
		return
	var unit_px: float = HudTheme.unit_px
	_material.set_shader_parameter("u_outer_radius_px", outer_radius_u * unit_px)
	_material.set_shader_parameter("u_inner_radius_px", inner_radius_u * unit_px)
	_material.set_shader_parameter("u_arm_h_px", arm_h_thickness_u * unit_px)
	_material.set_shader_parameter("u_arm_v_px", arm_v_thickness_u * unit_px)
	_material.set_shader_parameter("u_rotation", rotation_index)


func _update_port_positions() -> void:
	if not _port_markers:
		return
	var r := get_rect()
	var unit_px: float = HudTheme.unit_px
	var h_thick := arm_h_thickness_u * unit_px
	var v_thick := arm_v_thickness_u * unit_px
	match rotation_index:
		TL:
			$PortMarkers/PortH.position = Vector2(r.size.x, h_thick * 0.5)
			$PortMarkers/PortV.position = Vector2(v_thick * 0.5, r.size.y)
		TR:
			$PortMarkers/PortH.position = Vector2(0.0, h_thick * 0.5)
			$PortMarkers/PortV.position = Vector2(r.size.x - v_thick * 0.5, r.size.y)
		BR:
			$PortMarkers/PortH.position = Vector2(0.0, r.size.y - h_thick * 0.5)
			$PortMarkers/PortV.position = Vector2(r.size.x - v_thick * 0.5, 0.0)
		BL:
			$PortMarkers/PortH.position = Vector2(r.size.x, r.size.y - h_thick * 0.5)
			$PortMarkers/PortV.position = Vector2(v_thick * 0.5, 0.0)


func _on_palette_changed() -> void:
	pass
