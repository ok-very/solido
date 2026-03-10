// Solido v0.6 fluid simulation shader — velocity field + Navier-Stokes + RGB paint trails
//
// Runs at ½ viewport resolution. Multiple entry points for each simulation pass.
// Shares CellData layout with biofield.wgsl.

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PI: f32  = 3.14159265;
const MAX_CELLS: i32 = 128;

// Curl noise (matches biofield.wgsl)
const CURL_FINE_SCALE: f32   = 0.02;
const CURL_COARSE_SCALE: f32 = 0.005;
const CURL_SPEED: f32        = 0.3;

// Fluid parameters
const VISCOSITY: f32       = 0.001;
const CURL_FORCE: f32      = 0.15;    // curl noise force strength (reduced from 0.4)
const ORG_IMPULSE: f32     = 12.0;    // organism velocity impulse strength (raised for visible trail spread)
const ORG_RADIUS_SCALE: f32 = 1.5;    // impulse kernel radius multiplier
const VELOCITY_DECAY: f32  = 0.98;    // per-frame velocity damping (stronger from 0.995)

// Trail stamp — steep logistic for defined body-boundary emission
const STAMP_RADIUS: f32    = 0.8;     // stamp extends to 80% of organism radius
const STAMP_STEEPNESS: f32 = 12.0;    // logistic steepness (higher = sharper edge)

// Pressure solver
const SOR_OMEGA: f32 = 1.6;

// ---------------------------------------------------------------------------
// Data layout
// ---------------------------------------------------------------------------

struct FluidUniforms {
    viewport:   vec2f,   // full viewport size (fluid tex is ½)
    time:       f32,
    dt:         f32,
    texel_size: vec2f,   // 1.0 / fluid_resolution
    cell_count: f32,
    _pad:       f32,
}

struct CellData {
    pos:             vec2f,
    radius:          f32,
    audio_energy:    f32,
    cell_id:         u32,
    hue:             f32,
    vel:             vec2f,
    harmonic_count:  f32,
    ring_phase:      f32,
    shape_amplitude: f32,
    shape_frequency: f32,
    harmonic_amp:    f32,
    rd_fkr:          u32,
    elongation:      f32,
    rd_scale:        f32,
}

// ---------------------------------------------------------------------------
// Fullscreen triangle vertex shader (shared by all fluid passes)
// ---------------------------------------------------------------------------

struct VSOut {
    @builtin(position) pos: vec4f,
    @location(0)       uv:  vec2f,
}

