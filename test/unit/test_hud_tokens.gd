extends GutTest
## Tests for Phase 0: HudRole, HudTokens, HudPalette, HudTheme

# === HudRole ===

func test_role_constants():
	assert_eq(HudRole.NAV, 0)
	assert_eq(HudRole.EDIT, 1)
	assert_eq(HudRole.BLEND, 2)
	assert_eq(HudRole.TELEMETRY, 3)
	assert_eq(HudRole.ALERT, 4)
	assert_eq(HudRole.NEUTRAL, 5)
	assert_eq(HudRole.COUNT, 6)


func test_role_name():
	assert_eq(HudRole.role_name(HudRole.NAV), "NAV")
	assert_eq(HudRole.role_name(HudRole.NEUTRAL), "NEUTRAL")
	assert_eq(HudRole.role_name(-1), "UNKNOWN")
	assert_eq(HudRole.role_name(99), "UNKNOWN")

# === HudTokens ===


func test_tokens_defaults():
	var t = HudTokens.new()
	assert_eq(t.unit_px, 24.0)
	assert_eq(t.radius_small_u, 1.0)
	assert_eq(t.radius_medium_u, 2.0)
	assert_eq(t.radius_large_u, 4.0)
	assert_eq(t.bevel_u, 0.5)
	assert_eq(t.rim_px, 3.0)
	assert_eq(t.touch_min_u, 3.0)
	assert_eq(t.long_press_ms, 400.0)


func test_tokens_type_scale():
	var t = HudTokens.new()
	assert_eq(t.type_rail_header_u, 2.5)
	assert_eq(t.type_section_header_u, 1.67)
	assert_eq(t.type_label_u, 1.25)
	assert_eq(t.type_value_u, 1.5)
	assert_eq(t.type_micro_u, 1.0)


func test_tokens_tracking():
	var t = HudTokens.new()
	assert_eq(t.tracking_display_px, 2.0)
	assert_eq(t.tracking_ui_px, 0.0)
	assert_eq(t.tracking_numeric_px, -0.5)


func test_tokens_resource_load():
	var t = load("res://hud/tokens/hud_tokens.tres") as HudTokens
	assert_not_null(t, "hud_tokens.tres should load")
	assert_eq(t.unit_px, 24.0)
	assert_not_null(t.font_display, "font_display should be preloaded")
	assert_not_null(t.font_ui, "font_ui should be preloaded")
	assert_not_null(t.font_numeric, "font_numeric should be preloaded")

# === HudPalette ===


func test_palette_defaults():
	var p = HudPalette.new()
	assert_true(p.nav_solid.r > 0.9, "NAV solid should be warm amber")
	assert_true(p.neutral_solid.r < 0.5, "NEUTRAL solid should be muted")


func test_palette_solid_getter():
	var p = HudPalette.new()
	assert_eq(p.get_solid(HudRole.NAV), p.nav_solid)
	assert_eq(p.get_solid(HudRole.EDIT), p.edit_solid)
	assert_eq(p.get_solid(HudRole.BLEND), p.blend_solid)
	assert_eq(p.get_solid(HudRole.TELEMETRY), p.telemetry_solid)
	assert_eq(p.get_solid(HudRole.ALERT), p.alert_solid)
	assert_eq(p.get_solid(HudRole.NEUTRAL), p.neutral_solid)


func test_palette_glass_tint_getter():
	var p = HudPalette.new()
	var tint = p.get_glass_tint(HudRole.NAV)
	assert_true(tint.a < 0.5, "Glass tint should be translucent")


func test_palette_text_getters():
	var p = HudPalette.new()
	var ts = p.get_text_on_solid(HudRole.NAV)
	assert_true(ts.r < 0.2, "Text on solid NAV should be dark")
	var tg = p.get_text_on_glass(HudRole.NAV)
	assert_true(tg.r > 0.8, "Text on glass NAV should be light")


func test_palette_rim_getter():
	var p = HudPalette.new()
	var rim = p.get_rim(HudRole.NAV)
	assert_true(rim.r > 0.8, "NAV rim should be bright")


func test_palette_unknown_role_falls_back_to_neutral():
	var p = HudPalette.new()
	assert_eq(p.get_solid(99), p.neutral_solid)
	assert_eq(p.get_glass_tint(-1), p.neutral_glass_tint)


func test_palette_resource_load():
	var p = load("res://hud/tokens/palette_ops_amber.tres") as HudPalette
	assert_not_null(p, "palette_ops_amber.tres should load")
	assert_true(p.nav_solid.r > 0.9, "Loaded palette NAV should be amber")


func test_palette_all_presets_load():
	var presets: Array[String] = [
		"res://hud/tokens/palette_ops_amber.tres",
		"res://hud/tokens/palette_cyan_tools.tres",
		"res://hud/tokens/palette_violet_blend.tres",
		"res://hud/tokens/palette_green_telemetry.tres",
		"res://hud/tokens/palette_magenta_alert.tres",
		"res://hud/tokens/palette_neutral_smoke.tres",
	]
	for path in presets:
		var p: HudPalette = load(path) as HudPalette
		assert_not_null(p, "%s should load" % path)


