class_name GlassPane
extends PanelContainer

@export var role: int = HudRole.NEUTRAL
@export var blur_lod: float = 2.0
@export var alpha: float = 0.33
@export var grain: float = 0.03
@export var bevel_u: float = 1.0
@export var radius_u: float = 2.0
@export var rim_px: float = 3.0
@export var auto_contrast: bool = true
@export var blur_enabled: bool = true

signal port_positions_changed(ports: Dictionary)
signal content_margin_ready(margin_node: MarginContainer)

var _material: ShaderMaterial
var _back_buffer: BackBufferCopy
var _glass_body: ColorRect
var _content_margin: MarginContainer

const _SHADER = preload("res://hud/shaders/hud_glass.gdshader")


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_PASS
	_back_buffer = $BackBufferCopy
	_glass_body = $GlassBody
	_content_margin = $ContentMargin
	# Set PanelContainer's stylebox to empty
	add_theme_stylebox_override("panel", StyleBoxEmpty.new())
	# Create shader material
	_material = ShaderMaterial.new()
	_material.shader = _SHADER
	_material.resource_local_to_scene = true
	_glass_body.material = _material
	_update_shader_params()
	_update_back_buffer_rect()
	HudTheme.register_material(_material, role)
	HudTheme.palette_changed.connect(_on_palette_changed)
	content_margin_ready.emit(_content_margin)


func _exit_tree() -> void:
	HudTheme.unregister_material(_material)
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_update_back_buffer_rect()
		_update_shader_params()
		port_positions_changed.emit(get_port_positions())
	elif what == NOTIFICATION_TRANSFORM_CHANGED:
		_update_back_buffer_rect()


func set_role(new_role: int) -> void:
	HudTheme.unregister_material(_material)
	role = new_role
	HudTheme.register_material(_material, role)


func set_blur_enabled(enabled: bool) -> void:
	blur_enabled = enabled
	if _material:
		if enabled:
			_material.set_shader_parameter("u_blur_lod", blur_lod)
		else:
			_material.set_shader_parameter("u_blur_lod", 0.0)


func set_alpha(value: float) -> void:
	alpha = value
	if _material:
		_material.set_shader_parameter("u_alpha", alpha)


func set_grain(value: float) -> void:
	grain = value
	if _material:
		_material.set_shader_parameter("u_grain", grain)


func get_content_container() -> MarginContainer:
	return _content_margin


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


func _update_shader_params() -> void:
	if not _material:
		return
	var unit_px: float = HudTheme.unit_px
	_material.set_shader_parameter("u_blur_lod", blur_lod if blur_enabled else 0.0)
	_material.set_shader_parameter("u_grain", grain)
	_material.set_shader_parameter("u_radius_px", radius_u * unit_px)
	_material.set_shader_parameter("u_contrast_min", 0.3 if auto_contrast else 0.0)
	_material.set_shader_parameter("u_alpha", alpha)


func _update_back_buffer_rect() -> void:
	if not _back_buffer:
		return
	_back_buffer.copy_mode = BackBufferCopy.COPY_MODE_RECT
	_back_buffer.rect = get_rect()


func _on_palette_changed() -> void:
	pass
