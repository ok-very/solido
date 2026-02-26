// Solido v0.6 Blob metaball fragment shader — additive potential fields
//
// Implements the Shadertoy-style metaball approach: each lobe emits a
// potential field  r / distance(pixel, lobe_center). All lobe potentials
// sum additively across all organisms. A threshold determines the blob
// boundary. Color is the potential-weighted average of organism colors.
// This naturally produces merging when organisms (or lobes) approach
// each other — no explicit cross-organism blending logic needed.
//
// Bindings:
//   0: BlobUniforms (uniform)
//   1: BlobOrgData[] (storage)
//   2: LobeGpu[] (storage)
//   3: font atlas texture
//   4: font sampler
//   5: TextGlyph[] (storage)

const MAX_ORGS: i32 = 16;

struct BlobUniforms {
  viewport: vec2f,
  time: f32,
  organism_count: f32,
  dpr: f32,
  beat_phase: f32,
  gravity_strength: f32,
  cross_smin_k: f32,      // repurposed as metaball threshold
};

struct BlobOrgData {
  pos: vec2f,
  smin_k: f32,
  edge_softness: f32,
  thermal_temp: f32,
  hue_shift: f32,
  pulse_phase: f32,
  pulse_amp: f32,
  glow: f32,
  lobe_start: u32,
  lobe_count: u32,
  glob_group: u32,
};

struct LobeData {
  offset: vec2f,
  radius: f32,
  _pad: f32,
};

struct TextGlyph {
  pos: vec2f,
  size: vec2f,
  uv_rect: vec4f,
};

@group(0) @binding(0) var<uniform> u: BlobUniforms;
@group(0) @binding(1) var<storage, read> organisms: array<BlobOrgData>;
@group(0) @binding(2) var<storage, read> lobes: array<LobeData>;
@group(0) @binding(3) var font_atlas: texture_2d<f32>;
@group(0) @binding(4) var font_sampler: sampler;
@group(0) @binding(5) var<storage, read> glyphs: array<TextGlyph>;

// ============================================================================
// Fullscreen triangle
// ============================================================================

struct VSOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
  let x = f32(i32(vi) / 2) * 4.0 - 1.0;
  let y = f32(i32(vi) % 2) * 4.0 - 1.0;
  var out: VSOut;
  out.pos = vec4f(x, y, 0.0, 1.0);
  out.uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
  return out;
}

// ============================================================================
// Thermal palette
// ============================================================================

/// 8-stop thermal palette: black -> indigo -> blue -> cyan -> green -> yellow -> orange -> white
fn thermal_palette(t: f32) -> vec3f {
  let tc = clamp(t, 0.0, 1.0);
  let idx = tc * 7.0;
  let i = u32(floor(idx));
  let f = fract(idx);

  var c0: vec3f;
  var c1: vec3f;

  switch(i) {
    case 0u: { c0 = vec3f(0.0, 0.0, 0.0);     c1 = vec3f(0.18, 0.0, 0.35); }
    case 1u: { c0 = vec3f(0.18, 0.0, 0.35);    c1 = vec3f(0.0, 0.0, 0.8); }
    case 2u: { c0 = vec3f(0.0, 0.0, 0.8);      c1 = vec3f(0.0, 0.7, 0.9); }
    case 3u: { c0 = vec3f(0.0, 0.7, 0.9);      c1 = vec3f(0.1, 0.8, 0.2); }
    case 4u: { c0 = vec3f(0.1, 0.8, 0.2);      c1 = vec3f(1.0, 0.95, 0.2); }
    case 5u: { c0 = vec3f(1.0, 0.95, 0.2);     c1 = vec3f(1.0, 0.5, 0.0); }
    case 6u: { c0 = vec3f(1.0, 0.5, 0.0);      c1 = vec3f(1.0, 1.0, 1.0); }
    default: { c0 = vec3f(1.0, 1.0, 1.0);      c1 = vec3f(1.0, 1.0, 1.0); }
  }

  return mix(c0, c1, f);
}

