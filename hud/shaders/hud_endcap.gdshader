shader_type canvas_item;

/*
 * HUD Endcap Shader
 * Purpose: SDF-based half-pill and stepped endcap for LCARS rails
 * Dependencies: hud_sdf.gdshaderinc
 */

#include "hud_sdf.gdshaderinc"

// === HudTheme-pushed uniforms ===
uniform vec3 u_tint_color : source_color = vec3(0.8, 0.6, 0.2);
uniform vec3 u_rim_color : source_color = vec3(1.0, 0.8, 0.4);
uniform float u_alpha : hint_range(0.0, 1.0) = 1.0;
uniform float u_rim_px = 3.0;
uniform float u_bevel_px = 24.0;
uniform float u_state = 0.0;
uniform float u_bevel_scale = 1.0;
uniform float u_rim_alpha = 1.0;

// === Primitive-specific uniforms ===
uniform float u_radius_px = 36.0;
uniform int u_style = 0; // 0 = HALF_PILL, 1 = STEPPED
uniform int u_direction = 0; // 0=LEFT, 1=RIGHT, 2=UP, 3=DOWN
uniform float u_step_depth_px = 24.0;
uniform float u_step_offset_px = 24.0;

void fragment() {
	vec2 pixel_size = 1.0 / TEXTURE_PIXEL_SIZE;
	vec2 pixel_pos = UV * pixel_size;
	vec2 center = pixel_size * 0.5;
	vec2 p = pixel_pos - center;
	vec2 half_size = center;

	// Base shape: half-pill (one end rounded, one flat)
	float dist = sdf_pill(p, half_size, u_radius_px, u_direction);

	// Stepped variant: subtract a rectangular notch
	if (u_style == 1) {
		// Compute notch position based on direction
		vec2 notch_center;
		vec2 notch_half;
		if (u_direction == 0) {
			// Pill faces left, notch near the left rounded end
			notch_center = vec2(-half_size.x + u_step_offset_px, 0.0);
			notch_half = vec2(u_step_depth_px * 0.5, half_size.y * 0.5);
		} else if (u_direction == 1) {
			// Pill faces right
			notch_center = vec2(half_size.x - u_step_offset_px, 0.0);
			notch_half = vec2(u_step_depth_px * 0.5, half_size.y * 0.5);
		} else if (u_direction == 2) {
			// Pill faces up
			notch_center = vec2(0.0, -half_size.y + u_step_offset_px);
			notch_half = vec2(half_size.x * 0.5, u_step_depth_px * 0.5);
		} else {
			// Pill faces down
			notch_center = vec2(0.0, half_size.y - u_step_offset_px);
			notch_half = vec2(half_size.x * 0.5, u_step_depth_px * 0.5);
		}
		float notch_dist = sdf_box(p - notch_center, notch_half);
		dist = sdf_subtract(dist, notch_dist);
	}

	// Discard outside shape
	if (dist > 0.5) {
		discard;
	}

	// Base color
	vec4 color = vec4(u_tint_color, u_alpha);

	// Bevel
	vec3 bevel = sdf_bevel(dist, u_bevel_px, UV, vec2(-0.7, -0.7));
	color = apply_bevel_to_color(color, bevel, u_bevel_scale, u_rim_alpha * 0.5, u_rim_color);

	// Rim
	float rim = sdf_rim(dist, u_rim_px);
	color.rgb = mix(color.rgb, u_rim_color, rim * u_rim_alpha * 0.6);

	// Anti-aliasing at edge
	float aa = 1.0 - smoothstep(-0.5, 0.5, dist);
	color.a *= aa;

	COLOR = color;
}
