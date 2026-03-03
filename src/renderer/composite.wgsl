// Solido v0.6 Composite pass
//
// Combines background (generated inline) with biofield organisms
// sampled from the intermediate RGBA16Float texture.
//
// Bindings:
//   group(0) binding(0): CompositeUniforms (uniform)
//   group(0) binding(1): biofield_texture (texture_2d<f32>)
//   group(0) binding(2): sampler

struct CompositeUniforms {
    viewport: vec2f,
    time:     f32,
    _pad:     f32,
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
// Fragment — composite biofield over background
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * cu.viewport;
    let bg    = checkerboard(pixel);

    // Sample biofield texture (premultiplied alpha)
    let bio = textureSample(biofield_tex, biofield_samp, in.uv);

    // Un-premultiply for compositing, then blend over background
    // For premultiplied alpha compositing: result = bio + bg * (1 - bio.a)
    let final_col = bio.rgb + bg * (1.0 - bio.a);

    return vec4f(final_col, 1.0);
}
