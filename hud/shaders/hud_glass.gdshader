shader_type canvas_item;

/*
 * HUD Glass Shader
 * Purpose: Screen-read blur + tint + grain + SDF bevel/rim for glass pane surfaces
 * Dependencies: hud_sdf.gdshaderinc, hud_noise.gdshaderinc
 */

#include "hud_sdf.gdshaderinc"
#include "hud_noise.gdshaderinc"

// === Screen texture sampling ===
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

// === HudTheme-pushed uniforms ===
uniform vec3 u_tint_color : source_color = vec3(0.8, 0.6, 0.2);
uniform vec3 u_glass_tint : source_color = vec3(0.15, 0.12, 0.05);
uniform vec3 u_rim_color : source_color = vec3(1.0, 0.8, 0.4);
uniform float u_alpha : hint_range(0.0, 1.0) = 0.33;
uniform float u_rim_px = 3.0;
uniform float u_bevel_px = 24.0;
uniform float u_state = 0.0;
uniform float u_bevel_scale = 1.0;
uniform float u_rim_alpha = 1.0;

// === Primitive-specific uniforms ===
uniform float u_blur_lod = 2.0;
uniform float u_grain = 0.03;
uniform float u_radius_px = 48.0;
uniform float u_contrast_min = 0.0;

void fragment() {
	vec2 pixel_size = 1.0 / TEXTURE_PIXEL_SIZE;
	vec2 pixel_pos = UV * pixel_size;
	vec2 center = pixel_size * 0.5;
	vec2 p = pixel_pos - center;
	vec2 half_size = center;

	// SDF for rounded rectangle shape
	vec4 radii = vec4(u_radius_px, u_radius_px, u_radius_px, u_radius_px);
	float dist = sdf_rounded_rect(p, half_size, radii);

	// Discard outside shape
	if (dist > 0.5) {
		discard;
	}

	// Sample screen behind this pane (blurred)
	vec4 screen_color;
	if (u_blur_lod > 0.0) {
		screen_color = textureLod(SCREEN_TEXTURE, SCREEN_UV, u_blur_lod);
	} else {
		screen_color = texture(SCREEN_TEXTURE, SCREEN_UV);
	}

	// Tint the blurred background
	vec3 tinted = mix(screen_color.rgb, u_glass_tint, u_alpha);

	// Auto-contrast: thicken opacity when background luminance is close to tint
	if (u_contrast_min > 0.0) {
		float bg_lum = dot(screen_color.rgb, vec3(0.299, 0.587, 0.114));
		float tint_lum = dot(u_glass_tint, vec3(0.299, 0.587, 0.114));
		float lum_diff = abs(bg_lum - tint_lum);
		if (lum_diff < u_contrast_min) {
			float boost = (1.0 - lum_diff / u_contrast_min) * 0.15;
			tinted = mix(tinted, u_glass_tint, u_alpha + boost);
		}
	}

	vec4 color = vec4(tinted, 1.0);

	// Grain overlay
	if (u_grain > 0.0) {
		float noise = blue_noise_approx(UV * pixel_size, TIME);
		color = grain_overlay(color, noise, u_grain);
	}

	// Bevel
	vec3 bevel = sdf_bevel(dist, u_bevel_px, UV, vec2(-0.7, -0.7));
	color = apply_bevel_to_color(color, bevel, u_bevel_scale, u_rim_alpha * 0.4, u_rim_color);

	// Rim
	float rim = sdf_rim(dist, u_rim_px);
	color.rgb = mix(color.rgb, u_rim_color, rim * u_rim_alpha * 0.5);

	// Anti-aliasing at edge
	float aa = 1.0 - smoothstep(-0.5, 0.5, dist);
	color.a *= aa;

	COLOR = color;
}
