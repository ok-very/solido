class_name HudTokens
extends Resource
## Layout, typography, and interaction tokens for the HUD system.
## All u-suffixed values are in unit multiples (1u = unit_px pixels).

# === LAYOUT ===
@export_group("Layout")
@export var unit_px: float = 24.0
@export var radius_small_u: float = 1.0
@export var radius_medium_u: float = 2.0
@export var radius_large_u: float = 4.0
@export var bevel_u: float = 0.5
@export var rim_px: float = 3.0
@export var gap_u: float = 1.0

# === TYPOGRAPHY: Font References ===
@export_group("Typography")
@export_subgroup("Fonts")
@export var font_display: Font
@export var font_ui: Font
@export var font_ui_bold: Font
@export var font_numeric: Font

# === TYPOGRAPHY: Type Scale (in u-units) ===
@export_subgroup("Type Scale")
@export var type_rail_header_u: float = 2.5
@export var type_section_header_u: float = 1.67
@export var type_label_u: float = 1.25
@export var type_value_u: float = 1.5
@export var type_micro_u: float = 1.0

# === TYPOGRAPHY: Tracking ===
@export_subgroup("Tracking")
@export var tracking_display_px: float = 2.0
@export var tracking_ui_px: float = 0.0
@export var tracking_numeric_px: float = -0.5

# === INTERACTION ===
@export_group("Interaction")
@export var touch_min_u: float = 3.0
@export var touch_preferred_u: float = 4.0
@export var touch_gap_u: float = 1.0
@export var long_press_ms: float = 400.0
@export var rim_pulse_ms: float = 200.0
@export var hover_bevel_scale: float = 1.3
@export var active_bevel_scale: float = 0.7
@export var hover_rim_alpha: float = 1.5
@export var active_rim_alpha: float = 0.8
