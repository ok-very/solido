// Solido v0.6 Composite pass + post-processing
//
// Combines background (generated inline) with biofield organisms
// sampled from the intermediate RGBA16Float texture.
// Post-processing: chromatic aberration, vignette, grain.
//
// Bindings:
//   group(0) binding(0): CompositeUniforms (uniform)
//   group(0) binding(1): biofield_texture (texture_2d<f32>)
//   group(0) binding(2): sampler

struct CompositeUniforms {
    viewport:  vec2f,
    time:      f32,
    ca_amount: f32,    // chromatic aberration strength (0.0 = off)
}

@group(0) @binding(0) var<uniform>  cu:             CompositeUniforms;
@group(0) @binding(1) var           biofield_tex:   texture_2d<f32>;
@group(0) @binding(2) var           biofield_samp:  sampler;

// ============================================================================
// Fullscreen triangle
// ============================================================================

struct VSOut {
    @builtin(position) pos: vec4f,
    @location(0)       uv:  vec2f,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
    let x = f32(i32(vi) / 2) * 4.0 - 1.0;
    let y = f32(i32(vi) % 2) * 4.0 - 1.0;
    var out: VSOut;
    out.pos = vec4f(x, y, 0.0, 1.0);
    out.uv  = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// ============================================================================
// Background — subtle checkerboard
// ============================================================================

fn checkerboard(pixel: vec2f) -> vec3f {
    let size   = 20.0;
    let c      = floor(pixel / size);
    let parity = (i32(c.x) + i32(c.y)) % 2;
    return select(vec3f(0.030), vec3f(0.048), parity == 1);
}

// ============================================================================
// Post-processing utilities
// ============================================================================

// Hash for film grain
fn hash11(p: f32) -> f32 {
    return fract(sin(p * 12.9898) * 43758.5453);
}

// Chromatic aberration — 3-tap radial
fn chromatic_aberration(uv: vec2f, amount: f32) -> vec3f {
    let center_uv = uv - 0.5;
    let r = textureSample(biofield_tex, biofield_samp, 0.5 + center_uv * (1.0 - amount * 0.003)).r;
    let g = textureSample(biofield_tex, biofield_samp, uv).g;
    let b = textureSample(biofield_tex, biofield_samp, 0.5 + center_uv * (1.0 + amount * 0.003)).b;
    return vec3f(r, g, b);
}

// Vignette
fn vignette(uv: vec2f) -> f32 {
    let p = uv * 2.0 - 1.0;
    let v = 1.25 / (1.1 + 1.1 * dot(p, p));
    return mix(1.0, smoothstep(0.1, 1.1, v * v), 0.2);
}

// ============================================================================
// Fragment — composite biofield over background with post-processing
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * cu.viewport;
    let bg = checkerboard(pixel);

    // Direct sample — SDF blobs are analytically smooth, no AA needed
    let bio_raw = textureSample(biofield_tex, biofield_samp, in.uv);
    let bio_rgb = bio_raw.rgb;
    let bio_a = bio_raw.a;

    // Chromatic aberration (only on the biofield, not background)
    var scene_rgb = bio_rgb;
    if (cu.ca_amount > 0.001) {
        scene_rgb = chromatic_aberration(in.uv, cu.ca_amount);
    }

    // Composite over background (premultiplied alpha)
    let composited = scene_rgb + bg * (1.0 - bio_a);

    var final_col = composited;

    // Vignette
    final_col *= vignette(in.uv);

    // Film grain
    let grain = 0.012 * hash11(dot(pixel, vec2f(1.0, 317.0)) + cu.time * 0.1);
    final_col += vec3f(grain);

    return vec4f(final_col, 1.0);
}