func test_palette_presets_have_valid_colors():
	var presets: Array[String] = [
		"res://hud/tokens/palette_ops_amber.tres",
		"res://hud/tokens/palette_cyan_tools.tres",
		"res://hud/tokens/palette_violet_blend.tres",
		"res://hud/tokens/palette_green_telemetry.tres",
		"res://hud/tokens/palette_magenta_alert.tres",
		"res://hud/tokens/palette_neutral_smoke.tres",
	]
	for path in presets:
		var p: HudPalette = load(path) as HudPalette
		for role_idx in HudRole.COUNT:
			var solid: Color = p.get_solid(role_idx)
			var tint: Color = p.get_glass_tint(role_idx)
			var ts: Color = p.get_text_on_solid(role_idx)
			var tg: Color = p.get_text_on_glass(role_idx)
			var rim: Color = p.get_rim(role_idx)
			assert_true(solid.a >= 1.0, "%s role %d solid should be opaque" % [path, role_idx])
			assert_true(tint.a < 0.5, "%s role %d glass_tint should be translucent" % [path, role_idx])
			assert_true(ts.a >= 1.0, "%s role %d text_on_solid should be opaque" % [path, role_idx])
			assert_true(tg.a >= 1.0, "%s role %d text_on_glass should be opaque" % [path, role_idx])
			assert_true(rim.a >= 1.0, "%s role %d rim should be opaque" % [path, role_idx])


func test_palette_presets_hero_role_boosted():
	var cyan: HudPalette = load("res://hud/tokens/palette_cyan_tools.tres")
	assert_true(cyan.edit_solid.g > 0.85, "cyan_tools: EDIT solid should be boosted cyan")
	var violet: HudPalette = load("res://hud/tokens/palette_violet_blend.tres")
	assert_true(violet.blend_solid.r > 0.6, "violet_blend: BLEND solid should be boosted violet")
	var green: HudPalette = load("res://hud/tokens/palette_green_telemetry.tres")
	assert_true(green.telemetry_solid.g > 0.85, "green_telemetry: TELEMETRY solid should be boosted green")
	var magenta: HudPalette = load("res://hud/tokens/palette_magenta_alert.tres")
	assert_true(magenta.alert_solid.r > 0.9, "magenta_alert: ALERT solid should be boosted magenta")
	var smoke: HudPalette = load("res://hud/tokens/palette_neutral_smoke.tres")
	assert_true(smoke.neutral_solid.r > 0.5, "neutral_smoke: NEUTRAL solid should be boosted")

# === HudTheme ===

const HudThemeScript = preload("res://hud/theme/hud_theme.gd")


func _setup_theme():
	var node = HudThemeScript.new()
	add_child_autofree(node)
	return node


func test_theme_initializes():
	var t = _setup_theme()
	assert_not_null(t.tokens)
	assert_not_null(t.palette)
	assert_eq(t.unit_px, 24.0)
	assert_not_null(t.get_theme())


func test_theme_register_unregister_material():
	var t = _setup_theme()
	var mat = ShaderMaterial.new()
	t.register_material(mat, HudRole.NAV)
	t.unregister_material(mat)
	assert_true(true, "register/unregister completed without error")


func test_theme_text_color_helpers():
	var t = _setup_theme()
	var ts = t.get_text_on_solid_color(HudRole.NAV)
	assert_true(ts.r < 0.2)
	var tg = t.get_text_on_glass_color(HudRole.NAV)
	assert_true(tg.r > 0.8)


func test_theme_type_variations():
	var t = _setup_theme()
	var theme = t.get_theme()
	assert_true(theme.has_font("font", "HudRailHeader"), "Should have HudRailHeader font")
	assert_true(theme.has_font_size("font_size", "HudRailHeader"), "Should have HudRailHeader font_size")
	assert_true(theme.has_font("font", "HudValue"), "Should have HudValue font")
	assert_true(theme.has_font("font", "HudMicro"), "Should have HudMicro font")
	assert_true(theme.has_font("font", "HudChipLabel"), "Should have HudChipLabel font")


func test_theme_font_sizes():
	var t = _setup_theme()
	var theme = t.get_theme()
	assert_eq(theme.get_font_size("font_size", "HudRailHeader"), int(2.5 * 24.0))
	assert_eq(theme.get_font_size("font_size", "HudValue"), int(1.5 * 24.0))
	assert_eq(theme.get_font_size("font_size", "HudMicro"), int(1.0 * 24.0))


func test_theme_palette_switch():
	var t = _setup_theme()
	var new_palette = HudPalette.new()
	new_palette.nav_solid = Color.RED
	watch_signals(t)
	t.set_palette(new_palette)
	assert_eq(t.palette, new_palette)
	assert_signal_emitted(t, "palette_changed")