/// Simple hue rotation in RGB space (fast approximation).
fn hue_rotate(color: vec3f, angle: f32) -> vec3f {
  let cos_a = cos(angle * 6.28318);
  let sin_a = sin(angle * 6.28318);
  let k = vec3f(0.57735);
  return color * cos_a + cross(k, color) * sin_a + k * dot(k, color) * (1.0 - cos_a);
}

/// Compute thermal-palette color for an organism.
fn org_color(org: BlobOrgData) -> vec3f {
  var c = thermal_palette(org.thermal_temp);
  c = hue_rotate(c, org.hue_shift);
  return max(c, vec3f(0.0));
}

// ============================================================================
// Background
// ============================================================================

fn checkerboard(pixel: vec2f) -> vec3f {
  let size = 16.0 * u.dpr;
  let c = floor(pixel / size);
  let parity = (i32(c.x) + i32(c.y)) % 2;
  let dark  = vec3f(0.035);
  let light = vec3f(0.075);
  return select(dark, light, parity == 1);
}

// ============================================================================
// RGB <-> HSL conversion (for saturation-preserving color blending)
// ============================================================================

fn rgb_to_hsl(c: vec3f) -> vec3f {
  let mx = max(c.r, max(c.g, c.b));
  let mn = min(c.r, min(c.g, c.b));
  let l = (mx + mn) * 0.5;
  let d = mx - mn;

  if (d < 0.0001) {
    return vec3f(0.0, 0.0, l);
  }

  let s = select(d / (2.0 - mx - mn), d / (mx + mn), l < 0.5);

  var h = 0.0;
  if (mx == c.r) {
    h = (c.g - c.b) / d + select(0.0, 6.0, c.g < c.b);
  } else if (mx == c.g) {
    h = (c.b - c.r) / d + 2.0;
  } else {
    h = (c.r - c.g) / d + 4.0;
  }
  h /= 6.0;

  return vec3f(h, s, l);
}

