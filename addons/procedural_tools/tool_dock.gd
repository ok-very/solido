@tool
extends Control

# Main dock UI for Procedural Tools
# Discovers modules, displays UI, manages preview

var schema_parser: RefCounted
var ui_builder: RefCounted
var preview_manager: Node

var current_module_path: String = ""
var current_generator: Resource
var parameter_controls: Dictionary = {} # param_name -> Control

# UI References
var module_selector: OptionButton
var parameters_container: VBoxContainer
var preview_container: SubViewportContainer
var generate_button: Button
var save_button: Button

func _ready() -> void:
	print("[Tool Dock] Initializing...")
	
	# Load dependencies
	schema_parser = load("res://addons/procedural_tools/schema_parser.gd").new()
	ui_builder = load("res://addons/procedural_tools/ui_builder.gd").new()
	preview_manager = get_node("/root/EditorNode/ProceduralTools/PreviewViewport")
	
	_build_ui()
	_scan_modules()
	
	print("[Tool Dock] Initialized")

func _build_ui() -> void:
	# Main vertical layout
	var vbox = VBoxContainer.new()
	vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	vbox.size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_child(vbox)
	
	# === Module Selection ===
	var module_label = Label.new()
	module_label.text = "Module"
	vbox.add_child(module_label)
	
	module_selector = OptionButton.new()
	module_selector.item_selected.connect(_on_module_selected)
	vbox.add_child(module_selector)
	
	vbox.add_child(HSeparator.new())
	
	# === Parameters Section ===
	var params_label = Label.new()
	params_label.text = "Parameters"
	vbox.add_child(params_label)
	
	var scroll = ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size.y = 200
	vbox.add_child(scroll)
	
	parameters_container = VBoxContainer.new()
	parameters_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(parameters_container)
	
	vbox.add_child(HSeparator.new())
	
	# === Preview Section ===
	var preview_label = Label.new()
	preview_label.text = "Preview"
	vbox.add_child(preview_label)
	
	preview_container = SubViewportContainer.new()
	preview_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	preview_container.size_flags_vertical = Control.SIZE_EXPAND_FILL
	preview_container.custom_minimum_size = Vector2(300, 300)
	preview_container.stretch = true
	vbox.add_child(preview_container)
	
	# Create SubViewport
	var viewport = SubViewport.new()
	viewport.size = Vector2i(512, 512)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	preview_container.add_child(viewport)
	
	vbox.add_child(HSeparator.new())
	
	# === Action Buttons ===
	var button_hbox = HBoxContainer.new()
	vbox.add_child(button_hbox)
	
	generate_button = Button.new()
	generate_button.text = "Generate Preview"
	generate_button.pressed.connect(_on_generate_pressed)
	button_hbox.add_child(generate_button)
	
	save_button = Button.new()
	save_button.text = "Save to Library"
	save_button.pressed.connect(_on_save_pressed)
	save_button.disabled = true
	button_hbox.add_child(save_button)

func _scan_modules() -> void:
	print("[Tool Dock] Scanning for modules...")
	
	var modules_dir = "res://modules/"
	if not DirAccess.dir_exists_absolute(modules_dir):
		print("[Tool Dock] modules/ directory not found")
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
					print("[Tool Dock] Found module: ", file_name)
			file_name = dir.get_next()
		
		dir.list_dir_end()
	
	if module_selector.item_count > 0:
		module_selector.select(0)
		_on_module_selected(0)

func _on_module_selected(index: int) -> void:
	current_module_path = module_selector.get_item_metadata(index)
	print("[Tool Dock] Loading module: ", current_module_path)
	
	# Parse schema
	var schema_path = current_module_path + "/schema.toml"
	var schema_dict = schema_parser.parse_schema(schema_path)
	
	if schema_dict.is_empty():
		push_error("[Tool Dock] Failed to parse schema: " + schema_path)
		return
	
	# Load generator resource
	var generator_path = current_module_path + "/generator.gd"
	if FileAccess.file_exists(generator_path):
		var GeneratorClass = load(generator_path)
		current_generator = GeneratorClass.new()
	else:
		push_error("[Tool Dock] Generator not found: " + generator_path)
		return
	
	# Build UI from schema
	_rebuild_parameters_ui(schema_dict)
	
	# Load preview scene
	_load_preview_scene()

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
				
				# Connect value changes to live preview
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

func _on_parameter_changed() -> void:
	# Auto-generate preview on parameter change
	_generate_preview()

func _load_preview_scene() -> void:
	var preview_path = current_module_path + "/preview.tscn"
	
	if not FileAccess.file_exists(preview_path):
		print("[Tool Dock] No preview scene found: ", preview_path)
		return
	
	var viewport = preview_container.get_child(0) as SubViewport
	
	# Clear existing scene
	for child in viewport.get_children():
		child.queue_free()
	
	# Load and instantiate preview scene
	var preview_scene = load(preview_path)
	if preview_scene:
		var instance = preview_scene.instantiate()
		viewport.add_child(instance)
		print("[Tool Dock] Preview scene loaded")

func _on_generate_pressed() -> void:
	_generate_preview()

func _generate_preview() -> void:
	if not current_generator:
		return
	
	print("[Tool Dock] Generating preview...")
	
	# Collect parameter values
	var params = {}
	for param_name in parameter_controls:
		var control = parameter_controls[param_name]
		params[param_name] = ui_builder.get_control_value(control)
	
	# Apply parameters to generator
	for param_name in params:
		if param_name in current_generator:
			current_generator.set(param_name, params[param_name])
	
	# Generate and update preview
	if current_generator.has_method("generate"):
		var result = current_generator.generate()
		_update_preview(result)
		save_button.disabled = false

func _update_preview(generated_data) -> void:
	# Apply generated data to preview scene
	var viewport = preview_container.get_child(0) as SubViewport
	if viewport.get_child_count() > 0:
		var preview_root = viewport.get_child(0)
		
		# Look for updatable components
		if preview_root.has_method("update_preview"):
			preview_root.update_preview(generated_data)
		else:
			print("[Tool Dock] Preview scene has no update_preview() method")

func _on_save_pressed() -> void:
	if not current_generator:
		return
	
	# Prompt for save location
	var dialog = EditorFileDialog.new()
	dialog.file_mode = EditorFileDialog.FILE_MODE_SAVE_FILE
	dialog.access = EditorFileDialog.ACCESS_RESOURCES
	dialog.current_dir = "res://library/"
	dialog.add_filter("*.tres", "Godot Resource")
	dialog.file_selected.connect(_save_resource)
	add_child(dialog)
	dialog.popup_centered_ratio(0.6)

func _save_resource(path: String) -> void:
	if not path.ends_with(".tres"):
		path += ".tres"
	
	var error = ResourceSaver.save(current_generator, path)
	
	if error == OK:
		print("[Tool Dock] Saved resource to: ", path)
	else:
		push_error("[Tool Dock] Failed to save resource: ", error)
