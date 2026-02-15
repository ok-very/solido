shader_type canvas_item;

/*
 * HUD Elbow Shader
 * Purpose: SDF-based L-shape with rounded outer corner and inner cutout
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
uniform float u_outer_radius_px = 96.0;
uniform float u_inner_radius_px = 48.0;
uniform float u_arm_h_px = 72.0; // Horizontal arm thickness
uniform float u_arm_v_px = 72.0; // Vertical arm thickness
uniform int u_rotation = 0; // 0=TL, 1=TR, 2=BR, 3=BL

void fragment() {
	vec2 pixel_size = 1.0 / TEXTURE_PIXEL_SIZE;
	vec2 pixel_pos = UV * pixel_size;

	// Transform coordinates based on rotation so we always compute as TL
	// TL: origin at top-left, H-arm extends right, V-arm extends down
	vec2 p = pixel_pos;
	if (u_rotation == 0) {
		// TL: no transform needed
		p = pixel_pos;
	} else if (u_rotation == 1) {
		// TR: mirror horizontally
		p = vec2(pixel_size.x - pixel_pos.x, pixel_pos.y);
	} else if (u_rotation == 2) {
		// BR: mirror both
		p = pixel_size - pixel_pos;
	} else {
		// BL: mirror vertically
		p = vec2(pixel_pos.x, pixel_size.y - pixel_pos.y);
	}

	// In TL space: outer corner is at top-left (0,0)
	// Horizontal arm: extends from 0 to pixel_size.x, height = u_arm_h_px, at top
	// Vertical arm: extends from 0 to u_arm_v_px, height = pixel_size.y, at left

	// Build L-shape as union of two rectangles
	// H-arm: centered at (pixel_size.x * 0.5, u_arm_h_px * 0.5)
	vec2 h_center = vec2(pixel_size.x * 0.5, u_arm_h_px * 0.5);
	vec2 h_half = vec2(pixel_size.x * 0.5, u_arm_h_px * 0.5);
	float h_dist = sdf_box(p - h_center, h_half);

	// V-arm: centered at (u_arm_v_px * 0.5, pixel_size.y * 0.5)
	vec2 v_center = vec2(u_arm_v_px * 0.5, pixel_size.y * 0.5);
	vec2 v_half = vec2(u_arm_v_px * 0.5, pixel_size.y * 0.5);
	float v_dist = sdf_box(p - v_center, v_half);

	// Union of both arms
	float shape_dist = sdf_union(h_dist, v_dist);

	// Round the outer corner (top-left in TL space)
	// Use a rounded rect that covers the corner region
	vec2 corner_center = vec2(u_outer_radius_px, u_outer_radius_px);
	float corner_dist = length(max(corner_center - p, 0.0)) - u_outer_radius_px;
	// Only apply rounding where it matters: in the corner quadrant
	if (p.x < u_outer_radius_px && p.y < u_outer_radius_px) {
		// Inside the corner zone, use the rounded corner
		shape_dist = max(shape_dist, corner_dist);
	}

	// Inner cutout: rounded rectangle at the inner corner
	// Inner corner is at (u_arm_v_px, u_arm_h_px) in TL space
	float inner_x = u_arm_v_px + u_inner_radius_px;
	float inner_y = u_arm_h_px + u_inner_radius_px;
	float inner_dist = length(max(p - vec2(u_arm_v_px, u_arm_h_px), 0.0)) - u_inner_radius_px;
	// Subtract the inner rounded area from the shape
	if (p.x > u_arm_v_px - u_inner_radius_px && p.y > u_arm_h_px - u_inner_radius_px) {
		shape_dist = max(shape_dist, -inner_dist);
	}

	// Discard outside shape
	if (shape_dist > 0.5) {
		discard;
	}

	// Base color
	vec4 color = vec4(u_tint_color, u_alpha);

	// Bevel
	vec3 bevel = sdf_bevel(shape_dist, u_bevel_px, UV, vec2(-0.7, -0.7));
	color = apply_bevel_to_color(color, bevel, u_bevel_scale, u_rim_alpha * 0.5, u_rim_color);

	// Rim
	float rim = sdf_rim(shape_dist, u_rim_px);
	color.rgb = mix(color.rgb, u_rim_color, rim * u_rim_alpha * 0.6);

	// Anti-aliasing at edge
	float aa = 1.0 - smoothstep(-0.5, 0.5, shape_dist);
	color.a *= aa;

	COLOR = color;
}
