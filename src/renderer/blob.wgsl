// Solido v0.6 Blob SDF fragment shader — multi-lobe metaball organisms
//
// Each organism is composed of 1-12 circle SDF lobes blended with smooth
// minimum (smin). Per-organism thermal palette coloring driven by emotion
// arousal, with beat-synced pulsing and glow halos.
//
// Bindings:
//   0: BlobUniforms (uniform)
//   1: BlobOrgData[] (storage)
//   2: LobeGpu[] (storage)
//   3: font atlas texture
//   4: font sampler
//   5: TextGlyph[] (storage)

struct BlobUniforms {
  viewport: vec2f,
  time: f32,
  organism_count: f32,
  dpr: f32,
  beat_phase: f32,
  gravity_strength: f32,
  _pad: f32,
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
  _pad: f32,
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
// SDF primitives
// ============================================================================

fn sdCircle(p: vec2f, r: f32) -> f32 {
  return length(p) - r;
}

/// Smooth minimum (polynomial) — Inigo Quilez.
/// Blends two SDF values with C1 continuity.
fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = max(k - abs(a - b), 0.0) / k;
  return min(a, b) - h * h * 0.25 * k;
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

  // Inline color stops to avoid WGSL array-in-function limitations
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
// Organism SDF evaluation
// ============================================================================

/// Evaluate multi-lobe SDF for a single organism.
/// Each lobe is a circle SDF, blended with smin for smooth organic merging.
fn evalOrganism(pixel: vec2f, org: BlobOrgData) -> f32 {
  let lobe_count = org.lobe_count;
  if (lobe_count == 0u) {
    return 1e10;
  }

  // Beat pulse: scale all lobes slightly with beat phase
  let pulse = 1.0 + sin(org.pulse_phase * 6.28318) * org.pulse_amp * 0.1;

  var d = 1e10f;
  for (var i = 0u; i < lobe_count; i++) {
    let lobe = lobes[org.lobe_start + i];
    let p = pixel - org.pos - lobe.offset;
    let lobe_d = sdCircle(p, lobe.radius * pulse);
    d = smin(d, lobe_d, org.smin_k * lobe.radius);
  }

  return d;
}

// ============================================================================
// Text rendering via SDF font atlas (same as organism.wgsl)
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
  let org_count = i32(u.organism_count);

  var closest_d = 1e10f;
  var closest_org_idx = -1;

  // Evaluate all organisms, find closest distance per pixel
  for (var i = 0; i < org_count; i++) {
    let org = organisms[i];
    let d = evalOrganism(pixel, org);

    if (d < closest_d) {
      closest_d = d;
      closest_org_idx = i;
    }
  }

  let bg = checkerboard(pixel);

  // Early out: pixel is far from all organisms
  if (closest_org_idx < 0 || closest_d > 50.0) {
    return vec4f(bg, 1.0);
  }

  let org = organisms[closest_org_idx];

  // Edge fill with configurable softness
  let edge_w = org.edge_softness;
  let fill = 1.0 - smoothstep(0.0, edge_w, closest_d);

  // Glow halo outside the SDF boundary
  let glow_falloff = 0.03;
  let glow_intensity = exp(-max(closest_d, 0.0) * glow_falloff) * org.glow;

  // Thermal palette color
  var body_color = thermal_palette(org.thermal_temp);
  body_color = hue_rotate(body_color, org.hue_shift);

  // Glow color (slightly brighter/whiter version)
  let glow_color = mix(body_color, vec3f(1.0), 0.3) * glow_intensity;

  // Composite: background -> glow -> body fill
  var color = bg;
  color = color + glow_color * (1.0 - fill); // additive glow outside body
  color = mix(color, body_color, fill);       // body fill on top

  return vec4f(color, 1.0);
}

// ============================================================================
// Capture fragment shader — transparent background, premultiplied alpha
// ============================================================================

@fragment
fn fs_capture(in: VSOut) -> @location(0) vec4f {
  let pixel = in.uv * u.viewport;
  let org_count = i32(u.organism_count);

  var closest_d = 1e10f;
  var closest_org_idx = -1;

  for (var i = 0; i < org_count; i++) {
    let org = organisms[i];
    let d = evalOrganism(pixel, org);

    if (d < closest_d) {
      closest_d = d;
      closest_org_idx = i;
    }
  }

  if (closest_org_idx < 0 || closest_d > 50.0) {
    return vec4f(0.0, 0.0, 0.0, 0.0);
  }

  let org = organisms[closest_org_idx];

  let edge_w = org.edge_softness;
  let fill = 1.0 - smoothstep(0.0, edge_w, closest_d);

  let glow_falloff = 0.03;
  let glow_intensity = exp(-max(closest_d, 0.0) * glow_falloff) * org.glow;

  var body_color = thermal_palette(org.thermal_temp);
  body_color = hue_rotate(body_color, org.hue_shift);

  let glow_color = mix(body_color, vec3f(1.0), 0.3) * glow_intensity;

  // Premultiplied alpha
  let alpha = max(fill, glow_intensity * 0.5);
  var color = body_color * fill + glow_color * (1.0 - fill);
  color = color * alpha;

  return vec4f(color, alpha);
}
