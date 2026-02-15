class_name HudPalette
extends Resource
## Color palette for the HUD system.
## 6 roles x 5 shades each: solid, glass_tint, text_on_solid, text_on_glass, rim.

@export_group("NAV")
@export var nav_solid := Color(0.96, 0.73, 0.23)
@export var nav_glass_tint := Color(0.48, 0.36, 0.12, 0.33)
@export var nav_text_on_solid := Color(0.08, 0.06, 0.02)
@export var nav_text_on_glass := Color(0.96, 0.88, 0.72)
@export var nav_rim := Color(1.0, 0.85, 0.4)

@export_group("EDIT")
@export var edit_solid := Color(0.2, 0.78, 0.82)
@export var edit_glass_tint := Color(0.1, 0.39, 0.41, 0.33)
@export var edit_text_on_solid := Color(0.02, 0.08, 0.08)
@export var edit_text_on_glass := Color(0.72, 0.94, 0.96)
@export var edit_rim := Color(0.4, 0.92, 0.95)

@export_group("BLEND")
@export var blend_solid := Color(0.6, 0.35, 0.85)
@export var blend_glass_tint := Color(0.3, 0.18, 0.43, 0.33)
@export var blend_text_on_solid := Color(0.06, 0.03, 0.09)
@export var blend_text_on_glass := Color(0.85, 0.75, 0.96)
@export var blend_rim := Color(0.75, 0.5, 0.95)

@export_group("TELEMETRY")
@export var telemetry_solid := Color(0.25, 0.8, 0.4)
@export var telemetry_glass_tint := Color(0.12, 0.4, 0.2, 0.33)
@export var telemetry_text_on_solid := Color(0.03, 0.08, 0.04)
@export var telemetry_text_on_glass := Color(0.72, 0.96, 0.78)
@export var telemetry_rim := Color(0.45, 0.92, 0.55)

@export_group("ALERT")
@export var alert_solid := Color(0.9, 0.25, 0.45)
@export var alert_glass_tint := Color(0.45, 0.12, 0.22, 0.33)
@export var alert_text_on_solid := Color(0.09, 0.02, 0.04)
@export var alert_text_on_glass := Color(0.96, 0.72, 0.78)
@export var alert_rim := Color(1.0, 0.4, 0.55)

@export_group("NEUTRAL")
@export var neutral_solid := Color(0.45, 0.45, 0.48)
@export var neutral_glass_tint := Color(0.22, 0.22, 0.24, 0.33)
@export var neutral_text_on_solid := Color(0.9, 0.9, 0.92)
@export var neutral_text_on_glass := Color(0.88, 0.88, 0.9)
@export var neutral_rim := Color(0.6, 0.6, 0.65)


func get_solid(role: int) -> Color:
	match role:
		HudRole.NAV:
			return nav_solid
		HudRole.EDIT:
			return edit_solid
		HudRole.BLEND:
			return blend_solid
		HudRole.TELEMETRY:
			return telemetry_solid
		HudRole.ALERT:
			return alert_solid
		_:
			return neutral_solid


func get_glass_tint(role: int) -> Color:
	match role:
		HudRole.NAV:
			return nav_glass_tint
		HudRole.EDIT:
			return edit_glass_tint
		HudRole.BLEND:
			return blend_glass_tint
		HudRole.TELEMETRY:
			return telemetry_glass_tint
		HudRole.ALERT:
			return alert_glass_tint
		_:
			return neutral_glass_tint


func get_text_on_solid(role: int) -> Color:
	match role:
		HudRole.NAV:
			return nav_text_on_solid
		HudRole.EDIT:
			return edit_text_on_solid
		HudRole.BLEND:
			return blend_text_on_solid
		HudRole.TELEMETRY:
			return telemetry_text_on_solid
		HudRole.ALERT:
			return alert_text_on_solid
		_:
			return neutral_text_on_solid


func get_text_on_glass(role: int) -> Color:
	match role:
		HudRole.NAV:
			return nav_text_on_glass
		HudRole.EDIT:
			return edit_text_on_glass
		HudRole.BLEND:
			return blend_text_on_glass
		HudRole.TELEMETRY:
			return telemetry_text_on_glass
		HudRole.ALERT:
			return alert_text_on_glass
		_:
			return neutral_text_on_glass


func get_rim(role: int) -> Color:
	match role:
		HudRole.NAV:
			return nav_rim
		HudRole.EDIT:
			return edit_rim
		HudRole.BLEND:
			return blend_rim
		HudRole.TELEMETRY:
			return telemetry_rim
		HudRole.ALERT:
			return alert_rim
		_:
			return neutral_rim
