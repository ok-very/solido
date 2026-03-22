// Solido v0.6 Composite pass + post-processing
//
// Combines video substrate background with biofield organisms
// sampled from the intermediate RGBA16Float texture.
// Post-processing: chromatic aberration, vignette, grain.
//
// Bindings:
//   group(0) binding(0): CompositeUniforms (uniform)
//   group(0) binding(1): biofield_texture (texture_2d<f32>)
//   group(0) binding(2): sampler
//   group(0) binding(3): video_texture (texture_2d<f32>) — decoded video substrate

struct CompositeUniforms {
    viewport:  vec2f,
    time:      f32,
    ca_amount: f32,    // chromatic aberration strength (0.0 = off)
}

@group(0) @binding(0) var<uniform>  cu:             CompositeUniforms;
@group(0) @binding(1) var           biofield_tex:   texture_2d<f32>;
@group(0) @binding(2) var           biofield_samp:  sampler;
@group(0) @binding(3) var           video_tex:      texture_2d<f32>;

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
// Background — video substrate with gaussian splat upsampling
// ============================================================================

fn video_substrate(uv: vec2f) -> vec3f {
    let video_dim = vec2f(textureDimensions(video_tex, 0));
    let has_video = video_dim.x > 1.5;  // 1×1 placeholder = no video

    if (has_video) {
        // Aspect-ratio-correct UV: cover the viewport (crop if necessary)
        let video_aspect = video_dim.x / video_dim.y;
        let viewport_aspect = cu.viewport.x / cu.viewport.y;

        var video_uv = uv;
        if (viewport_aspect > video_aspect) {
            let scale = viewport_aspect / video_aspect;
            video_uv.y = (uv.y - 0.5) / scale + 0.5;
        } else {
            let scale = video_aspect / viewport_aspect;
            video_uv.x = (uv.x - 0.5) / scale + 0.5;
        }
        video_uv = clamp(video_uv, vec2f(0.0), vec2f(1.0));

        // === Two-pass bloom: sharp detail + wide glow (light through pixel blocks) ===
        let texel = 1.0 / video_dim;

        // Pass 1: Tight gaussian (1-texel radius) — preserves detail
        let w0 = 0.38;
        let w1 = 0.24;
        let w2 = 0.07;

        var sharp = textureSample(video_tex, biofield_samp, video_uv).rgb * w0;
        sharp += textureSample(video_tex, biofield_samp, video_uv + vec2f(texel.x, 0.0)).rgb * w1;
        sharp += textureSample(video_tex, biofield_samp, video_uv - vec2f(texel.x, 0.0)).rgb * w1;
        sharp += textureSample(video_tex, biofield_samp, video_uv + vec2f(0.0, texel.y)).rgb * w1;
        sharp += textureSample(video_tex, biofield_samp, video_uv - vec2f(0.0, texel.y)).rgb * w1;
        sharp += textureSample(video_tex, biofield_samp, video_uv + texel).rgb * w2;
        sharp += textureSample(video_tex, biofield_samp, video_uv - texel).rgb * w2;
        sharp += textureSample(video_tex, biofield_samp, video_uv + vec2f(texel.x, -texel.y)).rgb * w2;
        sharp += textureSample(video_tex, biofield_samp, video_uv + vec2f(-texel.x, texel.y)).rgb * w2;

        // Pass 2: Wide glow (3-texel radius) — light bleeding through blocks
        let gt = texel * 3.0;
        var glow = textureSample(video_tex, biofield_samp, video_uv).rgb * 0.20;
        glow += textureSample(video_tex, biofield_samp, video_uv + vec2f(gt.x, 0.0)).rgb * 0.12;
        glow += textureSample(video_tex, biofield_samp, video_uv - vec2f(gt.x, 0.0)).rgb * 0.12;
        glow += textureSample(video_tex, biofield_samp, video_uv + vec2f(0.0, gt.y)).rgb * 0.12;
        glow += textureSample(video_tex, biofield_samp, video_uv - vec2f(0.0, gt.y)).rgb * 0.12;
        glow += textureSample(video_tex, biofield_samp, video_uv + gt).rgb * 0.08;
        glow += textureSample(video_tex, biofield_samp, video_uv - gt).rgb * 0.08;
        glow += textureSample(video_tex, biofield_samp, video_uv + vec2f(gt.x, -gt.y)).rgb * 0.08;
        glow += textureSample(video_tex, biofield_samp, video_uv + vec2f(-gt.x, gt.y)).rgb * 0.08;

        // Combine: sharp detail + additive glow (projected light effect)
        let col = sharp + glow * 0.6;

        return col;
    }

    // Fallback: subtle checkerboard when no video loaded
    let pixel = uv * cu.viewport;
    let size = 20.0;
    let c = floor(pixel / size);
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
// Fragment — composite biofield over video substrate with post-processing
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * cu.viewport;
    let bg = video_substrate(in.uv);

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
