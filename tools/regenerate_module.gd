extends SceneTree

# Headless Module Regeneration Script
# Called by watch.py to regenerate modules without editor

func _init() -> void:
	var args = OS.get_cmdline_user_args()
	
	if args.size() == 0:
		print("[Regenerate] Error: No module path provided")
		quit(1)
		return
	
	var module_path = args[0]
	print("[Regenerate] Module: ", module_path)
	
	# Load schema
	var schema_path = module_path + "/schema.toml"
	if not FileAccess.file_exists(schema_path):
		print("[Regenerate] Error: schema.toml not found")
		quit(1)
		return
	
	# Load generator
	var generator_path = module_path + "/generator.gd"
	if not FileAccess.file_exists(generator_path):
		print("[Regenerate] Error: generator.gd not found")
		quit(1)
		return
	
	var GeneratorClass = load(generator_path)
	var generator = GeneratorClass.new()
	
	# Generate with default parameters
	var result = generator.generate()
	
	if result:
		print("[Regenerate] ✓ Generation successful")
		quit(0)
	else:
		print("[Regenerate] ✗ Generation failed")
		quit(1)