fn hue_to_rgb(p: f32, q: f32, t_in: f32) -> f32 {
  var t = t_in;
  if (t < 0.0) { t += 1.0; }
  if (t > 1.0) { t -= 1.0; }
  if (t < 1.0 / 6.0) { return p + (q - p) * 6.0 * t; }
  if (t < 0.5)        { return q; }
  if (t < 2.0 / 3.0)  { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
  return p;
}

fn hsl_to_rgb(hsl: vec3f) -> vec3f {
  let h = hsl.x;
  let s = hsl.y;
  let l = hsl.z;

  if (s < 0.0001) {
    return vec3f(l, l, l);
  }

  let q = select(l + s - l * s, l * (1.0 + s), l < 0.5);
  let p = 2.0 * l - q;

  return vec3f(
    hue_to_rgb(p, q, h + 1.0 / 3.0),
    hue_to_rgb(p, q, h),
    hue_to_rgb(p, q, h - 1.0 / 3.0),
  );
}

// ============================================================================
// Additive potential field evaluation (Shadertoy metaball approach)
// ============================================================================

struct FieldHit {
  total: f32,        // summed potential from all lobes
  color: vec3f,      // final blended RGB color
  glow: f32,         // max glow across contributing organisms
};

/// Evaluate the global potential field at a pixel.
///
/// Each lobe contributes  r / distance(pixel, lobe_center)  to the field.
/// All lobes from all organisms add to a single global potential.
/// Color is blended in HSL space (preserving saturation during overlap).
fn evalField(pixel: vec2f) -> FieldHit {
  let org_count = min(i32(u.organism_count), MAX_ORGS);

  var total = 0.0;
  // Accumulate HSL components weighted by potential
  var weighted_h_sin = 0.0;  // circular hue average via sin/cos
  var weighted_h_cos = 0.0;
  var weighted_s = 0.0;
  var weighted_l = 0.0;
  var max_glow = 0.0;

  for (var i = 0; i < org_count; i++) {
    let org = organisms[i];
    let col = org_color(org);
    let pulse = 1.0 + sin(org.pulse_phase * 6.28318) * org.pulse_amp * 0.3;

    var org_field = 0.0;
    for (var j = 0u; j < org.lobe_count; j++) {
      let lobe = lobes[org.lobe_start + j];
      let p = pixel - org.pos - lobe.offset;
      let dist = length(p);
      let r = lobe.radius * pulse;
      let potential = r / max(dist, 0.5);
      org_field += potential;
    }

    total += org_field;
    let hsl = rgb_to_hsl(col);
    // Circular hue averaging (prevents 0.0+1.0 = 0.5 problem)
    let h_angle = hsl.x * 6.28318;
    weighted_h_sin += sin(h_angle) * org_field;
    weighted_h_cos += cos(h_angle) * org_field;
    weighted_s += hsl.y * org_field;
    weighted_l += hsl.z * org_field;
    max_glow = max(max_glow, org.glow);
  }

  var final_color = vec3f(0.5);
  if (total > 0.001) {
    let avg_h = atan2(weighted_h_sin, weighted_h_cos) / 6.28318;
    let avg_s = weighted_s / total;
    let avg_l = weighted_l / total;
    // Normalize hue to [0, 1]
    let h_norm = select(avg_h, avg_h + 1.0, avg_h < 0.0);
    final_color = hsl_to_rgb(vec3f(h_norm, avg_s, avg_l));
  }

  return FieldHit(total, final_color, max_glow);
}

// ============================================================================
// Text rendering via SDF font atlas
// ============================================================================

fn median3(r: f32, g: f32, b: f32) -> f32 {
  return max(min(r, g), min(max(r, g), b));
}

fn sampleGlyph(pixel: vec2f, glyph: TextGlyph) -> f32 {
  let local = pixel - glyph.pos;
  if (local.x < 0.0 || local.y < 0.0 ||
      local.x > glyph.size.x || local.y > glyph.size.y) {
    return 0.0;
  }
  let t = local / glyph.size;
  let uv_xy = vec2f(glyph.uv_rect.x, glyph.uv_rect.y);
  let uv_zw = vec2f(glyph.uv_rect.z, glyph.uv_rect.w);
  let sample_uv = mix(uv_xy, uv_zw, t);
  let s = textureSample(font_atlas, font_sampler, sample_uv);
  return median3(s.r, s.g, s.b);
}

// ============================================================================
// Fragment shader
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
  let pixel = in.uv * u.viewport;
  let hit = evalField(pixel);

  let bg = checkerboard(pixel);

  // Metaball threshold — from cross_smin_k uniform
  let threshold = u.cross_smin_k;

  // Early out: pixel too far from all organisms
  if (hit.total < threshold * 0.3) {
    return vec4f(bg, 1.0);
  }

  // Body color: HSL-blended, already normalized in evalField
  let body_color = hit.color;

  // Smooth fill at threshold boundary
  let edge_width = threshold * 0.12;
  let fill = smoothstep(threshold - edge_width, threshold, hit.total);

  // Glow halo outside boundary
  let glow_zone = smoothstep(threshold * 0.2, threshold * 0.8, hit.total);
  let glow_intensity = glow_zone * (1.0 - fill) * hit.glow;
  let glow_color = mix(body_color, vec3f(1.0), 0.3) * glow_intensity;

  // Composite: background -> glow -> body fill
  var color = bg;
  color = color + glow_color;
  color = mix(color, body_color, fill);

  return vec4f(color, 1.0);
}

// ============================================================================
// Capture fragment shader — transparent background, premultiplied alpha
// ============================================================================

@fragment
fn fs_capture(in: VSOut) -> @location(0) vec4f {
  let pixel = in.uv * u.viewport;
  let hit = evalField(pixel);

  let threshold = u.cross_smin_k;

  if (hit.total < threshold * 0.3) {
    return vec4f(0.0, 0.0, 0.0, 0.0);
  }

  let body_color = hit.color;

  let edge_width = threshold * 0.12;
  let fill = smoothstep(threshold - edge_width, threshold, hit.total);

  let glow_zone = smoothstep(threshold * 0.2, threshold * 0.8, hit.total);
  let glow_intensity = glow_zone * (1.0 - fill) * hit.glow;
  let glow_color = mix(body_color, vec3f(1.0), 0.3) * glow_intensity;

  // Premultiplied alpha
  let alpha = max(fill, glow_intensity * 0.5);
  var color = body_color * fill + glow_color * (1.0 - fill);
  color = color * alpha;

  return vec4f(color, alpha);
}
