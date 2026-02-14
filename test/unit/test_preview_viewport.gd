extends GutTest

# Tests for addons/procedural_tools/preview_viewport.gd
# Strategy: create SubViewport in tree, instantiate manager, call setup_viewport.
# Camera/pivot are created inside SubViewport — all testable.

var manager: Node
var viewport: SubViewport


func before_each():
	viewport = SubViewport.new()
	add_child_autofree(viewport)
	manager = load("res://addons/procedural_tools/preview_viewport.gd").new()
	add_child_autofree(manager)
	manager.setup_viewport(viewport)

# --- Setup creates camera and pivot ---


func test_setup_creates_camera():
	assert_not_null(manager.camera)
	assert_is(manager.camera, Camera3D)


func test_setup_creates_pivot():
	assert_not_null(manager.camera_pivot)
	assert_is(manager.camera_pivot, Node3D)


func test_camera_is_child_of_pivot():
	assert_eq(manager.camera.get_parent(), manager.camera_pivot)


func test_pivot_is_child_of_viewport():
	assert_eq(manager.camera_pivot.get_parent(), viewport)

# --- Default values ---


func test_default_distance():
	assert_eq(manager.camera_distance, 5.0)


func test_default_rotation():
	assert_eq(manager.camera_rotation, Vector2(-30, 45))


func test_camera_position_uses_distance():
	assert_eq(manager.camera.position.z, 5.0)

# --- Reset ---


func test_reset_camera_restores_defaults():
	manager.camera_distance = 10.0
	manager.camera_rotation = Vector2(0, 0)
	manager.reset_camera()
	assert_eq(manager.camera_distance, 5.0)
	assert_eq(manager.camera_rotation, Vector2(-30, 45))

# --- Zoom ---


func _make_wheel_event(button: int) -> InputEventMouseButton:
	var event = InputEventMouseButton.new()
	event.button_index = button
	event.pressed = true
	return event


func test_zoom_in_reduces_distance():
	manager.handle_input(_make_wheel_event(MOUSE_BUTTON_WHEEL_UP))
	assert_true(manager.camera_distance < 5.0)


func test_zoom_out_increases_distance():
	manager.handle_input(_make_wheel_event(MOUSE_BUTTON_WHEEL_DOWN))
	assert_true(manager.camera_distance > 5.0)


func test_zoom_clamps_min():
	manager.camera_distance = 1.0
	manager.handle_input(_make_wheel_event(MOUSE_BUTTON_WHEEL_UP))
	assert_true(manager.camera_distance >= 1.0)


func test_zoom_clamps_max():
	manager.camera_distance = 20.0
	manager.handle_input(_make_wheel_event(MOUSE_BUTTON_WHEEL_DOWN))
	assert_true(manager.camera_distance <= 20.0)

# --- Pitch clamp ---


func test_pitch_clamps_at_89():
	# Simulate middle-button press, then large positive delta motion
	var press = InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_MIDDLE
	press.pressed = true
	press.position = Vector2(100, 100)
	manager.handle_input(press)

	var motion = InputEventMouseMotion.new()
	# Large Y delta to push pitch beyond 89
	motion.position = Vector2(100, 2000)
	manager.handle_input(motion)

	assert_true(manager.camera_rotation.x <= 89, "Pitch should clamp at 89")

# --- Existing camera reuse ---


func test_existing_camera_reused():
	# Fresh viewport with a pre-existing camera
	var vp2 = SubViewport.new()
	add_child_autofree(vp2)
	var existing_cam = Camera3D.new()
	existing_cam.name = "SceneCamera"
	vp2.add_child(existing_cam)

	var mgr2 = load("res://addons/procedural_tools/preview_viewport.gd").new()
	add_child_autofree(mgr2)
	mgr2.setup_viewport(vp2)

	assert_eq(mgr2.camera, existing_cam, "Should reuse existing camera")
	assert_null(mgr2.camera_pivot, "Should not create pivot when reusing camera")
