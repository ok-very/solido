shader_type canvas_item;

/*
 * HUD Connector Shader
 * Purpose: Core stroke + rim highlight + scanline animation for spline connectors
 * Dependencies: Applied to Line2D material
 *
 * UV mapping on Line2D:
 *   UV.x = 0..1 along polyline length
 *   UV.y = 0..1 across line width
 */

// === HudTheme-pushed uniforms ===
uniform vec3 u_tint_color : source_color = vec3(0.8, 0.6, 0.2);
uniform vec3 u_rim_color : source_color = vec3(1.0, 0.8, 0.4);
uniform float u_alpha : hint_range(0.0, 1.0) = 0.6;
uniform float u_state = 0.0; // 0=normal, 1=hover, 2=active
uniform float u_rim_alpha = 1.0;

// === Primitive-specific uniforms ===
uniform float u_stroke_width = 36.0;
uniform float u_rim_px = 3.0;
uniform float u_scanline_speed = 0.0;
uniform float u_scanline_alpha = 0.3;

void fragment() {
	// Core stroke color
	vec3 core = u_tint_color;
	float alpha_out = u_alpha;

	// Rim highlight along one edge (UV.y near 0.0)
	float rim_band = smoothstep(0.0, 0.15, UV.y) * (1.0 - smoothstep(0.0, 0.15, UV.y));
	// Also add rim at both edges
	float edge_rim = 1.0 - smoothstep(0.0, 0.12, UV.y);
	edge_rim += 1.0 - smoothstep(0.88, 1.0, 1.0 - UV.y);
	edge_rim = clamp(edge_rim, 0.0, 1.0);

	core = mix(core, u_rim_color, edge_rim * u_rim_alpha * 0.4);

	// Scanline animation along UV.x
	if (u_scanline_speed > 0.0) {
		float scanline_pos = fract(UV.x - TIME * u_scanline_speed * 0.01);
		float scanline = smoothstep(0.0, 0.05, scanline_pos) * (1.0 - smoothstep(0.05, 0.1, scanline_pos));
		core = mix(core, u_rim_color, scanline * u_scanline_alpha);
	}

	// State modifiers
	if (u_state >= 1.0) {
		// Hover: brighten
		alpha_out = min(alpha_out + 0.2, 1.0);
		core += 0.05;
	}
	if (u_state >= 2.0) {
		// Active: pulse
		float pulse = sin(TIME * 4.0) * 0.1 + 0.1;
		alpha_out = min(alpha_out + pulse, 1.0);
	}

	COLOR = vec4(core, alpha_out);
}
