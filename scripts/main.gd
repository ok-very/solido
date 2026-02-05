extends Control

# Main Runtime UI
# Standalone application for procedural generation with 3D preview
# Does not require editor - can be run as exported executable

var schema_parser: RefCounted
var ui_builder: RefCounted

var current_module_path: String = ""
var current_generator: Resource
var parameter_controls: Dictionary = {}

# UI Node References
@onready var module_selector: OptionButton = $HSplitContainer/LeftPanel/MarginContainer/VBoxContainer/ModuleSelector
@onready var parameters_container: VBoxContainer = $HSplitContainer/LeftPanel/MarginContainer/VBoxContainer/ScrollContainer/ParametersContainer
@onready var generate_button: Button = $HSplitContainer/LeftPanel/MarginContainer/VBoxContainer/ButtonsContainer/GenerateButton
@onready var save_button: Button = $HSplitContainer/LeftPanel/MarginContainer/VBoxContainer/ButtonsContainer/SaveButton
@onready var viewport: SubViewport = $HSplitContainer/RightPanel/ViewportContainer/SubViewport

func _ready() -> void:
	print("[Main] Initializing runtime application...")
	
	# Load dependencies
	schema_parser = load("res://addons/procedural_tools/schema_parser.gd").new()
	ui_builder = load("res://addons/procedural_tools/ui_builder.gd").new()
	
	# Connect signals
	module_selector.item_selected.connect(_on_module_selected)
	generate_button.pressed.connect(_on_generate_pressed)
	save_button.pressed.connect(_on_save_pressed)
	
	# Scan and load modules
	_scan_modules()
	
	# Setup default camera in viewport
	_setup_default_camera()
	
	print("[Main] Ready")

func _setup_default_camera() -> void:
	# Create default 3D environment
	var world_env = WorldEnvironment.new()
	var environment = Environment.new()
	environment.background_mode = Environment.BG_SKY
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	world_env.environment = environment
	viewport.add_child(world_env)
	
	# Create directional light
	var light = DirectionalLight3D.new()
	light.light_energy = 1.0
	light.light_color = Color(1.0, 0.95, 0.9)
	light.shadow_enabled = true
	light.rotation_degrees = Vector3(-45, 45, 0)
	viewport.add_child(light)
	
	# Create camera pivot and camera
	var camera_pivot = Node3D.new()
	camera_pivot.name = "CameraPivot"
	viewport.add_child(camera_pivot)
	
	var camera = Camera3D.new()
	camera.name = "Camera"
	camera.position = Vector3(0, 0, 15)
	camera.look_at(Vector3.ZERO, Vector3.UP)
	camera_pivot.add_child(camera)
	
	print("[Main] Default 3D environment setup complete")

func _scan_modules() -> void:
	print("[Main] Scanning for modules...")
	
	var modules_dir = "res://modules/"
	if not DirAccess.dir_exists_absolute(modules_dir):
		print("[Main] modules/ directory not found")
		return
	
	var dir = DirAccess.open(modules_dir)
	if dir:
		dir.list_dir_begin()
		var file_name = dir.get_next()
		
		while file_name != "":
			if dir.current_is_dir() and not file_name.begins_with("."):
				var schema_path = modules_dir + file_name + "/schema.toml"
				if FileAccess.file_exists(schema_path):
					module_selector.add_item(file_name.capitalize())
					module_selector.set_item_metadata(module_selector.item_count - 1, modules_dir + file_name)
					print("[Main] Found module: ", file_name)
			file_name = dir.get_next()
		
		dir.list_dir_end()
	
	if module_selector.item_count > 0:
		module_selector.select(0)
		_on_module_selected(0)
	else:
		print("[Main] No modules found")

func _on_module_selected(index: int) -> void:
	current_module_path = module_selector.get_item_metadata(index)
	print("[Main] Loading module: ", current_module_path)
	
	# Parse schema
	var schema_path = current_module_path + "/schema.toml"
	var schema_dict = schema_parser.parse_schema(schema_path)
	
	if schema_dict.is_empty():
		push_error("[Main] Failed to parse schema: " + schema_path)
		return
	
	# Load generator resource
	var generator_path = current_module_path + "/generator.gd"
	if FileAccess.file_exists(generator_path):
		var GeneratorClass = load(generator_path)
		current_generator = GeneratorClass.new()
	else:
		push_error("[Main] Generator not found: " + generator_path)
		return
	
	# Build UI from schema
	_rebuild_parameters_ui(schema_dict)
	
	# Load preview scene
	_load_preview_scene()
	
	# Auto-generate initial preview
	_generate_preview()

func _rebuild_parameters_ui(schema: Dictionary) -> void:
	# Clear existing controls
	for child in parameters_container.get_children():
		child.queue_free()
	parameter_controls.clear()
	
	# Build new controls from schema
	if schema.has("parameters"):
		for param_name in schema["parameters"]:
			var param_def = schema["parameters"][param_name]
			var control = ui_builder.create_control(param_name, param_def)
			
			if control:
				parameters_container.add_child(control)
				parameter_controls[param_name] = control
				
				# Connect value changes for auto-update
				_connect_parameter_change(control, param_name)

