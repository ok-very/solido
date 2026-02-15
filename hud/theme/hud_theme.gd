extends Node
## HUD theme autoload singleton.
## Loads tokens + active palette, maintains shader material registry (push model),
## generates Godot Theme with type variations for typography.

signal palette_changed

var tokens: HudTokens
var palette: HudPalette
var unit_px: float = 24.0

var _theme: Theme
var _materials: Dictionary = { } # ShaderMaterial -> int (role)


func _ready() -> void:
	tokens = preload("res://hud/tokens/hud_tokens.tres")
	palette = preload("res://hud/tokens/palette_ops_amber.tres")
	unit_px = tokens.unit_px
	_theme = Theme.new()
	_rebuild_theme()

# === MATERIAL REGISTRY ===


func register_material(mat: ShaderMaterial, role: int) -> void:
	_materials[mat] = role
	_push_uniforms(mat, role)


func unregister_material(mat: ShaderMaterial) -> void:
	_materials.erase(mat)


func _push_uniforms(mat: ShaderMaterial, role: int) -> void:
	var solid := palette.get_solid(role)
	var glass_tint := palette.get_glass_tint(role)
	var rim := palette.get_rim(role)

	mat.set_shader_parameter("u_tint_color", Vector3(solid.r, solid.g, solid.b))
	mat.set_shader_parameter("u_glass_tint", Vector3(glass_tint.r, glass_tint.g, glass_tint.b))
	mat.set_shader_parameter("u_rim_color", Vector3(rim.r, rim.g, rim.b))
	mat.set_shader_parameter("u_alpha", 1.0)
	mat.set_shader_parameter("u_rim_px", tokens.rim_px)
	mat.set_shader_parameter("u_bevel_px", tokens.bevel_u * unit_px)
	mat.set_shader_parameter("u_state", 0.0)
	mat.set_shader_parameter("u_bevel_scale", 1.0)
	mat.set_shader_parameter("u_rim_alpha", 1.0)


func push_all_uniforms() -> void:
	for mat in _materials:
		var role: int = _materials[mat]
		_push_uniforms(mat, role)

# === PALETTE SWITCHING ===


func set_palette(new_palette: HudPalette) -> void:
	palette = new_palette
	push_all_uniforms()
	_rebuild_theme()
	palette_changed.emit()

# === TEXT COLOR HELPERS ===


func get_text_on_solid_color(role: int) -> Color:
	return palette.get_text_on_solid(role)


func get_text_on_glass_color(role: int) -> Color:
	return palette.get_text_on_glass(role)

# === THEME ACCESS ===


func get_theme() -> Theme:
	return _theme

# === THEME GENERATION ===


func _rebuild_theme() -> void:
	var unit := tokens.unit_px

	# Base Label type
	if tokens.font_ui:
		_theme.set_font("font", "Label", tokens.font_ui)
	_theme.set_font_size("font_size", "Label", int(tokens.type_label_u * unit))
	_theme.set_color("font_color", "Label", palette.get_text_on_glass(HudRole.NEUTRAL))

	# Base Button type
	if tokens.font_ui:
		_theme.set_font("font", "Button", tokens.font_ui)
	_theme.set_font_size("font_size", "Button", int(tokens.type_label_u * unit))

	# --- Type variations ---

	# HudRailHeader
	_theme.add_type("HudRailHeader")
	_theme.set_type_variation("HudRailHeader", "Label")
	if tokens.font_display:
		_theme.set_font("font", "HudRailHeader", tokens.font_display)
	_theme.set_font_size("font_size", "HudRailHeader", int(tokens.type_rail_header_u * unit))
	_theme.set_color("font_color", "HudRailHeader", palette.get_text_on_solid(HudRole.NAV))

	# HudSectionHeader
	_theme.add_type("HudSectionHeader")
	_theme.set_type_variation("HudSectionHeader", "Label")
	if tokens.font_display:
		_theme.set_font("font", "HudSectionHeader", tokens.font_display)
	_theme.set_font_size("font_size", "HudSectionHeader", int(tokens.type_section_header_u * unit))
	_theme.set_color("font_color", "HudSectionHeader", palette.get_text_on_glass(HudRole.NEUTRAL))

	# HudLabel
	_theme.add_type("HudLabel")
	_theme.set_type_variation("HudLabel", "Label")
	if tokens.font_ui:
		_theme.set_font("font", "HudLabel", tokens.font_ui)
	_theme.set_font_size("font_size", "HudLabel", int(tokens.type_label_u * unit))
	_theme.set_color("font_color", "HudLabel", palette.get_text_on_glass(HudRole.NEUTRAL))

	# HudButton
	_theme.add_type("HudButton")
	_theme.set_type_variation("HudButton", "Button")
	if tokens.font_ui:
		_theme.set_font("font", "HudButton", tokens.font_ui)
	_theme.set_font_size("font_size", "HudButton", int(tokens.type_label_u * unit))

	# HudButtonBold
	_theme.add_type("HudButtonBold")
	_theme.set_type_variation("HudButtonBold", "Button")
	if tokens.font_ui_bold:
		_theme.set_font("font", "HudButtonBold", tokens.font_ui_bold)
	_theme.set_font_size("font_size", "HudButtonBold", int(tokens.type_label_u * unit))

	# HudValue
	_theme.add_type("HudValue")
	_theme.set_type_variation("HudValue", "Label")
	if tokens.font_numeric:
		_theme.set_font("font", "HudValue", tokens.font_numeric)
	_theme.set_font_size("font_size", "HudValue", int(tokens.type_value_u * unit))
	_theme.set_color("font_color", "HudValue", palette.get_text_on_glass(HudRole.NEUTRAL))

	# HudMicro
	_theme.add_type("HudMicro")
	_theme.set_type_variation("HudMicro", "Label")
	if tokens.font_numeric:
		_theme.set_font("font", "HudMicro", tokens.font_numeric)
	_theme.set_font_size("font_size", "HudMicro", int(tokens.type_micro_u * unit))
	_theme.set_color("font_color", "HudMicro", palette.get_text_on_glass(HudRole.NEUTRAL))

	# HudChipLabel
	_theme.add_type("HudChipLabel")
	_theme.set_type_variation("HudChipLabel", "Label")
	if tokens.font_ui:
		_theme.set_font("font", "HudChipLabel", tokens.font_ui)
	_theme.set_font_size("font_size", "HudChipLabel", int(tokens.type_label_u * unit))
	_theme.set_color("font_color", "HudChipLabel", palette.get_text_on_solid(HudRole.NEUTRAL))
