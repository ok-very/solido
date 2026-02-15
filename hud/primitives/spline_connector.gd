class_name SplineConnector
extends Control

const CAP_NONE := 0
const CAP_DOT := 1
const CAP_ENDCAP := 2
const CAP_BRACKET := 3

@export var role: int = HudRole.NEUTRAL
@export var importance: int = 50
@export var start_cap_style: int = CAP_NONE
@export var end_cap_style: int = CAP_NONE
@export var scanline_speed: float = 0.0
@export var bake_interval_px: float = 8.0

signal connector_tapped(connector: SplineConnector)
signal connector_long_pressed(connector: SplineConnector)
signal connector_hovered(connector: SplineConnector)
signal connector_unhovered(connector: SplineConnector)

var _material: ShaderMaterial
var _curve: Curve2D
var _baked_points: PackedVector2Array
var _connector_line: Line2D
var _rim_line: Line2D
var _hit_area: Control
var _start_cap: Control
var _end_cap: Control
var _long_press_timer: Timer
var _is_pressing: bool = false
var _press_position: Vector2
var _long_press_fired: bool = false

const _SHADER = preload("res://hud/shaders/hud_connector.gdshader")


func _ready() -> void:
	_connector_line = $ConnectorLine
	_rim_line = $RimLine
	_hit_area = $HitArea
	_start_cap = $StartCap
	_end_cap = $EndCap
	_curve = Curve2D.new()
	_curve.bake_interval = bake_interval_px
	# Connector line setup
	_material = ShaderMaterial.new()
	_material.shader = _SHADER
	_material.resource_local_to_scene = true
	_connector_line.material = _material
	_connector_line.antialiased = true
	_connector_line.begin_cap_mode = Line2D.LINE_CAP_ROUND
	_connector_line.end_cap_mode = Line2D.LINE_CAP_ROUND
	_connector_line.joint_mode = Line2D.LINE_JOINT_ROUND
	# Rim line setup
	_rim_line.antialiased = true
	_rim_line.width = 1.0
	_rim_line.default_color = HudTheme.palette.get_rim(role)
	# Hit area
	_hit_area.mouse_filter = Control.MOUSE_FILTER_STOP
	_hit_area.gui_input.connect(_on_hit_area_gui_input)
	_hit_area.mouse_entered.connect(func(): connector_hovered.emit(self))
	_hit_area.mouse_exited.connect(func(): connector_unhovered.emit(self))
	# Long press timer
	_long_press_timer = Timer.new()
	_long_press_timer.one_shot = true
	_long_press_timer.wait_time = HudTheme.tokens.long_press_ms / 1000.0
	_long_press_timer.timeout.connect(_on_long_press_timeout)
	add_child(_long_press_timer)
	# Apply importance class
	_apply_importance()
	# Register
	HudTheme.register_material(_material, role)
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	HudTheme.unregister_material(_material)
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func set_role(new_role: int) -> void:
	HudTheme.unregister_material(_material)
	role = new_role
	HudTheme.register_material(_material, role)
	_rim_line.default_color = HudTheme.palette.get_rim(role)


func set_importance(value: int) -> void:
	importance = value
	_apply_importance()


func set_curve_points(p0: Vector2, p1: Vector2, p2: Vector2, p3: Vector2) -> void:
	_curve.clear_points()
	_curve.add_point(p0, Vector2.ZERO, p1 - p0)
	_curve.add_point(p3, p2 - p3, Vector2.ZERO)
	_bake_and_render()


func set_points_from_bake(points: PackedVector2Array) -> void:
	_baked_points = points
	_render_points()


func set_bus_segment(entry: Vector2, exit: Vector2) -> void:
	_baked_points = PackedVector2Array([entry, exit])
	_render_points()


func get_baked_points() -> PackedVector2Array:
	return _baked_points


func get_start_position() -> Vector2:
	if _baked_points.size() > 0:
		return _baked_points[0]
	return Vector2.ZERO


func get_end_position() -> Vector2:
	if _baked_points.size() > 0:
		return _baked_points[_baked_points.size() - 1]
	return Vector2.ZERO


