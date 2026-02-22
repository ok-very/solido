// Solido v0.5 SDF fragment shader — fullscreen triangle, checkerboard + organism SDFs + text
//
// L-shape organisms built from a GROUP of rounded box primitives:
//   Primitive 1: Stem  — sdRoundedBox4, r at outer corners, 0 where bar joins
//   Primitive 2: Bar   — sdRoundedBox4, r at outer corners, 0 where stem joins
//   Compose:     Union — min(stem_d, bar_d), zero-artifact crisp join
//   Primitive 3: Fillet — circle cutout at interior junction for concave scoop
//
// Three-stage SDF pipeline:
//   STAGE 1 — BASE:     Analytical primitives composed with min() union
//   STAGE 2 — SUBTRACT: Analytical cutouts composed with max(d, -cutout)
//   STAGE 3 — ADD:      MTSDF texture elements composed with min(d, element_d)
//                        (future — safe because min() has no bounding-box seams)
//
// Corner rounding uses analytical sdRoundedBox4 (per-corner radii).
// MTSDF texture pipeline (sampleCornerSDF, applyCorner) is preserved for
// future Stage 3 additive elements only — NOT used for corners, because
// max(box_d, texture_d) creates visible seams at bounding-box edges due to
// pxrange saturation in the MTSDF texture.
//
// SDF font atlas for text labels inside organisms (MSDF median3 sampling).
// SDF references: Inigo Quilez, https://iquilezles.org/articles/distfunctions2d/

struct Uniforms {
  viewport: vec2f,
  time: f32,
  organism_count: f32,
  dpr: f32,
  _pad0: f32,
  _pad1: f32,
  _pad2: f32,
};

struct OrganismData {
  pos: vec2f,
  stem_size: vec2f,
  bar_offset: vec2f,
  bar_size: vec2f,
  corner_radius: f32,
  fillet_radius: f32,
  glyph_start: u32,
  glyph_count: u32,
};

