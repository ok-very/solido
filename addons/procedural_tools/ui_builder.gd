@tool
extends RefCounted

# UI Builder
# Converts parameter definitions from schema Dictionary
# into appropriate Godot Control nodes

func create_control(param_name: String, param_def: Dictionary) -> Control:
	var type = param_def.get("type", "float")
	
	match type:
		"int":
			return _create_int_control(param_name, param_def)
		"float":
			return _create_float_control(param_name, param_def)
		"bool":
			return _create_bool_control(param_name, param_def)
		"string":
			return _create_string_control(param_name, param_def)
		"enum":
			return _create_enum_control(param_name, param_def)
		"color":
			return _create_color_control(param_name, param_def)
		"vector2":
			return _create_vector2_control(param_name, param_def)
		"vector3":
			return _create_vector3_control(param_name, param_def)
		_:
			push_error("[UI Builder] Unknown type: ", type)
			return null

func _create_int_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# SpinBox
	var spinbox = SpinBox.new()
	spinbox.step = 1
	spinbox.min_value = param_def.get("min", 0)
	spinbox.max_value = param_def.get("max", 100)
	spinbox.value = param_def.get("default", 0)
	spinbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	container.add_child(spinbox)
	
	return container

func _create_float_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# HSlider + SpinBox combo
	var hbox = HBoxContainer.new()
	hbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	container.add_child(hbox)
	
	var slider = HSlider.new()
	slider.min_value = param_def.get("min", 0.0)
	slider.max_value = param_def.get("max", 1.0)
	slider.step = param_def.get("step", 0.01)
	slider.value = param_def.get("default", 0.5)
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hbox.add_child(slider)
	
	var spinbox = SpinBox.new()
	spinbox.min_value = slider.min_value
	spinbox.max_value = slider.max_value
	spinbox.step = slider.step
	spinbox.value = slider.value
	spinbox.custom_minimum_size.x = 80
	hbox.add_child(spinbox)
	
	# Sync slider and spinbox
	slider.value_changed.connect(func(v): spinbox.value = v)
	spinbox.value_changed.connect(func(v): slider.value = v)
	
	return container

func _create_bool_control(param_name: String, param_def: Dictionary) -> Control:
	var checkbox = CheckBox.new()
	checkbox.text = param_name.capitalize()
	checkbox.button_pressed = param_def.get("default", false)
	
	if param_def.has("description"):
		checkbox.tooltip_text = param_def["description"]
	
	return checkbox

func _create_string_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# LineEdit
	var lineedit = LineEdit.new()
	lineedit.text = param_def.get("default", "")
	lineedit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	container.add_child(lineedit)
	
	return container

func _create_enum_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# OptionButton
	var option_button = OptionButton.new()
	option_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	var options = param_def.get("options", [])
	for i in range(options.size()):
		option_button.add_item(str(options[i]))
	
	var default = param_def.get("default", "")
	if default in options:
		option_button.select(options.find(default))
	
	container.add_child(option_button)
	
	return container

func _create_color_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# ColorPickerButton
	var color_picker = ColorPickerButton.new()
	var default_color = param_def.get("default", [1.0, 1.0, 1.0, 1.0])
	
	if default_color is Array and default_color.size() >= 3:
		var r = default_color[0]
		var g = default_color[1]
		var b = default_color[2]
		var a = default_color[3] if default_color.size() > 3 else 1.0
		color_picker.color = Color(r, g, b, a)
	
	container.add_child(color_picker)
	
	return container

func _create_vector2_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# X and Y spinboxes
	var hbox = HBoxContainer.new()
	container.add_child(hbox)
	
	var default = param_def.get("default", [0.0, 0.0])
	
	for i in range(2):
		var axis_label = Label.new()
		axis_label.text = ["X:", "Y:"][i]
		hbox.add_child(axis_label)
		
		var spinbox = SpinBox.new()
		spinbox.step = 0.1
		spinbox.value = default[i] if i < default.size() else 0.0
		spinbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		hbox.add_child(spinbox)
	
	return container

func _create_vector3_control(param_name: String, param_def: Dictionary) -> Control:
	var container = VBoxContainer.new()
	
	# Label
	var label = Label.new()
	label.text = param_name.capitalize()
	if param_def.has("description"):
		label.tooltip_text = param_def["description"]
	container.add_child(label)
	
	# X, Y, Z spinboxes
	var hbox = HBoxContainer.new()
	container.add_child(hbox)
	
	var default = param_def.get("default", [0.0, 0.0, 0.0])
	
	for i in range(3):
		var axis_label = Label.new()
		axis_label.text = ["X:", "Y:", "Z:"][i]
		hbox.add_child(axis_label)
		
		var spinbox = SpinBox.new()
		spinbox.step = 0.1
		spinbox.value = default[i] if i < default.size() else 0.0
		spinbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		hbox.add_child(spinbox)
	
	return container

func get_control_value(control: Control):
	# Extract value from various control types
	if control is CheckBox:
		return control.button_pressed
	elif control is LineEdit:
		return control.text
	elif control is ColorPickerButton:
		return control.color
	
	# For containers, look for the actual input control
	if control is VBoxContainer or control is HBoxContainer:
		for child in control.get_children():
			if child is SpinBox:
				return child.value
			elif child is Slider:
				return child.value
			elif child is OptionButton:
				return child.get_item_text(child.selected)
			elif child is LineEdit:
				return child.text
			elif child is HBoxContainer:
				# For Vector2/Vector3 controls
				var values = []
				for grandchild in child.get_children():
					if grandchild is SpinBox:
						values.append(grandchild.value)
				if values.size() == 2:
					return Vector2(values[0], values[1])
				elif values.size() == 3:
					return Vector3(values[0], values[1], values[2])
	
	return null
