@tool
extends EditorPlugin

# EditorPlugin entry point for Procedural Tools
# Manages dock lifecycle and editor integration

var tool_dock: Control
var preview_viewport: Node

func _enter_tree() -> void:
	print("[Procedural Tools] Initializing plugin...")
	
	# Load and instantiate the main dock UI
	var dock_scene = preload("res://addons/procedural_tools/tool_dock.tscn")
	if dock_scene:
		tool_dock = dock_scene.instantiate()
		add_control_to_dock(DOCK_SLOT_RIGHT_UL, tool_dock)
		print("[Procedural Tools] Dock added successfully")
	else:
		push_error("[Procedural Tools] Failed to load tool_dock.tscn")
	
	# Initialize preview viewport manager
	preview_viewport = preload("res://addons/procedural_tools/preview_viewport.gd").new()
	add_child(preview_viewport)
	
	print("[Procedural Tools] Plugin initialized")

func _exit_tree() -> void:
	print("[Procedural Tools] Shutting down plugin...")
	
	if tool_dock:
		remove_control_from_docks(tool_dock)
		tool_dock.queue_free()
	
	if preview_viewport:
		preview_viewport.queue_free()
	
	print("[Procedural Tools] Plugin shut down")

func _get_plugin_name() -> String:
	return "Procedural Tools"

func _get_plugin_icon() -> Texture2D:
	# Return custom icon if available
	return null