@vertex
fn vs_fluid(@builtin(vertex_index) vi: u32) -> VSOut {
    let x = f32(i32(vi) / 2) * 4.0 - 1.0;
    let y = f32(i32(vi) % 2) * 4.0 - 1.0;
    var out: VSOut;
    out.pos = vec4f(x, y, 0.0, 1.0);
    out.uv  = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// ---------------------------------------------------------------------------
// 2D Simplex noise (same as biofield.wgsl — needed for curl noise force)
// ---------------------------------------------------------------------------

fn mod289_v3(x: vec3f) -> vec3f {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn mod289_v2(x: vec2f) -> vec2f {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn permute(x: vec3f) -> vec3f {
    return mod289_v3(((x * 34.0) + vec3f(1.0)) * x);
}

fn snoise2(v: vec2f) -> f32 {
    let C = vec4f(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    let s = dot(v, vec2f(C.y));
    var i = floor(v + vec2f(s));
    let us = dot(i, vec2f(C.x));
    let x0 = v - i + vec2f(us);
    let i1 = select(vec2f(0.0, 1.0), vec2f(1.0, 0.0), x0.x > x0.y);
    let x1 = x0 + vec2f(C.x) - i1;
    let x2 = x0 + vec2f(C.z);
    i = mod289_v2(i);
    let p = permute(permute(
                vec3f(i.y) + vec3f(0.0, i1.y, 1.0))
              + vec3f(i.x) + vec3f(0.0, i1.x, 1.0));
    var m = max(vec3f(0.5) - vec3f(dot(x0, x0), dot(x1, x1), dot(x2, x2)), vec3f(0.0));
    m = m * m;
    m = m * m;
    let x_ = 2.0 * fract(p * C.w) - vec3f(1.0);
    let h  = abs(x_) - vec3f(0.5);
    let ox = floor(x_ + vec3f(0.5));
    let a0 = x_ - ox;
    m *= vec3f(1.79284291400159) - 0.85373472095314 * (a0 * a0 + h * h);
    let gx = vec3f(a0.x * x0.x + h.x * x0.y,
                   a0.y * x1.x + h.y * x1.y,
                   a0.z * x2.x + h.z * x2.y);
    return 130.0 * dot(m, gx);
}

fn curl_noise(p: vec2f, t: f32) -> vec2f {
    let eps = 1.0;
    let pf = p * CURL_FINE_SCALE + vec2f(t * CURL_SPEED, t * CURL_SPEED * 0.7);
    let nf_dx = snoise2(pf + vec2f(eps, 0.0)) - snoise2(pf - vec2f(eps, 0.0));
    let nf_dy = snoise2(pf + vec2f(0.0, eps)) - snoise2(pf - vec2f(0.0, eps));
    let curl_fine = vec2f(nf_dy, -nf_dx);
    let pc = p * CURL_COARSE_SCALE + vec2f(t * CURL_SPEED * 0.3, -t * CURL_SPEED * 0.2);
    let nc_dx = snoise2(pc + vec2f(eps, 0.0)) - snoise2(pc - vec2f(eps, 0.0));
    let nc_dy = snoise2(pc + vec2f(0.0, eps)) - snoise2(pc - vec2f(0.0, eps));
    let curl_coarse = vec2f(nc_dy, -nc_dx);
    return normalize(curl_fine * 0.6 + curl_coarse * 0.4 + vec2f(0.001, 0.001));
}

// ============================================================================
// Pass 1: Velocity self-advection (semi-Lagrangian backtrace)
// ============================================================================
//
// Bind group: uniform, velocity_in (texture), sampler

@group(0) @binding(0) var<uniform>       fu: FluidUniforms;
@group(0) @binding(1) var vel_tex_in:    texture_2d<f32>;
@group(0) @binding(2) var vel_sampler:   sampler;
@group(0) @binding(3) var<storage, read> fluid_cells: array<CellData>;

@fragment
fn fs_velocity_advect(in: VSOut) -> @location(0) vec4f {
    let vel = textureSample(vel_tex_in, vel_sampler, in.uv).rg;
    // Backtrace: where did this fluid come from?
    let uv_src = in.uv - fu.dt * vel * fu.texel_size;
    let advected = textureSample(vel_tex_in, vel_sampler, uv_src).rg;
    // Decay to prevent runaway
    return vec4f(advected * VELOCITY_DECAY, 0.0, 1.0);
}

// ============================================================================
// Pass 2: Force injection (curl noise + organism impulses)
// ============================================================================

@fragment
fn fs_force_inject(in: VSOut) -> @location(0) vec4f {
    let vel = textureSample(vel_tex_in, vel_sampler, in.uv).rg;
    // Pixel position in full viewport coords
    let pixel = in.uv * fu.viewport;

    // Curl noise turbulence force
    let curl = curl_noise(pixel, fu.time);
    var force = curl * CURL_FORCE;

    // Organism velocity impulses — Gaussian kernel
    let n = min(i32(fu.cell_count), MAX_CELLS);
    for (var i = 0i; i < n; i++) {
        let org_pos = fluid_cells[i].pos;
        let org_vel = fluid_cells[i].vel;
        let org_r   = fluid_cells[i].radius * ORG_RADIUS_SCALE;
        let energy  = fluid_cells[i].audio_energy;

        let delta = pixel - org_pos;
        let dist2 = dot(delta, delta);
        let r2 = org_r * org_r;

        // Gaussian falloff
        let weight = exp(-dist2 / (2.0 * r2));

        // Push fluid in organism's direction of travel, scaled by energy
        force += org_vel * ORG_IMPULSE * weight * (0.3 + energy * 0.7);
    }

    return vec4f(vel + force * fu.dt, 0.0, 1.0);
}

// ============================================================================
// Pass 3: Divergence  (∇·u)
// ============================================================================
//
// Reads velocity texture, writes R16F divergence

@fragment
fn fs_divergence(in: VSOut) -> @location(0) vec4f {
    let ts = fu.texel_size;
    let u_r = textureSample(vel_tex_in, vel_sampler, in.uv + vec2f(ts.x, 0.0)).r;
    let u_l = textureSample(vel_tex_in, vel_sampler, in.uv - vec2f(ts.x, 0.0)).r;
    let v_t = textureSample(vel_tex_in, vel_sampler, in.uv + vec2f(0.0, ts.y)).g;
    let v_b = textureSample(vel_tex_in, vel_sampler, in.uv - vec2f(0.0, ts.y)).g;
    let div = (u_r - u_l + v_t - v_b) * 0.5;
    return vec4f(div, 0.0, 0.0, 1.0);
}

// ============================================================================
// Pass 4-9: Pressure SOR relaxation
// ============================================================================
//
// Reads pressure_in + divergence, writes pressure_out
// group(0) binding(1) = pressure_in, binding(4) = divergence

@group(0) @binding(4) var div_tex: texture_2d<f32>;

@fragment
fn fs_pressure_sor(in: VSOut) -> @location(0) vec4f {
    let ts = fu.texel_size;
    // Read 4 neighbors from pressure field
    let p_r = textureSample(vel_tex_in, vel_sampler, in.uv + vec2f(ts.x, 0.0)).r;
    let p_l = textureSample(vel_tex_in, vel_sampler, in.uv - vec2f(ts.x, 0.0)).r;
    let p_t = textureSample(vel_tex_in, vel_sampler, in.uv + vec2f(0.0, ts.y)).r;
    let p_b = textureSample(vel_tex_in, vel_sampler, in.uv - vec2f(0.0, ts.y)).r;
    let p_c = textureSample(vel_tex_in, vel_sampler, in.uv).r;

    let div = textureSample(div_tex, vel_sampler, in.uv).r;

    // Jacobi step
    let jacobi = (p_l + p_r + p_b + p_t - div) * 0.25;

    // SOR: over-relax
    let p_new = mix(p_c, jacobi, SOR_OMEGA);
    return vec4f(p_new, 0.0, 0.0, 1.0);
}

// ============================================================================
// Pass 10: Gradient subtraction (pressure → velocity correction)
// ============================================================================
//
// group(0) binding(1) = velocity_in, binding(4) = pressure

@fragment
fn fs_gradient_sub(in: VSOut) -> @location(0) vec4f {
    let ts = fu.texel_size;
    let vel = textureSample(vel_tex_in, vel_sampler, in.uv).rg;

    let p_r = textureSample(div_tex, vel_sampler, in.uv + vec2f(ts.x, 0.0)).r;
    let p_l = textureSample(div_tex, vel_sampler, in.uv - vec2f(ts.x, 0.0)).r;
    let p_t = textureSample(div_tex, vel_sampler, in.uv + vec2f(0.0, ts.y)).r;
    let p_b = textureSample(div_tex, vel_sampler, in.uv - vec2f(0.0, ts.y)).r;

    let grad_p = vec2f(p_r - p_l, p_t - p_b) * 0.5;
    return vec4f(vel - grad_p, 0.0, 1.0);
}

// ============================================================================
// Paint: Trail injection + decay (persistence layer — no advection)
// ============================================================================
//
// Trail texture is RGBA16F: RGB = premultiplied color, A = density.
// No advection — paint stays exactly where deposited.

// HSL → sRGB (same as biofield.wgsl)
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3f {
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let hp = fract(h) * 6.0;
    let x = c * (1.0 - abs(hp % 2.0 - 1.0));
    var rgb: vec3f;
    if      (hp < 1.0) { rgb = vec3f(c, x, 0.0); }
    else if (hp < 2.0) { rgb = vec3f(x, c, 0.0); }
    else if (hp < 3.0) { rgb = vec3f(0.0, c, x); }
    else if (hp < 4.0) { rgb = vec3f(0.0, x, c); }
    else if (hp < 5.0) { rgb = vec3f(x, 0.0, c); }
    else               { rgb = vec3f(c, 0.0, x); }
    let m = l - 0.5 * c;
    return rgb + vec3f(m);
}

@fragment
fn fs_trail_inject_decay(in: VSOut) -> @location(0) vec4f {
    var paint = textureSample(vel_tex_in, vel_sampler, in.uv);

    // RD V channel from previous frame — modulates trail decay (binding 4)
    let rd_v = textureSample(div_tex, vel_sampler, in.uv).g;

    let pixel = in.uv * fu.viewport;
    var decay = 0.9992;  // slow base decay — trails persist ~20 seconds

    let n = min(i32(fu.cell_count), MAX_CELLS);
    for (var i = 0i; i < n; i++) {
        let org_pos = fluid_cells[i].pos;
        let org_r   = fluid_cells[i].radius;
        let energy  = fluid_cells[i].audio_energy;
        let hue     = fluid_cells[i].hue;

        let delta = pixel - org_pos;
        let dist2 = dot(delta, delta);
        let d = sqrt(dist2);

        // Viscosity derived from energy × reactivity
        let visc_r = org_r * 3.0;
        if (d < visc_r) {
            let proximity = 1.0 - d / visc_r;
            let react = f32(fluid_cells[i].rd_fkr & 0x3FFu) / 1000.0;
            let viscosity = energy * react;
            decay = max(decay, mix(0.9992, 0.9999, viscosity * proximity));
        }

        // Steep logistic stamp — vivid at body boundary, sharp falloff
        let t = d / (org_r * STAMP_RADIUS);
        let stamp = 1.0 / (1.0 + exp(STAMP_STEEPNESS * (t - 1.0)));
        let weight = stamp * (0.05 + energy * 0.35);

        // Direct RGB + density deposit (premultiplied color in RGB, density in A)
        let trail_rgb = hsl_to_rgb(hue, 0.85, 0.55);
        paint += vec4f(trail_rgb * weight * fu.dt, weight * fu.dt);
    }

    // RD-modulated decay: where V is high, trail persists; where V is low, trail dissolves
    // This bakes the Turing pattern INTO the trail texture cumulatively
    let rd_decay = mix(0.96, 1.0, smoothstep(0.0, 0.35, rd_v));
    paint *= min(decay, rd_decay);
    return paint;
}

// ============================================================================
// Reaction-Diffusion: Gray-Scott step (ping-ponged on RD texture pair)
// ============================================================================
//
// RD texture: Rg16Float — R = U (substrate), G = V (activator).
// binding(1) = RD texture (self-read), binding(4) = trail texture (V injection).
// Per-organism rd_reactivity, rd_feed, rd_kill spatially blended from CellData.

const DA: f32 = 0.21;          // substrate diffusion (standard Gray-Scott / Pearson)
const DB: f32 = 0.105;         // activator diffusion (DA:DB = 2:1 → Turing instability)
const RD_DT: f32 = 1.0;        // timestep (CFL: 4*DA*dt = 0.84 < 1 ✓)
const INJECT_RATE: f32 = 0.15;  // trail density → activator injection strength
const RD_RES_SCALE: f32 = 2.0;  // RD texture is 1/2 fluid res — base neighbor step

@fragment
fn fs_rd_step(in: VSOut) -> @location(0) vec4f {
    let uv = in.uv;

    // Per-organism: blend reactivity, feed, kill, scale spatially
    let pixel = uv * fu.viewport;
    var local_reactivity = 0.0;
    var local_f = 0.0;
    var local_k = 0.0;
    var local_scale = 0.0;
    var local_energy = 0.0;
    var total_w = 0.0;
    let n = min(i32(fu.cell_count), MAX_CELLS);
    for (var i = 0; i < n; i++) {
        let delta = pixel - fluid_cells[i].pos;
        let d2 = dot(delta, delta);
        let react_r = fluid_cells[i].radius * 3.0;
        let w = exp(-d2 / (2.0 * react_r * react_r));
        // Unpack rd_fkr: feed(bits 31-20), kill(bits 19-10), reactivity(bits 9-0)
        let bits = fluid_cells[i].rd_fkr;
        let org_feed = f32(bits >> 20u) / 10000.0;
        let org_kill = f32((bits >> 10u) & 0x3FFu) / 10000.0;
        let org_react = f32(bits & 0x3FFu) / 1000.0;
        local_reactivity = max(local_reactivity, w * org_react);
        local_energy = max(local_energy, w * fluid_cells[i].audio_energy);
        let rw = w * org_react;
        local_f += rw * org_feed;
        local_k += rw * org_kill;
        local_scale += rw * fluid_cells[i].rd_scale;
        total_w += rw;
    }
    // Normalize blended params (fallback to defaults if no organism nearby)
    if (total_w > 0.001) {
        local_f /= total_w;
        local_k /= total_w;
        local_scale /= total_w;
    } else {
        local_f = 0.035;
        local_k = 0.065;
        local_scale = 6.0;
    }

    // Energy modulates effective scale: loud = DNA baseline, quiet = collapses
    let eff_scale = local_scale * (0.3 + local_energy * 0.7);

    // Neighbor step: RD_RES_SCALE accounts for 1/2 res, eff_scale sets pattern wavelength
    let h = RD_RES_SCALE * eff_scale;        // grid spacing in RD texels
    let ts = fu.texel_size * h;              // UV-space step for sampling

    // Current state
    let c = textureSample(vel_tex_in, vel_sampler, uv).rg;
    let U = c.x;
    let V = c.y;

    // 5-point Laplacian — normalized by h² for correct diffusion at any scale
    let r = textureSample(vel_tex_in, vel_sampler, uv + vec2f(ts.x, 0.0)).rg;
    let l = textureSample(vel_tex_in, vel_sampler, uv - vec2f(ts.x, 0.0)).rg;
    let t = textureSample(vel_tex_in, vel_sampler, uv + vec2f(0.0, ts.y)).rg;
    let b = textureSample(vel_tex_in, vel_sampler, uv - vec2f(0.0, ts.y)).rg;
    let lap = (r + l + t + b - 4.0 * c) / (h * h);

    // Trail feed (binding 4) — new paint deposits inject activator
    let trail = textureSample(div_tex, vel_sampler, uv);
    let trail_density = trail.a;

    // Scale feed by reactivity — low reactivity = slow RD, trails just fade
    let f = local_f * max(local_reactivity, 0.01);
    let k = local_k;
    let uvv = U * V * V;

    let new_U = U + (DA * lap.x - uvv + f * (1.0 - U)) * RD_DT;
    let new_V = V + (DB * lap.y + uvv - (k + f) * V
                 + trail_density * INJECT_RATE * local_reactivity) * RD_DT;

    return vec4f(clamp(new_U, 0.0, 1.0), clamp(new_V, 0.0, 1.0), 0.0, 1.0);
}

