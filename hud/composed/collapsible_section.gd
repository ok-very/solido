class_name CollapsibleSection
extends Control

@export var section_name: String = ""
@export var role: int = HudRole.NEUTRAL
@export var bracket_style: int = Bracket.ANGLE_BRACKET
@export var bracket_orientation: int = Bracket.RIGHT
@export var tick_count: int = 0
@export var collapsed: bool = false

signal collapsed_changed(is_collapsed: bool)

var _chip: Chip
var _bracket: Bracket
var _content: VBoxContainer


func _ready() -> void:
	_chip = $SectionHeader/HeaderChip
	_bracket = $SectionHeader/HeaderBracket
	_content = $SectionContent
	_chip.set_label(section_name)
	_chip.set_role(role)
	_bracket.style = bracket_style
	_bracket.orientation = bracket_orientation
	_bracket.tick_count = tick_count
	_bracket.set_role(role)
	_bracket.rebuild_geometry()
	_chip.pressed.connect(_on_header_pressed)
	_content.visible = !collapsed
	_chip.set_toggled(!collapsed)


func _on_header_pressed() -> void:
	collapsed = !collapsed
	_content.visible = !collapsed
	_chip.set_toggled(!collapsed)
	collapsed_changed.emit(collapsed)


func set_collapsed(value: bool) -> void:
	if collapsed == value:
		return
	collapsed = value
	if _content:
		_content.visible = !collapsed
	if _chip:
		_chip.set_toggled(!collapsed)
	collapsed_changed.emit(collapsed)


func get_content() -> VBoxContainer:
	return _content
