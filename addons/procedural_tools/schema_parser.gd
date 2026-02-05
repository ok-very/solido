@tool
extends RefCounted

# TOML Schema Parser
# Converts schema.toml files into Dictionary structures
# for UI generation and validation

func parse_schema(path: String) -> Dictionary:
	print("[Schema Parser] Parsing: ", path)
	
	if not FileAccess.file_exists(path):
		push_error("[Schema Parser] File not found: ", path)
		return {}
	
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		push_error("[Schema Parser] Could not open file: ", path)
		return {}
	
	var content = file.get_as_text()
	file.close()
	
	# Parse TOML content
	var result = _parse_toml(content)
	
	if result.is_empty():
		push_error("[Schema Parser] Failed to parse TOML")
	
	return result

func _parse_toml(content: String) -> Dictionary:
	# Simple TOML parser for our schema format
	# Handles: [section], key = value, quoted strings, numbers, booleans, arrays
	
	var result = {}
	var current_section = ""
	var lines = content.split("\n")
	
	for line in lines:
		line = line.strip_edges()
		
		# Skip empty lines and comments
		if line.is_empty() or line.begins_with("#"):
			continue
		
		# Section header [section.subsection]
		if line.begins_with("[") and line.ends_with("]"):
			current_section = line.substr(1, line.length() - 2)
			_ensure_nested_dict(result, current_section)
			continue
		
		# Key = Value
		if "=" in line:
			var parts = line.split("=", true, 1)
			if parts.size() == 2:
				var key = parts[0].strip_edges()
				var value = parts[1].strip_edges()
				
				var parsed_value = _parse_value(value)
				
				if current_section.is_empty():
					result[key] = parsed_value
				else:
					_set_nested_value(result, current_section, key, parsed_value)
	
	return result

func _ensure_nested_dict(dict: Dictionary, path: String) -> void:
	var keys = path.split(".")
	var current = dict
	
	for i in range(keys.size()):
		var key = keys[i]
		if not current.has(key):
			current[key] = {}
		if i < keys.size() - 1:
			current = current[key]

func _set_nested_value(dict: Dictionary, path: String, key: String, value) -> void:
	var keys = path.split(".")
	var current = dict
	
	for section_key in keys:
		if not current.has(section_key):
			current[section_key] = {}
		current = current[section_key]
	
	current[key] = value

func _parse_value(value: String):
	# Remove quotes from strings
	if value.begins_with('"') and value.ends_with('"'):
		return value.substr(1, value.length() - 2)
	
	if value.begins_with("'") and value.ends_with("'"):
		return value.substr(1, value.length() - 2)
	
	# Arrays [1, 2, 3]
	if value.begins_with("[") and value.ends_with("]"):
		return _parse_array(value)
	
	# Booleans
	if value == "true":
		return true
	if value == "false":
		return false
	
	# Numbers
	if value.is_valid_float():
		if "." in value:
			return float(value)
		else:
			return int(value)
	
	# Default to string
	return value

func _parse_array(value: String) -> Array:
	var result = []
	var content = value.substr(1, value.length() - 2).strip_edges()
	
	if content.is_empty():
		return result
	
	var items = content.split(",")
	for item in items:
		result.append(_parse_value(item.strip_edges()))
	
	return result

func validate_schema(schema: Dictionary) -> bool:
	# Validate schema structure
	if not schema.has("module"):
		push_error("[Schema Parser] Missing [module] section")
		return false
	
	if not schema.has("parameters"):
		push_error("[Schema Parser] Missing [parameters] section")
		return false
	
	return true
