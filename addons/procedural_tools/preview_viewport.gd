@tool
extends Node

# Preview Viewport Manager
# Manages SubViewport lifecycle and rendering for preview display
# Handles camera controls and viewport updates

var current_viewport: SubViewport
var camera: Camera3D
var camera_pivot: Node3D

var mouse_button_pressed: bool = false
var last_mouse_position: Vector2

# Camera control parameters
var camera_distance: float = 5.0
var camera_rotation: Vector2 = Vector2(-30, 45) # pitch, yaw in degrees
var camera_zoom_speed: float = 0.5
var camera_rotate_speed: float = 0.3

func _ready() -> void:
	print("[Preview Viewport] Manager initialized")

func setup_viewport(viewport: SubViewport) -> void:
	current_viewport = viewport
	
	# Setup default camera if preview scene doesn't have one
	_ensure_camera()

func _ensure_camera() -> void:
	if not current_viewport:
		return
	
	# Check if preview scene already has a camera
	for child in current_viewport.get_children():
		var cam = _find_camera_recursive(child)
		if cam:
			camera = cam
			print("[Preview Viewport] Using existing camera")
			return
	
	# Create default camera setup
	print("[Preview Viewport] Creating default camera")
	
	camera_pivot = Node3D.new()
	camera_pivot.name = "CameraPivot"
	current_viewport.add_child(camera_pivot)
	
	camera = Camera3D.new()
	camera.name = "PreviewCamera"
	camera_pivot.add_child(camera)
	
	_update_camera_transform()

func _find_camera_recursive(node: Node) -> Camera3D:
	if node is Camera3D:
		return node
	
	for child in node.get_children():
		var cam = _find_camera_recursive(child)
		if cam:
			return cam
	
	return null

func _update_camera_transform() -> void:
	if not camera or not camera_pivot:
		return
	
	# Apply rotation to pivot
	camera_pivot.rotation_degrees = Vector3(camera_rotation.x, camera_rotation.y, 0)
	
	# Position camera relative to pivot
	camera.position = Vector3(0, 0, camera_distance)
	camera.look_at(camera_pivot.global_position, Vector3.UP)

func handle_input(event: InputEvent) -> void:
	# Handle camera controls for preview viewport
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_MIDDLE:
			mouse_button_pressed = event.pressed
			last_mouse_position = event.position
		
		# Zoom with mouse wheel
		elif event.button_index == MOUSE_BUTTON_WHEEL_UP:
			camera_distance = max(1.0, camera_distance - camera_zoom_speed)
			_update_camera_transform()
		
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			camera_distance = min(20.0, camera_distance + camera_zoom_speed)
			_update_camera_transform()
	
	elif event is InputEventMouseMotion:
		if mouse_button_pressed:
			var delta = event.position - last_mouse_position
			last_mouse_position = event.position
			
			# Rotate camera
			camera_rotation.y += delta.x * camera_rotate_speed
			camera_rotation.x += delta.y * camera_rotate_speed
			
			# Clamp pitch
			camera_rotation.x = clamp(camera_rotation.x, -89, 89)
			
			_update_camera_transform()

func reset_camera() -> void:
	camera_distance = 5.0
	camera_rotation = Vector2(-30, 45)
	_update_camera_transform()
