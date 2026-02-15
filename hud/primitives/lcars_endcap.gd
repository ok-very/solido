class_name LcarsEndcap
extends Control

const HALF_PILL := 0
const STEPPED := 1

const LEFT := 0
const RIGHT := 1
const UP := 2
const DOWN := 3

@export var role: int = HudRole.NEUTRAL
@export var style: int = HALF_PILL
@export var direction: int = LEFT
@export var thickness_u: float = 3.0
@export var length_u: float = 4.0
@export var pill_radius_u: float = 0.0
@export var step_depth_u: float = 1.0
@export var step_offset_u: float = 1.0

signal port_positions_changed(ports: Dictionary)

var _material: ShaderMaterial
var _endcap_body: ColorRect
var _port_markers: Control

const _SHADER = preload("res://hud/shaders/hud_endcap.gdshader")


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_endcap_body = $EndcapBody
	_port_markers = $PortMarkers
	_material = ShaderMaterial.new()
	_material.shader = _SHADER
	_material.resource_local_to_scene = true
	_endcap_body.material = _material
	var unit_px: float = HudTheme.unit_px
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
	return {
		"N": Vector2(r.position.x + r.size.x * 0.5, r.position.y),
		"S": Vector2(r.position.x + r.size.x * 0.5, r.position.y + r.size.y),
		"E": Vector2(r.position.x + r.size.x, r.position.y + r.size.y * 0.5),
		"W": Vector2(r.position.x, r.position.y + r.size.y * 0.5),
	}


func get_attachment_edge() -> Rect2:
	var r := get_global_rect()
	match direction:
		LEFT:
			return Rect2(r.position.x + r.size.x, r.position.y, 0.0, r.size.y)
		RIGHT:
			return Rect2(r.position.x, r.position.y, 0.0, r.size.y)
		UP:
			return Rect2(r.position.x, r.position.y + r.size.y, r.size.x, 0.0)
		DOWN:
			return Rect2(r.position.x, r.position.y, r.size.x, 0.0)
	return Rect2()


func _update_minimum_size() -> void:
	var unit_px: float = HudTheme.unit_px
	var thick_px := thickness_u * unit_px
	var len_px := length_u * unit_px
	if direction in [LEFT, RIGHT]:
		custom_minimum_size = Vector2(len_px, thick_px)
	else:
		custom_minimum_size = Vector2(thick_px, len_px)


func _update_shader_params() -> void:
	if not _material:
		return
	var unit_px: float = HudTheme.unit_px
	var radius_px: float
	if pill_radius_u <= 0.0:
		if direction in [LEFT, RIGHT]:
			radius_px = size.y * 0.5
		else:
			radius_px = size.x * 0.5
	else:
		radius_px = pill_radius_u * unit_px
	_material.set_shader_parameter("u_radius_px", radius_px)
	_material.set_shader_parameter("u_style", style)
	_material.set_shader_parameter("u_direction", direction)
	_material.set_shader_parameter("u_step_depth_px", step_depth_u * unit_px)
	_material.set_shader_parameter("u_step_offset_px", step_offset_u * unit_px)


func _update_port_positions() -> void:
	if not _port_markers:
		return
	var r := get_rect()
	match direction:
		LEFT:
			$PortMarkers/PortFlat.position = Vector2(r.size.x, r.size.y * 0.5)
			$PortMarkers/PortRound.position = Vector2(0.0, r.size.y * 0.5)
		RIGHT:
			$PortMarkers/PortFlat.position = Vector2(0.0, r.size.y * 0.5)
			$PortMarkers/PortRound.position = Vector2(r.size.x, r.size.y * 0.5)
		UP:
			$PortMarkers/PortFlat.position = Vector2(r.size.x * 0.5, r.size.y)
			$PortMarkers/PortRound.position = Vector2(r.size.x * 0.5, 0.0)
		DOWN:
			$PortMarkers/PortFlat.position = Vector2(r.size.x * 0.5, 0.0)
			$PortMarkers/PortRound.position = Vector2(r.size.x * 0.5, r.size.y)


func _on_palette_changed() -> void:
	pass