struct TextGlyph {
  pos: vec2f,
  size: vec2f,
  uv_rect: vec4f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> organisms: array<OrganismData>;
@group(0) @binding(2) var font_atlas: texture_2d<f32>;
@group(0) @binding(3) var font_sampler: sampler;
@group(0) @binding(4) var<storage, read> glyphs: array<TextGlyph>;
@group(0) @binding(5) var shape_atlas: texture_2d<f32>;
@group(0) @binding(6) var shape_sampler: sampler;

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

/// Standard rounded box SDF (Inigo Quilez).
/// p: sample point, center: box center, half_size: half-extents, r: corner radius.
fn sdRoundedBox(p: vec2f, center: vec2f, half_size: vec2f, r: f32) -> f32 {
  let d = abs(p - center) - half_size + vec2f(r);
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

/// Rounded box with per-corner radii.
/// Adapted from Inigo Quilez (https://iquilezles.org/articles/distfunctions2d/).
/// p: sample point, center: box center, half_size: half-extents.
/// radii: corner radii in screen space (+y down):
///   x = top-right, y = bottom-right, z = bottom-left, w = top-left
fn sdRoundedBox4(p: vec2f, center: vec2f, half_size: vec2f, radii: vec4f) -> f32 {
  let lp = p - center;
  // Screen space (+y down): radii = (top-right, bottom-right, bottom-left, top-left)
  //
  // Step 1: pick the pair for the horizontal half.
  //   right half (lp.x > 0): radii.xy = (top-right, bottom-right)
  //   left half  (lp.x <= 0): radii.wz = (top-left, bottom-left)
  let rs = select(radii.wz, radii.xy, lp.x > 0.0);
  // rs is now (top_*, bottom_*) for whichever side we're on.
  //
  // Step 2: pick single radius for vertical half.
  //   top half (lp.y < 0 in screen space): rs.x (the "top" entry)
  //   bottom half (lp.y >= 0): rs.y (the "bottom" entry)
  let rv = select(rs.y, rs.x, lp.y < 0.0);
  let q = abs(lp) - half_size + vec2f(rv);
  return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - rv;
}

/// Sharp (unrounded) box SDF.
fn sdBox(p: vec2f, center: vec2f, half_size: vec2f) -> f32 {
  let d = abs(p - center) - half_size;
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0);
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
// MTSDF texture sampling (STAGE 3 — future additive elements only)
//
// NOT used for corner rounding (analytical sdRoundedBox4 handles that).
// These functions are preserved for future additive shape elements where
// min(base_d, element_d) composition is seam-free.
// ============================================================================

/// Sample the corner MTSDF texture and return signed distance in screen pixels.
/// corner_pos: top-left of the r×r corner bounding box (screen pixels).
/// r: corner radius in screen pixels.
/// flip: (0,0)=TL, (1,0)=TR, (0,1)=BL, (1,1)=BR orientation.
///
/// Maps corner region [0..r] to UV [0..1], then samples texture at [0..0.5]
/// (arc sits at UV 0.5). Texture range is limited by pxrange=8, so deep
/// inside the arc the texture SDF saturates. An analytical quarter-circle
/// SDF provides correct deep values via min(tex, analytical).
fn sampleCornerSDF(pixel: vec2f, corner_pos: vec2f, r: f32, flip: vec2f) -> f32 {
  var uv = (pixel - corner_pos) / r;
  uv = mix(uv, vec2f(1.0) - uv, flip);

  // Texture sampling: UV [0..1] → texture [0..0.5] (arc at 0.5)
  let tex_uv = clamp(uv * 0.5, vec2f(0.002), vec2f(0.498));
  let s = textureSample(shape_atlas, shape_sampler, tex_uv);
  let sd = median3(s.r, s.g, s.b);
  let tex_dist = (0.5 - sd) * r / 16.0;

  // Analytical quarter-circle SDF (arc center at UV (1,1), radius 1)
  // Extends the texture's limited range deep inside the arc.
  let analytical_dist = (length(uv - vec2f(1.0)) - 1.0) * r;

  // min: agrees near the arc; picks analytical deep inside where texture
  // saturates; caps outside slightly (invisible since both are positive).
  return min(tex_dist, analytical_dist);
}

/// Apply texture-based corner rounding at a single corner.
/// Only modifies d when the pixel is inside the r×r corner bounding box.
/// Interior edges (facing the organism center) get a 1.5px soft blend
/// to prevent fwidth derivative discontinuities that cause thin-line artifacts.
fn applyCorner(d: f32, pixel: vec2f, corner_pos: vec2f, r: f32, flip: vec2f) -> f32 {
  let local = pixel - corner_pos;
  let in_box = local.x >= 0.0 && local.y >= 0.0 && local.x <= r && local.y <= r;
  let corner_d = sampleCornerSDF(pixel, corner_pos, r, flip);

  // Interior edge distance: x=r when flip.x=0, x=0 when flip.x=1 (etc for y).
  let ix = mix(r - local.x, local.x, flip.x);
  let iy = mix(r - local.y, local.y, flip.y);
  let edge_blend = smoothstep(0.0, 1.5, min(ix, iy));

  let result = mix(d, max(d, corner_d), edge_blend);
  return select(d, result, in_box);
}

// ============================================================================
// Organism SDF evaluation (STAGE 1 base + STAGE 2 subtract)
// ============================================================================

/// Evaluate the L-shaped organism SDF.
///
///  org.pos (top-left)
///    +----stem_w---+---bar_w------+
///    |  TL   TR=0  | SCOOP        |
///    |  STEM       +~~fillet~~+   |  <- scoop_px
///    |             |    BAR    TR |
///    |  BL    BR=0 + BL=0    BR  |
///    +-------------+--------------+  <- stem_h
///
/// Analytical rounded-box union + analytical fillet.
/// Per-corner radii via sdRoundedBox4 (radii: TR, BR, BL, TL).
///
fn evalOrganism(pixel: vec2f, org: OrganismData) -> f32 {
  let r = org.corner_radius;
  let fr = org.fillet_radius;

  let stem_w = org.stem_size.x;
  let stem_h = org.stem_size.y;

  let stem_r = min(r, min(stem_w, stem_h) * 0.25);

  // --- Stem-only: all four corners rounded ---
  if (org.bar_size.x < 0.5) {
    let stem_center = org.pos + org.stem_size * 0.5;
    let stem_half = org.stem_size * 0.5;
    return sdRoundedBox4(pixel, stem_center, stem_half,
      vec4f(stem_r, stem_r, stem_r, stem_r));
  }

  // --- L-shape ---
  let bar_w = org.bar_size.x;
  let bar_h = org.bar_size.y;
  let scoop_px = org.bar_offset.y;
  let bar_r = min(r, min(bar_w, bar_h) * 0.25);

  // Stem: TL, BL rounded — TR=0 (scoop), BR=0 (joins bar)
  let stem_center = org.pos + vec2f(stem_w, stem_h) * 0.5;
  let stem_half = vec2f(stem_w, stem_h) * 0.5;
  let stem_d = sdRoundedBox4(pixel, stem_center, stem_half,
    vec4f(0.0, 0.0, stem_r, stem_r));

  // Bar: TR, BR rounded — BL=0 (joins stem), TL=0 (scoop)
  let bar_origin = org.pos + vec2f(stem_w, scoop_px);
  let bar_center = bar_origin + vec2f(bar_w, bar_h) * 0.5;
  let bar_half = vec2f(bar_w, bar_h) * 0.5;
  let bar_d = sdRoundedBox4(pixel, bar_center, bar_half,
    vec4f(bar_r, bar_r, 0.0, 0.0));

  // --- Stage 1: Base union ---
  var d = min(stem_d, bar_d);

  // --- Stage 2: Subtractive fillet ---
  if (fr > 0.5) {
    let junction = org.pos + vec2f(stem_w, scoop_px);
    let fillet_center = junction + vec2f(fr, fr);
    let circle_d = length(pixel - fillet_center) - fr;

    let corner_center = junction + vec2f(fr * 0.5, fr * 0.5);
    let corner_half = vec2f(fr * 0.5);
    let corner_d = sdBox(pixel, corner_center, corner_half);

    let cutout = max(corner_d, -circle_d);
    d = max(d, -cutout);
  }

  return d;
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
  let uv = vec2f(
    mix(glyph.uv_rect.x, glyph.uv_rect.z, t.x),
    mix(glyph.uv_rect.y, glyph.uv_rect.w, t.y),
  );
  let s = textureSample(font_atlas, font_sampler, uv);
  return median3(s.r, s.g, s.b);
}

fn evalOrganismText(pixel: vec2f, org: OrganismData) -> f32 {
  var text_alpha = 0.0;
  let start = org.glyph_start;
  let count = org.glyph_count;

  for (var i = 0u; i < count; i++) {
    let g = glyphs[start + i];
    let sdf_val = sampleGlyph(pixel, g);

    // Valve SDF text formula: sharp edge with ~1px AA
    let dist = sdf_val - 0.5;
    let aa = fwidth(dist);
    let glyph_alpha = clamp(dist / aa + 0.5, 0.0, 1.0);

    text_alpha = max(text_alpha, glyph_alpha);
  }

  return text_alpha;
}

// ============================================================================
// Fragment shader
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
  let pixel = in.uv * u.viewport;
  let org_count = i32(u.organism_count);

  var d_org = 1e10;
  var text_alpha = 0.0;

  for (var i = 0; i < org_count; i++) {
    let org = organisms[i];
    let org_d = evalOrganism(pixel, org);

    // Only sample text when close to or inside this organism
    if (org_d < 2.0 && org.glyph_count > 0u) {
      let ta = evalOrganismText(pixel, org);
      let org_aa = fwidth(org_d);
      let org_mask = 1.0 - smoothstep(0.0, org_aa, org_d);
      text_alpha = max(text_alpha, ta * org_mask);
    }

    d_org = min(d_org, org_d);
  }

  let bg = checkerboard(pixel);
  let org_color = vec3f(1.0, 0.9, 0.2);
  let text_color = vec3f(0.05, 0.04, 0.0);

  let aa = fwidth(d_org);
  let org_fill = 1.0 - smoothstep(0.0, aa, d_org);

  var color = bg;
  color = mix(color, org_color, org_fill);
  color = mix(color, text_color, text_alpha);

  return vec4f(color, 1.0);
}

// ============================================================================
// Capture fragment shader — transparent background, premultiplied alpha
// ============================================================================

@fragment
fn fs_capture(in: VSOut) -> @location(0) vec4f {
  let pixel = in.uv * u.viewport;
  let org_count = i32(u.organism_count);

  var d_org = 1e10;
  var text_alpha = 0.0;

  for (var i = 0; i < org_count; i++) {
    let org = organisms[i];
    let org_d = evalOrganism(pixel, org);

    if (org_d < 2.0 && org.glyph_count > 0u) {
      let ta = evalOrganismText(pixel, org);
      let org_aa = fwidth(org_d);
      let org_mask = 1.0 - smoothstep(0.0, org_aa, org_d);
      text_alpha = max(text_alpha, ta * org_mask);
    }

    d_org = min(d_org, org_d);
  }

  let org_color = vec3f(1.0, 0.9, 0.2);
  let text_color = vec3f(0.05, 0.04, 0.0);

  let aa = fwidth(d_org);
  let org_fill = 1.0 - smoothstep(0.0, aa, d_org);

  // Premultiplied alpha: rgb * alpha
  var color = org_color * org_fill;
  color = mix(color, text_color * org_fill, text_alpha);
  let alpha = org_fill;

  return vec4f(color, alpha);
}
