class_name Bracket
extends Control

const SQUARE_BRACKET := 0
const ANGLE_BRACKET := 1
const TICK_GROUP := 2

const LEFT := 0
const RIGHT := 1
const TOP := 2
const BOTTOM := 3

@export var role: int = HudRole.NEUTRAL
@export var style: int = SQUARE_BRACKET
@export var orientation: int = LEFT
@export var arm_length_u: float = 2.0
@export var stroke_width_px: float = 2.0
@export var tick_count: int = 0
@export var tick_spacing_u: float = 1.0
@export var tick_length_u: float = 0.5

var _bracket_lines: Node2D


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_bracket_lines = $BracketLines
	rebuild_geometry()
	HudTheme.palette_changed.connect(_on_palette_changed)


func _exit_tree() -> void:
	if HudTheme.palette_changed.is_connected(_on_palette_changed):
		HudTheme.palette_changed.disconnect(_on_palette_changed)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		rebuild_geometry()


func set_role(new_role: int) -> void:
	role = new_role
	_apply_role_color()


func get_span() -> float:
	var unit_px: float = HudTheme.unit_px
	if style == TICK_GROUP and tick_count > 1:
		return (tick_count - 1) * tick_spacing_u * unit_px
	return size.y if orientation in [LEFT, RIGHT] else size.x


func rebuild_geometry() -> void:
	if not _bracket_lines:
		return
	for child in _bracket_lines.get_children():
		child.queue_free()
	var unit_px: float = HudTheme.unit_px
	var arm_px := arm_length_u * unit_px
	var color := HudTheme.palette.get_rim(role)
	var w := size.x
	var h := size.y
	match style:
		SQUARE_BRACKET:
			_build_square_bracket(arm_px, w, h, color)
		ANGLE_BRACKET:
			_build_angle_bracket(arm_px, w, h, color)
		TICK_GROUP:
			_build_tick_group(arm_px, w, h, color)


func _build_square_bracket(arm_px: float, w: float, h: float, color: Color) -> void:
	match orientation:
		LEFT:
			_add_line([Vector2(arm_px, 0.0), Vector2(0.0, 0.0)], color)
			_add_line([Vector2(0.0, 0.0), Vector2(0.0, h)], color)
			_add_line([Vector2(0.0, h), Vector2(arm_px, h)], color)
		RIGHT:
			_add_line([Vector2(w - arm_px, 0.0), Vector2(w, 0.0)], color)
			_add_line([Vector2(w, 0.0), Vector2(w, h)], color)
			_add_line([Vector2(w, h), Vector2(w - arm_px, h)], color)
		TOP:
			_add_line([Vector2(0.0, arm_px), Vector2(0.0, 0.0)], color)
			_add_line([Vector2(0.0, 0.0), Vector2(w, 0.0)], color)
			_add_line([Vector2(w, 0.0), Vector2(w, arm_px)], color)
		BOTTOM:
			_add_line([Vector2(0.0, h - arm_px), Vector2(0.0, h)], color)
			_add_line([Vector2(0.0, h), Vector2(w, h)], color)
			_add_line([Vector2(w, h), Vector2(w, h - arm_px)], color)


func _build_angle_bracket(arm_px: float, w: float, h: float, color: Color) -> void:
	match orientation:
		LEFT:
			_add_line([Vector2(arm_px, 0.0), Vector2(0.0, h * 0.5)], color, Line2D.LINE_CAP_NONE)
			_add_line([Vector2(0.0, h * 0.5), Vector2(arm_px, h)], color, Line2D.LINE_CAP_NONE)
		RIGHT:
			_add_line([Vector2(w - arm_px, 0.0), Vector2(w, h * 0.5)], color, Line2D.LINE_CAP_NONE)
			_add_line([Vector2(w, h * 0.5), Vector2(w - arm_px, h)], color, Line2D.LINE_CAP_NONE)
		TOP:
			_add_line([Vector2(0.0, arm_px), Vector2(w * 0.5, 0.0)], color, Line2D.LINE_CAP_NONE)
			_add_line([Vector2(w * 0.5, 0.0), Vector2(w, arm_px)], color, Line2D.LINE_CAP_NONE)
		BOTTOM:
			_add_line([Vector2(0.0, h - arm_px), Vector2(w * 0.5, h)], color, Line2D.LINE_CAP_NONE)
			_add_line([Vector2(w * 0.5, h), Vector2(w, h - arm_px)], color, Line2D.LINE_CAP_NONE)


func _build_tick_group(_arm_px: float, _w: float, _h: float, color: Color) -> void:
	var unit_px: float = HudTheme.unit_px
	var spacing := tick_spacing_u * unit_px
	var tick_len := tick_length_u * unit_px
	var count: int = max(tick_count, 0)
	var total_span: float = (count - 1) * spacing if count > 1 else 0.0
	match orientation:
		LEFT, RIGHT:
			var x_base: float = 0.0 if orientation == LEFT else _w
			var x_dir: float = 1.0 if orientation == LEFT else -1.0
			_add_line([Vector2(x_base, 0.0), Vector2(x_base, total_span)], color)
			for i in count:
				var y: float = float(i) * spacing
				_add_line([Vector2(x_base, y), Vector2(x_base + tick_len * x_dir, y)], color)
		TOP, BOTTOM:
			var y_base: float = 0.0 if orientation == TOP else _h
			var y_dir: float = 1.0 if orientation == TOP else -1.0
			_add_line([Vector2(0.0, y_base), Vector2(total_span, y_base)], color)
			for i in count:
				var x: float = float(i) * spacing
				_add_line([Vector2(x, y_base), Vector2(x, y_base + tick_len * y_dir)], color)


func _add_line(points: Array, color: Color, cap_mode: Line2D.LineCapMode = Line2D.LINE_CAP_ROUND) -> void:
	var line := Line2D.new()
	for p in points:
		line.add_point(p)
	line.width = stroke_width_px
	line.default_color = color
	line.antialiased = true
	line.begin_cap_mode = cap_mode
	line.end_cap_mode = cap_mode
	_bracket_lines.add_child(line)


func _apply_role_color() -> void:
	if not _bracket_lines:
		return
	var color := HudTheme.palette.get_rim(role)
	for child in _bracket_lines.get_children():
		if child is Line2D:
			child.default_color = color


func _on_palette_changed() -> void:
	_apply_role_color()