func get_bounding_rect() -> Rect2:
	if _baked_points.size() == 0:
		return Rect2()
	var min_p := _baked_points[0]
	var max_p := _baked_points[0]
	for p in _baked_points:
		min_p.x = min(min_p.x, p.x)
		min_p.y = min(min_p.y, p.y)
		max_p.x = max(max_p.x, p.x)
		max_p.y = max(max_p.y, p.y)
	var padding := 1.5 * HudTheme.unit_px
	return Rect2(min_p - Vector2(padding, padding), max_p - min_p + Vector2(padding * 2.0, padding * 2.0))


func _apply_importance() -> void:
	var unit_px: float = HudTheme.unit_px
	var line_width: float
	var line_alpha: float
	var scan_speed: float
	var scan_alpha: float
	if importance >= 80:
		line_width = 0.375 * unit_px
		line_alpha = 0.55
		scan_speed = 0.08
		scan_alpha = 0.3
	elif importance >= 50:
		line_width = 0.25 * unit_px
		line_alpha = 0.40
		scan_speed = 0.0
		scan_alpha = 0.0
	else:
		line_width = 0.125 * unit_px
		line_alpha = 0.30
		scan_speed = 0.0
		scan_alpha = 0.0
	_connector_line.width = line_width
	if _material:
		_material.set_shader_parameter("u_stroke_width", line_width)
		_material.set_shader_parameter("u_alpha", line_alpha)
		_material.set_shader_parameter("u_scanline_speed", scan_speed)
		_material.set_shader_parameter("u_scanline_alpha", scan_alpha)


func _bake_and_render() -> void:
	_curve.bake_interval = bake_interval_px
	_baked_points = _curve.get_baked_points()
	_render_points()


func _render_points() -> void:
	if _baked_points.size() < 2:
		return
	_connector_line.clear_points()
	_rim_line.clear_points()
	for p in _baked_points:
		_connector_line.add_point(p)
	# Compute rim offset
	var rim_points := _compute_rim_offset(_baked_points)
	for p in rim_points:
		_rim_line.add_point(p)
	# Update hit area
	var bounds := get_bounding_rect()
	_hit_area.position = bounds.position
	_hit_area.size = bounds.size
	# Position caps
	_start_cap.position = _baked_points[0]
	_end_cap.position = _baked_points[_baked_points.size() - 1]


func _compute_rim_offset(points: PackedVector2Array) -> PackedVector2Array:
	var result := PackedVector2Array()
	var count: int = points.size()
	if count < 2:
		return points
	for i in count:
		var tangent: Vector2
		if i == 0:
			tangent = points[1] - points[0]
		elif i == count - 1:
			tangent = points[i] - points[i - 1]
		else:
			tangent = points[i + 1] - points[i - 1]
		var len_sq := tangent.length_squared()
		if len_sq < 0.0001:
			result.append(points[i])
		else:
			var perp := tangent.orthogonal().normalized()
			result.append(points[i] + perp * 1.0)
	return result


func _on_hit_area_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			# Check proximity to polyline
			var local_pos: Vector2 = event.position + _hit_area.position
			if _is_near_polyline(local_pos):
				_is_pressing = true
				_press_position = event.position
				_long_press_fired = false
				_long_press_timer.start()
		elif not event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			if _is_pressing:
				_long_press_timer.stop()
				if not _long_press_fired:
					connector_tapped.emit(self)
				_is_pressing = false
	elif event is InputEventMouseMotion and _is_pressing:
		if _press_position.distance_to(event.position) > HudTheme.unit_px:
			_long_press_timer.stop()
			_is_pressing = false


func _is_near_polyline(point: Vector2) -> bool:
	var threshold := 1.5 * HudTheme.unit_px
	for i in range(_baked_points.size() - 1):
		var dist := _point_to_segment_distance(point, _baked_points[i], _baked_points[i + 1])
		if dist <= threshold:
			return true
	return false


func _point_to_segment_distance(point: Vector2, seg_a: Vector2, seg_b: Vector2) -> float:
	var ab := seg_b - seg_a
	var len_sq := ab.dot(ab)
	if len_sq < 0.0001:
		return point.distance_to(seg_a)
	var ap := point - seg_a
	var t := clampf(ap.dot(ab) / len_sq, 0.0, 1.0)
	var closest := seg_a + ab * t
	return point.distance_to(closest)


func _on_long_press_timeout() -> void:
	if _is_pressing:
		_long_press_fired = true
		connector_long_pressed.emit(self)


func _on_palette_changed() -> void:
	_rim_line.default_color = HudTheme.palette.get_rim(role)