func _connect_parameter_change(control: Control, param_name: String) -> void:
	# Connect appropriate signal based on control type
	if control is SpinBox or control is Slider:
		control.value_changed.connect(func(_v): _on_parameter_changed())
	elif control is LineEdit:
		control.text_changed.connect(func(_t): _on_parameter_changed())
	elif control is CheckBox:
		control.toggled.connect(func(_t): _on_parameter_changed())
	elif control is OptionButton:
		control.item_selected.connect(func(_i): _on_parameter_changed())
	
	# For container controls, connect children
	for child in control.get_children():
		if child is SpinBox or child is Slider:
			child.value_changed.connect(func(_v): _on_parameter_changed())
		elif child is HBoxContainer:
			for grandchild in child.get_children():
				if grandchild is SpinBox:
					grandchild.value_changed.connect(func(_v): _on_parameter_changed())

func _on_parameter_changed() -> void:
	# Auto-generate on parameter change with debouncing
	if not has_meta("update_timer"):
		var timer = Timer.new()
		timer.one_shot = true
		timer.timeout.connect(_generate_preview)
		add_child(timer)
		set_meta("update_timer", timer)
	
	var timer = get_meta("update_timer") as Timer
	timer.start(0.3) # 300ms debounce

func _load_preview_scene() -> void:
	var preview_path = current_module_path + "/preview.tscn"
	
	if not FileAccess.file_exists(preview_path):
		print("[Main] No preview scene found: ", preview_path)
		return
	
	# Clear existing scene (except camera, light, environment)
	for child in viewport.get_children():
		if not child is Camera3D and not child is DirectionalLight3D and not child is WorldEnvironment:
			if child.name != "CameraPivot":
				child.queue_free()
	
	# Load and instantiate preview scene
	var preview_scene = load(preview_path)
	if preview_scene:
		var instance = preview_scene.instantiate()
		viewport.add_child(instance)
		print("[Main] Preview scene loaded")

func _on_generate_pressed() -> void:
	_generate_preview()

func _generate_preview() -> void:
	if not current_generator:
		return
	
	print("[Main] Generating preview...")
	
	# Collect parameter values
	var params = {}
	for param_name in parameter_controls:
		var control = parameter_controls[param_name]
		params[param_name] = ui_builder.get_control_value(control)
	
	# Apply parameters to generator
	for param_name in params:
		if param_name in current_generator:
			current_generator.set(param_name, params[param_name])
	
	# Generate
	if current_generator.has_method("generate"):
		var result = current_generator.generate()
		_update_preview(result)
		save_button.disabled = false
		print("[Main] Preview generated")

func _update_preview(generated_data) -> void:
	# Find and update preview controller
	for child in viewport.get_children():
		if child.has_method("update_preview"):
			child.update_preview(generated_data)
			return
	
	print("[Main] No preview controller found with update_preview() method")

func _on_save_pressed() -> void:
	if not current_generator:
		return
	
	# Create file dialog
	var dialog = FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	dialog.access = FileDialog.ACCESS_RESOURCES
	dialog.current_dir = "res://library/"
	dialog.filters = PackedStringArray(["*.tres ; Godot Resource"])
	dialog.file_selected.connect(_save_resource)
	add_child(dialog)
	dialog.popup_centered_ratio(0.6)

func _save_resource(path: String) -> void:
	if not path.ends_with(".tres"):
		path += ".tres"
	
	var error = ResourceSaver.save(current_generator, path)
	
	if error == OK:
		print("[Main] Saved resource to: ", path)
	else:
		push_error("[Main] Failed to save resource: ", error)

func _input(event: InputEvent) -> void:
	# Handle camera rotation with middle mouse button
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_MIDDLE:
			Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if event.pressed else Input.MOUSE_MODE_VISIBLE
			
		# Zoom with scroll wheel
		elif event.button_index == MOUSE_BUTTON_WHEEL_UP:
			_zoom_camera(-0.5)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			_zoom_camera(0.5)
	
	elif event is InputEventMouseMotion:
		if Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
			_rotate_camera(event.relative)

func _rotate_camera(delta: Vector2) -> void:
	var camera_pivot = viewport.get_node_or_null("CameraPivot")
	if not camera_pivot:
		return
	
	var sensitivity = 0.3
	camera_pivot.rotate_y(deg_to_rad(-delta.x * sensitivity))
	
	var current_rot = camera_pivot.rotation_degrees
	current_rot.x = clamp(current_rot.x + delta.y * sensitivity, -89, 89)
	camera_pivot.rotation_degrees = current_rot

func _zoom_camera(amount: float) -> void:
	var camera_pivot = viewport.get_node_or_null("CameraPivot")
	if not camera_pivot:
		return
	
	var camera = camera_pivot.get_node_or_null("Camera")
	if not camera:
		return
	
	var new_z = clamp(camera.position.z + amount, 3.0, 30.0)
	camera.position.z = new_z
