extends GutTest

# Tests for addons/procedural_tools/schema_parser.gd

var parser: RefCounted


func before_each():
	parser = load("res://addons/procedural_tools/schema_parser.gd").new()

# --- TOML Parsing ---


func test_parse_simple_key_value():
	var result = parser._parse_toml('key = "value"')
	assert_eq(result["key"], "value")


func test_parse_integer():
	var result = parser._parse_toml("count = 42")
	assert_eq(result["count"], 42)
	assert_typeof(result["count"], TYPE_INT)


func test_parse_float():
	var result = parser._parse_toml("scale = 1.5")
	assert_eq(result["scale"], 1.5)
	assert_typeof(result["scale"], TYPE_FLOAT)


func test_parse_boolean_true():
	var result = parser._parse_toml("enabled = true")
	assert_true(result["enabled"])


func test_parse_boolean_false():
	var result = parser._parse_toml("enabled = false")
	assert_false(result["enabled"])


func test_parse_array_strings():
	var result = parser._parse_toml('options = ["a", "b", "c"]')
	assert_eq(result["options"], ["a", "b", "c"])


func test_parse_array_numbers():
	var result = parser._parse_toml("values = [1, 2, 3]")
	assert_eq(result["values"], [1, 2, 3])


func test_parse_empty_array():
	var result = parser._parse_toml("items = []")
	assert_eq(result["items"], [])


func test_parse_section():
	var toml = "[module]\nname = \"Test\""
	var result = parser._parse_toml(toml)
	assert_eq(result["module"]["name"], "Test")


func test_parse_nested_section():
	var toml = "[parameters.width]\ntype = \"int\"\ndefault = 10"
	var result = parser._parse_toml(toml)
	assert_eq(result["parameters"]["width"]["type"], "int")
	assert_eq(result["parameters"]["width"]["default"], 10)


func test_parse_comments_ignored():
	var toml = "# This is a comment\nkey = \"value\"\n# Another comment"
	var result = parser._parse_toml(toml)
	assert_eq(result.size(), 1)
	assert_eq(result["key"], "value")


func test_parse_empty_lines_ignored():
	var toml = "\n\nkey = \"value\"\n\n"
	var result = parser._parse_toml(toml)
	assert_eq(result["key"], "value")


func test_parse_single_quoted_string():
	var result = parser._parse_toml("name = 'single'")
	assert_eq(result["name"], "single")


func test_parse_value_with_equals_sign():
	# Edge case: value containing =
	var result = parser._parse_toml('formula = "a = b"')
	assert_eq(result["formula"], "a = b")

# --- Full Schema ---


func test_parse_terrain_schema():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	assert_false(schema.is_empty(), "Schema should not be empty")
	assert_has(schema, "module")
	assert_has(schema, "parameters")
	assert_has(schema, "output")


func test_terrain_schema_module_section():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	assert_eq(schema["module"]["name"], "Terrain Generator")
	assert_eq(schema["module"]["version"], "1.0.0")


func test_terrain_schema_parameters():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	var params = schema["parameters"]
	assert_has(params, "width")
	assert_has(params, "height")
	assert_has(params, "scale")
	assert_has(params, "terrain_type")


func test_terrain_schema_enum_parameter():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	var terrain_type = schema["parameters"]["terrain_type"]
	assert_eq(terrain_type["type"], "enum")
	assert_eq(terrain_type["default"], "plains")
	assert_has(terrain_type, "options")
	assert_eq(terrain_type["options"].size(), 4)


func test_terrain_schema_float_parameter():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	var scale = schema["parameters"]["scale"]
	assert_eq(scale["type"], "float")
	assert_eq(scale["default"], 1.0)
	assert_eq(scale["min"], 0.1)
	assert_eq(scale["max"], 10.0)


func test_terrain_schema_output_section():
	var schema = parser.parse_schema("res://modules/terrain/schema.toml")
	assert_eq(schema["output"]["type"], "Resource")
	assert_eq(schema["output"]["format"], "tres")

# --- Validation ---


func test_validate_valid_schema():
	var schema = { "module": { "name": "Test" }, "parameters": { "x": { } } }
	assert_true(parser.validate_schema(schema))


func test_validate_missing_module():
	var schema = { "parameters": { "x": { } } }
	assert_false(parser.validate_schema(schema))
	assert_push_error("Missing [module] section")


func test_validate_missing_parameters():
	var schema = { "module": { "name": "Test" } }
	assert_false(parser.validate_schema(schema))
	assert_push_error("Missing [parameters] section")


func test_validate_empty_schema():
	assert_false(parser.validate_schema({ }))
	assert_push_error("Missing [module] section")

# --- Edge Cases ---


func test_parse_nonexistent_file():
	var result = parser.parse_schema("res://nonexistent.toml")
	assert_true(result.is_empty())
	assert_push_error("File not found")


func test_parse_empty_content():
	var result = parser._parse_toml("")
	assert_true(result.is_empty())


func test_parse_multiple_sections():
	var toml = "[a]\nx = 1\n[b]\ny = 2"
	var result = parser._parse_toml(toml)
	assert_eq(result["a"]["x"], 1)
	assert_eq(result["b"]["y"], 2)
