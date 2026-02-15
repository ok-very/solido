/*
 * HUD Noise Utilities
 * Purpose: Hash and grain functions for glass/surface effects
 * Used by: hud_glass
 */

// Fast 2D -> 1D hash (no texture lookup)
float hash21(vec2 p) {
	p = fract(p * vec2(233.34, 851.74));
	p += dot(p, p + 23.45);
	return fract(p.x * p.y);
}

// Approximated blue noise using interleaved hash layers
// time parameter for temporal variation (avoids static grain pattern)
float blue_noise_approx(vec2 uv, float time) {
	float n1 = hash21(uv * 127.1 + time * 0.37);
	float n2 = hash21(uv * 269.5 + time * 0.71);
	float n3 = hash21(uv * 419.2 + time * 1.13);
	return (n1 + n2 + n3) / 3.0;
}

// Mix noise into base color as grain overlay
// Additive blend: base + (noise - 0.5) * intensity
// Preserves alpha channel
vec4 grain_overlay(vec4 base_color, float noise_value, float intensity) {
	vec3 grain = base_color.rgb + (noise_value - 0.5) * intensity;
	return vec4(grain, base_color.a);
}
