/*
 * HUD SDF Utilities
 * Purpose: Shared signed distance field functions for HUD primitives
 * Used by: hud_endcap, hud_elbow, hud_glass
 */

// Axis-aligned box SDF
float sdf_box(vec2 p, vec2 half_size) {
	vec2 d = abs(p) - half_size;
	return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}

// Rounded rectangle SDF with per-corner radii
// radii = vec4(top_left, top_right, bottom_right, bottom_left)
float sdf_rounded_rect(vec2 p, vec2 half_size, vec4 radii) {
	// Select radius based on quadrant
	vec2 r_sel = (p.x > 0.0) ?
		((p.y > 0.0) ? radii.zw : radii.yw) :
		((p.y > 0.0) ? radii.xw : radii.xy);
	float radius = (p.x > 0.0) ?
		((p.y > 0.0) ? radii.z : radii.y) :
		((p.y > 0.0) ? radii.w : radii.x);
	vec2 q = abs(p) - half_size + radius;
	return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - radius;
}

// Half-pill SDF: one end rounded (radius = half_size.y for full pill), one flat
// direction: 0=pill on left, 1=pill on right, 2=pill on top, 3=pill on bottom
float sdf_pill(vec2 p, vec2 half_size, float radius, int direction) {
	vec4 radii;
	if (direction == 0) {
		// Pill faces left: round top-left and bottom-left
		radii = vec4(radius, 0.0, 0.0, radius);
	} else if (direction == 1) {
		// Pill faces right: round top-right and bottom-right
		radii = vec4(0.0, radius, radius, 0.0);
	} else if (direction == 2) {
		// Pill faces up: round top-left and top-right
		radii = vec4(radius, radius, 0.0, 0.0);
	} else {
		// Pill faces down: round bottom-left and bottom-right
		radii = vec4(0.0, 0.0, radius, radius);
	}
	return sdf_rounded_rect(p, half_size, radii);
}

// Boolean subtraction: d1 AND NOT d2
float sdf_subtract(float d1, float d2) {
	return max(d1, -d2);
}

// Boolean union
float sdf_union(float d1, float d2) {
	return min(d1, d2);
}

// Bevel computation from SDF distance
// Returns vec3(highlight, shadow, rim) intensities
// light_dir: normalized direction from which light comes (e.g., vec2(-0.7, -0.7) for top-left)
vec3 sdf_bevel(float dist, float bevel_width, vec2 uv, vec2 light_dir) {
	// Inner bevel zone: dist is negative (inside shape) and within bevel width
	float bevel_factor = smoothstep(-bevel_width, 0.0, dist);
	// Gradient direction approximated from UV
	float light_dot = dot(normalize(uv - 0.5), light_dir);
	float highlight = max(light_dot, 0.0) * (1.0 - bevel_factor);
	float shadow = max(-light_dot, 0.0) * (1.0 - bevel_factor);
	float rim = 1.0 - smoothstep(0.0, 1.5, abs(dist));
	return vec3(highlight, shadow, rim);
}

// Rim intensity from SDF distance
// Returns 1.0 at the boundary, smooth falloff
float sdf_rim(float dist, float rim_width) {
	return 1.0 - smoothstep(0.0, rim_width, abs(dist));
}

// Composite bevel onto base color
vec4 apply_bevel_to_color(vec4 base_color, vec3 bevel, float bevel_scale, float rim_color_mix, vec3 rim_color) {
	vec3 color = base_color.rgb;
	// Apply highlight (lighten)
	color += bevel.x * bevel_scale * 0.15;
	// Apply shadow (darken)
	color -= bevel.y * bevel_scale * 0.1;
	// Apply rim
	color = mix(color, rim_color, bevel.z * rim_color_mix);
	return vec4(color, base_color.a);
}
