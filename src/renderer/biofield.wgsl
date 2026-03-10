// Solido v0.6 BioField Voronoi spine shader — unified render pipeline
//
// Weighted Voronoi tessellation gives each organism a territory.
// Three zones per pixel: body interior (dome), aura zone (fluid paint),
// Voronoi edge (membrane). Spectral paint mixing (Kubelka-Munk) at edges.
// Paraboloid height → analytical normals → specular volume.
//
// Output: premultiplied RGBA on transparent background (composited in composite.wgsl).
//
// Bindings:
//   group(0) binding(0): BioFieldUniforms (uniform)
//   group(0) binding(1): CellData[] (storage)

// ---------------------------------------------------------------------------
// Tuning constants
// ---------------------------------------------------------------------------

const MAX_CELLS: i32  = 128;
const PI: f32         = 3.14159265;
const TAU: f32        = 6.28318530;

// Dissolved body edge (amplitude now per-organism via CellData.shape_amplitude)
const DISSOLVE_WIDTH: f32    = 6.0;    // smoothstep transition width at noisy edge

// Early-exit distance beyond dissolved body edge
const EXIT_MARGIN: f32       = 10.0;

// Ridged shimmer
const SHIMMER_SCALE: f32     = 0.035;  // ridged noise frequency for shimmer
const SHIMMER_INTENSITY: f32 = 0.45;   // peak shimmer brightness

// Fresnel rim glow
const FRESNEL_POWER: f32     = 3.0;    // rim glow exponent
const FRESNEL_STRENGTH: f32  = 0.4;    // rim glow max intensity

// Subsurface translucency
const SSS_STRENGTH: f32      = 0.25;   // subsurface translucency at thin edges

const MEMBRANE_HALF_W: f32 = 4.0;    // half-width of membrane band (pixels)
const MEMBRANE_BRIGHT: f32 = 0.85;   // luminance of membrane

// Aura fluid dynamics
const CURL_FINE_SCALE: f32   = 0.02;   // fine octave frequency
const CURL_COARSE_SCALE: f32 = 0.005;  // coarse octave frequency
const CURL_SPEED: f32        = 0.3;    // time animation speed
const VEL_STRETCH: f32       = 0.15;   // velocity→flow stretching factor

// ---------------------------------------------------------------------------
// Data layout (must match biofield_renderer.rs)
// ---------------------------------------------------------------------------

struct BioFieldUniforms {
    viewport:   vec2f,
    time:       f32,
    cell_count: f32,
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

@group(0) @binding(0) var<uniform>       u:     BioFieldUniforms;
@group(0) @binding(1) var<storage, read> cells: array<CellData>;
@group(0) @binding(2) var velocity_tex:  texture_2d<f32>;
@group(0) @binding(3) var vel_sampler:   sampler;
@group(0) @binding(4) var trail_tex:     texture_2d<f32>;  // persistence layer (packed 4-band)
@group(0) @binding(5) var rd_tex:       texture_2d<f32>;  // reaction-diffusion (Rg16F: R=U, G=V)

// Trail texture: RGBA16F with premultiplied RGB color in .rgb and density in .a

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
// Polynomial smooth minimum (Inigo Quilez) — C1 continuous
// Matches CPU smooth_min() in sdf.rs
// ============================================================================

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / max(k, 0.001), 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

const MAX_SUBNODES: i32 = 6;

// ============================================================================
// Voronoi distance — weighted by radius, crawling sub-node metaball blending
// Returns normalized distance: 0 at center, 1.0 at body boundary, grows beyond
// ============================================================================

fn voronoi_dist(p: vec2f, idx: i32) -> f32 {
    let delta = p - cells[idx].pos;
    let r     = cells[idx].radius;

    // Velocity direction and speed (used only for crawl bias direction)
    let speed = length(cells[idx].vel);
    let dir = select(vec2f(1.0, 0.0), cells[idx].vel / speed, speed > 1.0);

    // Sub-node count
    let n_sub = i32(cells[idx].harmonic_count);

    // --- Single-node fallback (harmonic_count < 2) ---
    if (n_sub < 2) {
        let eff_dist = length(delta);
        let phase = f32(cells[idx].cell_id) * 0.7;
        let energy = cells[idx].audio_energy;
        let noise_coord = delta / r * 3.0
            + vec2f(u.time * 0.4 + phase, u.time * 0.25 - phase);
        let wobble = snoise2(noise_coord) * (0.03 + energy * 0.05);
        return eff_dist / (r * (1.0 + wobble));
    }

    // --- Crawling sub-nodes with Chladni standing waves ---
    // All geometry in world space — no heading-relative frame, no rotation artifacts
    let crawl_phase = cells[idx].ring_phase;
    let amp         = cells[idx].harmonic_amp;
    let energy      = cells[idx].audio_energy;
    let fn_sub      = f32(n_sub);
    let id_f        = f32(cells[idx].cell_id);

    // Per-species Chladni modal numbers (from DNA, packed in elongation slot)
    let packed = cells[idx].elongation;
    let m_mode = floor(packed);                                    // 2–5
    let n_mode = round(fract(packed) * 10.0);                     // 1–3
    let omega1 = m_mode * 0.35 + 0.5 + fract(id_f * 0.317) * 0.3;  // species base + instance jitter
    let omega2 = n_mode * 0.25 + 0.4 + fract(id_f * 0.223) * 0.2;

    // Sub-node geometry
    let core_r  = r * 0.6;
    let node_r  = r * (0.18 + amp * 0.5);
    let blend_k = r * 0.25;

    // Heading angle for directional crawl bias
    let heading_angle = atan2(dir.y, dir.x);
    let speed_factor = min(speed / 50.0, 1.0);

    // Core body SDF (world space — pure circle, no heading-dependent stretch)
    var result = length(delta) - core_r;

    // Blend sub-node blobs — world-space angles, Chladni-modulated extension
    for (var i = 0; i < MAX_SUBNODES; i++) {
        if (i >= n_sub) { break; }

        let fi = f32(i);
        // Fixed world-space angular position (golden ratio offset per organism)
        let theta_i = fi / fn_sub * TAU + id_f * 0.618034;

        // Chladni standing wave: product of two cosine modes
        let chladni = cos(m_mode * theta_i + omega1 * crawl_phase)
                    * cos(n_mode * theta_i + omega2 * crawl_phase);

        // Directional crawl bias: forward-facing nodes reach further
        let crawl_bias = cos(theta_i - heading_angle) * speed_factor * 0.6;

        // Combined extension (audio energy scales Chladni amplitude)
        let dance_intensity = 0.15 + energy * 0.85;
        let extension = amp * chladni * dance_intensity + crawl_bias * amp;

        // Radial reach
        let reach = r * (0.25 + extension * 0.45);

        // World-space node position + distance (no frame transform)
        let world_node = vec2f(cos(theta_i), sin(theta_i)) * reach;
        let node_dist = length(delta - world_node) - node_r;
        result = smin(result, node_dist, blend_k);
    }

    // Noise wobble on the blended surface (world-space coords — stable on body)
    let phase = f32(cells[idx].cell_id) * 0.7;
    let noise_coord = delta / r * 3.0
        + vec2f(u.time * 0.4 + phase, u.time * 0.25 - phase);
    let wobble = snoise2(noise_coord) * (0.03 + energy * 0.05);

    // Normalize: SDF=0 (surface) → 1.0, SDF<0 (inside) → <1.0, SDF>0 (outside) → >1.0
    // Downstream code: body_d = (f1 - 1.0) * eff_radius
    return (result / r) + 1.0 - wobble * 0.5;
}

// ============================================================================
// 2D Simplex noise (Ashima Arts / Stefan Gustavson — MIT License)
// ============================================================================

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
    let C = vec4f(0.211324865405187,   // (3.0 - sqrt(3.0)) / 6.0
                  0.366025403784439,   // 0.5 * (sqrt(3.0) - 1.0)
                  -0.577350269189626,  // -1.0 + 2.0 * C.x
                  0.024390243902439);  // 1.0 / 41.0

    // First corner (skew to simplex grid)
    let s = dot(v, vec2f(C.y));
    var i = floor(v + vec2f(s));
    let us = dot(i, vec2f(C.x));
    let x0 = v - i + vec2f(us);

    // Other corners
    let i1 = select(vec2f(0.0, 1.0), vec2f(1.0, 0.0), x0.x > x0.y);
    let x1 = x0 + vec2f(C.x) - i1;
    let x2 = x0 + vec2f(C.z);

    // Permutations
    i = mod289_v2(i);
    let p = permute(permute(
                vec3f(i.y) + vec3f(0.0, i1.y, 1.0))
              + vec3f(i.x) + vec3f(0.0, i1.x, 1.0));

    var m = max(vec3f(0.5) - vec3f(dot(x0, x0), dot(x1, x1), dot(x2, x2)), vec3f(0.0));
    m = m * m;
    m = m * m;

    // Gradients
    let x_ = 2.0 * fract(p * C.w) - vec3f(1.0);
    let h  = abs(x_) - vec3f(0.5);
    let ox = floor(x_ + vec3f(0.5));
    let a0 = x_ - ox;

    // Normalize gradients
    m *= vec3f(1.79284291400159) - 0.85373472095314 * (a0 * a0 + h * h);

    // Compute final noise value
    let gx = vec3f(a0.x * x0.x + h.x * x0.y,
                   a0.y * x1.x + h.y * x1.y,
                   a0.z * x2.x + h.z * x2.y);

    return 130.0 * dot(m, gx);
}

// ============================================================================
// Curl noise — divergence-free 2D flow field from simplex gradient
// Two octaves for organic turbulence. Returns flow direction vector.
// ============================================================================

fn curl_noise(p: vec2f, t: f32) -> vec2f {
    let eps = 1.0;

    // Fine octave
    let pf = p * CURL_FINE_SCALE + vec2f(t * CURL_SPEED, t * CURL_SPEED * 0.7);
    let nf_dx = snoise2(pf + vec2f(eps, 0.0)) - snoise2(pf - vec2f(eps, 0.0));
    let nf_dy = snoise2(pf + vec2f(0.0, eps)) - snoise2(pf - vec2f(0.0, eps));
    let curl_fine = vec2f(nf_dy, -nf_dx);

    // Coarse octave
    let pc = p * CURL_COARSE_SCALE + vec2f(t * CURL_SPEED * 0.3, -t * CURL_SPEED * 0.2);
    let nc_dx = snoise2(pc + vec2f(eps, 0.0)) - snoise2(pc - vec2f(eps, 0.0));
    let nc_dy = snoise2(pc + vec2f(0.0, eps)) - snoise2(pc - vec2f(0.0, eps));
    let curl_coarse = vec2f(nc_dy, -nc_dx);

    return normalize(curl_fine * 0.6 + curl_coarse * 0.4 + vec2f(0.001, 0.001));
}

// ============================================================================
// Ridged multifractal noise — sharp ridges for shimmer contour lines
// ============================================================================

fn ridged_noise(p: vec2f) -> f32 {
    var sum = 0.0;
    var amp = 0.6;
    var freq = 1.0;
    var weight = 1.0;
    for (var i = 0; i < 3; i++) {
        let n = 1.0 - abs(snoise2(p * freq));
        let n2 = n * n * weight;
        sum += n2 * amp;
        weight = clamp(n2, 0.0, 1.0);
        amp *= 0.4;
        freq *= 2.2;
    }
    return sum;
}

// ============================================================================
// fBm — 3-octave fractional Brownian motion for dissolve edge displacement
// ============================================================================

fn fbm2(p: vec2f) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i = 0; i < 3; i++) {
        val += amp * snoise2(p * freq);
        amp *= 0.5;
        freq *= 2.0;
    }
    return val;
}

// ============================================================================
// HSL → sRGB
// ============================================================================

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

// ============================================================================
// sRGB ↔ linear
// ============================================================================

fn srgb_to_linear_ch(x: f32) -> f32 {
    return select(pow((x + 0.055) / 1.055, 2.4), x / 12.92, x < 0.04045);
}

fn linear_to_srgb_ch(x: f32) -> f32 {
    return select(1.055 * pow(x, 1.0 / 2.4) - 0.055, x * 12.92, x < 0.0031308);
}

fn srgb_to_linear(c: vec3f) -> vec3f {
    return vec3f(srgb_to_linear_ch(c.x), srgb_to_linear_ch(c.y), srgb_to_linear_ch(c.z));
}

fn linear_to_srgb(c: vec3f) -> vec3f {
    return clamp(vec3f(linear_to_srgb_ch(c.x), linear_to_srgb_ch(c.y), linear_to_srgb_ch(c.z)), vec3f(0.0), vec3f(1.0));
}

// ============================================================================
// Identity ring — three polar bands per cell
// ============================================================================

fn identity_band_color(pixel: vec2f, cell_pos: vec2f, cell_radius: f32,
                        hue: f32, cell_id: u32, energy: f32, ring_phase: f32) -> vec3f {
    let variation = fract(f32(cell_id) * 0.618034);
    let phase = f32(cell_id) * 1.3;

    let delta = pixel - cell_pos;
    let nd = length(delta) / cell_radius;   // 0 at center, ~1 at boundary

    // Large-scale simplex noise distortion of the radial field
    let noise = snoise2(delta / cell_radius * 1.2 + vec2f(phase, phase * 0.7 + u.time * 0.08));
    let d = nd + noise * 0.2;

    // Concentric ring pattern — sqrt compresses outer rings (tighter at boundary)
    // CPU-accumulated phase avoids jitter from energy*time multiplication
    let circles = sqrt(abs(d) * 6.0) * 5.0 - ring_phase;

    // Two gentle rhythmic drivers (reduced from 3 for calmer motion)
    let r0 = sin(circles * 1.0 + 2.0);
    let r1 = abs(sin(circles - 1.0) - sin(circles * 0.7));

    // Single HSL buffer: hue from identity, saturation from energy, luminance from rings
    let h = hue + variation * 0.2 * r0;
    let s = 0.7 + 0.2 * r1 * energy;
    let l = 0.25 + 0.35 * (1.0 - r1 * 0.5) + energy * 0.15 * r0;

    return hsl_to_rgb(fract(h), clamp(s, 0.3, 1.0), clamp(l, 0.08, 0.8));
}

// ============================================================================
// Spectral paint mixing (Kubelka-Munk, 16-wavelength)
//
// Ported from spectral.js v3.0.0 by Ronald van Wijnen (MIT License).
// https://github.com/rvanwijnen/spectral.js
//
// Reduced from 38 to 16 bands (20nm spacing, 400-700nm) for GPU efficiency.
// Decomposes sRGB → linear → spectral reflectance, mixes in K/S space,
// then integrates back to XYZ → sRGB. Blue + Yellow = Green, not gray.
// ============================================================================

const SPECTRAL_SIZE: i32 = 16;
const SPECTRAL_EPSILON: f32 = 0.0000000000000001;

// Reflectance-to-XYZ integration weights (CIE 1931 2° observer, 20nm steps)
// 16 bands at 400-700nm (indices 2,4,6,...,32 from the original 38-band table).
const CIE_XYZ: array<vec3f, 16> = array<vec3f, 16>(
    vec3f(0.0011205743509343, 0.0000310096046799, 0.0053131363323992),
    vec3f(0.0118805536037990, 0.0003536405299538, 0.0570775815345485),
    vec3f(0.0345594181969747, 0.0022822631748318, 0.1733587261835500),
    vec3f(0.0324183761091486, 0.0066887983719014, 0.1860823707062960),
    vec3f(0.0104909907685421, 0.0152494514496311, 0.0891745294268649),
    vec3f(0.0005070351633801, 0.0334229301575068, 0.0281456253957952),
    vec3f(0.0062737180998318, 0.0704020839399490, 0.0077591019215214),
    vec3f(0.0286896490259810, 0.0942490536184085, 0.0020055092122156),
    vec3f(0.0562547481311377, 0.0941521856862608, 0.0003690387177652),
    vec3f(0.0830531516998291, 0.0788565338632013, 0.0001495555858975),
    vec3f(0.0904661376847769, 0.0537414167568200, 0.0000681349182337),
    vec3f(0.0709066691074488, 0.0316173492792708, 0.0000157671820553),
    vec3f(0.0354739618852640, 0.0138601101360152, 0.0000015840125870),
    vec3f(0.0125164567619117, 0.0046301022588030, 0.0000000000000000),
    vec3f(0.0034645657946526, 0.0012593033677378, 0.0000000000000000),
    vec3f(0.0007697004809280, 0.0002779528920067, 0.0000000000000000)
);

// Per-wavelength basis coefficients for the 7 CMY-RGB decomposition channels.
// Each row = [White, Cyan, Magenta, Yellow, Red, Green, Blue] for one wavelength.
// 16 bands at 20nm spacing (indices 2,4,6,...,32 from original 38-band table).
const BASIS: array<array<f32, 7>, 16> = array<array<f32, 7>, 16>(
    array<f32, 7>(1.0011603192274700, 0.9706253487298910, 0.9906625823534210, 0.0210746178695038, 0.0315148215513658, 0.0095673245444588, 0.9793829034702610),
    array<f32, 7>(1.0011525984455200, 0.9713686732282480, 0.9904514808787100, 0.0215027957272504, 0.0306729857725527, 0.0097837090401843, 0.9789630146085700),
    array<f32, 7>(1.0010850066332700, 0.9767402231587650, 0.9882866087596400, 0.0258235649693629, 0.0246450407045709, 0.0120026452378567, 0.9747243211338360),
    array<f32, 7>(1.0008652515227400, 0.9862802656529490, 0.9739349056253060, 0.0519069663740307, 0.0142066612220556, 0.0267061902231680, 0.9490796575305750),
    array<f32, 7>(1.0005049611488800, 0.9924927015384200, 0.8173903261951560, 0.2391298997068470, 0.0076191460521811, 0.1860398265328260, 0.7631504454622400),
    array<f32, 7>(1.0001196660201300, 0.9951839750332120, 0.1384539782588700, 0.7978075786430300, 0.0048233247781713, 0.8614677684002920, 0.2012632804510050),
    array<f32, 7>(0.9998218368992970, 0.9959128182867100, 0.0292174996673231, 0.9537979630045070, 0.0040599171299341, 0.9704654864743050, 0.0457176793291679),
    array<f32, 7>(0.9997095516396120, 0.9945976009618540, 0.0201349530181136, 0.9793031238075880, 0.0053434425970201, 0.9795890314112240, 0.0205271767569850),
    array<f32, 7>(0.9997994363461950, 0.9862364527832490, 0.0372236145223627, 0.9854612465677550, 0.0135969795736536, 0.9622887553978130, 0.0145135107212858),
    array<f32, 7>(1.0000204065261100, 0.8912850042449430, 0.2053754719423990, 0.9867382506701410, 0.1078611963552490, 0.7934340189431110, 0.0133604258769571),
    array<f32, 7>(1.0002599790341200, 0.1541081190018780, 0.8158416850864860, 0.9862777767586430, 0.8470554052720110, 0.1855741036663030, 0.0139594356366992),
    array<f32, 7>(1.0004275378026900, 0.0315349873107007, 0.9463398301669620, 0.9854749276762100, 0.9688621506965580, 0.0543630228766700, 0.0148854440621406),
    array<f32, 7>(1.0005072096750800, 0.0182022841492439, 0.9662605952303120, 0.9849715740141810, 0.9820436438543060, 0.0342215204316970, 0.0154592848180209),
    array<f32, 7>(1.0005350960689600, 0.0153656239334613, 0.9708545367213990, 0.9847753518111990, 0.9848454841543820, 0.0295708898336134, 0.0156824871281936),
    array<f32, 7>(1.0005427281678400, 0.0146954339898235, 0.9719627697573920, 0.9847196483117650, 0.9855072952198250, 0.0284486271324597, 0.0157458108784121),
    array<f32, 7>(1.0005444821215100, 0.0145470156699655, 0.9722094177458120, 0.9847066833006760, 0.9856538499335780, 0.0281988376490237, 0.0157605443964911)
);

// Decompose linear RGB → 7-component spectral weights per the CMY-RGB octant method.
// Returns reflectance for a single wavelength band at index `i`.
fn reflectance_at(lrgb: vec3f, i: i32) -> f32 {
    let w = min(lrgb.r, min(lrgb.g, lrgb.b));
    let lrgb2 = lrgb - w;

    let c = min(lrgb2.g, lrgb2.b);
    let m = min(lrgb2.r, lrgb2.b);
    let y = min(lrgb2.r, lrgb2.g);
    let r = min(max(0.0, lrgb2.r - lrgb2.b), max(0.0, lrgb2.r - lrgb2.g));
    let g = min(max(0.0, lrgb2.g - lrgb2.b), max(0.0, lrgb2.g - lrgb2.r));
    let b = min(max(0.0, lrgb2.b - lrgb2.g), max(0.0, lrgb2.b - lrgb2.r));

    let row = BASIS[i];
    return max(SPECTRAL_EPSILON, w * row[0] + c * row[1] + m * row[2] + y * row[3] + r * row[4] + g * row[5] + b * row[6]);
}

// Compute luminance (CIE Y) for a linear RGB color via spectral decomposition.
fn spectral_luminance(lrgb: vec3f) -> f32 {
    var lum = 0.0;
    for (var i = 0i; i < SPECTRAL_SIZE; i++) {
        lum += reflectance_at(lrgb, i) * CIE_XYZ[i].y;
    }
    return lum;
}

// Kubelka-Munk: reflectance → absorption/scattering ratio
fn KS(R: f32) -> f32 {
    return (1.0 - R) * (1.0 - R) / (2.0 * R);
}

// Kubelka-Munk: K/S ratio → reflectance
fn KM(ks: f32) -> f32 {
    return 1.0 + ks - sqrt(ks * ks + 2.0 * ks);
}

// XYZ → linear sRGB
fn xyz_to_linear_srgb(xyz: vec3f) -> vec3f {
    return vec3f(
        dot(vec3f( 3.2409699419045200, -1.537383177570090, -0.4986107602930030), xyz),
        dot(vec3f(-0.9692436362808790,  1.875967501507720,  0.0415550574071756), xyz),
        dot(vec3f( 0.0556300796969936, -0.203976958888976,  1.0569715142428700), xyz)
    );
}

// Spectral paint mixing: mix two sRGB colors subtractively.
// t=0 → color_a, t=1 → color_b. Blue + Yellow = Green.
fn spectral_mix(color_a: vec3f, color_b: vec3f, t: f32) -> vec3f {
    let lrgb1 = srgb_to_linear(color_a);
    let lrgb2 = srgb_to_linear(color_b);

    let factor1 = 1.0 - t;
    let factor2 = t;

    let lum1 = spectral_luminance(lrgb1);
    let lum2 = spectral_luminance(lrgb2);

    let conc1 = factor1 * factor1 * lum1;
    let conc2 = factor2 * factor2 * lum2;
    let total_conc = conc1 + conc2;

    var xyz = vec3f(0.0);
    for (var i = 0i; i < SPECTRAL_SIZE; i++) {
        let r1 = reflectance_at(lrgb1, i);
        let r2 = reflectance_at(lrgb2, i);

        let ks_mix = (KS(r1) * conc1 + KS(r2) * conc2) / total_conc;
        let R = KM(ks_mix);
        xyz += R * CIE_XYZ[i];
    }

    return linear_to_srgb(xyz_to_linear_srgb(xyz));
}

// ============================================================================
// Paraboloid height + normals
// ============================================================================

const NORMAL_Z_SCALE: f32 = 0.35;
const SPECULAR_POWER: f32 = 40.0;
const SPECULAR_INTENSITY: f32 = 0.6;

fn paraboloid_height(d: f32, eff_radius: f32) -> f32 {
    let t = clamp(-d / eff_radius, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);   // smoothstep hermite
}

// Evaluate Voronoi field at a point (for normal computation).
// Returns combined signed distance: body surface OR Voronoi edge, whichever closer.
fn eval_voronoi_field(p: vec2f, n: i32) -> f32 {
    if (n == 0) { return 1e5; }
    var f1 = voronoi_dist(p, 0);
    var f2 = 1e6;
    var id0 = 0i;
    for (var i = 1i; i < n; i++) {
        let d = voronoi_dist(p, i);
        if (d < f1) { f2 = f1; f1 = d; id0 = i; }
        else if (d < f2) { f2 = d; }
    }
    let r = cells[id0].radius;
    return max(-(f2 - f1) * 0.5 * r, (f1 - 1.0) * r);
}

fn compute_normal(pixel: vec2f, n: i32, eff_radius: f32) -> vec3f {
    let eps = 3.0;
    let h_c = paraboloid_height(eval_voronoi_field(pixel, n), eff_radius);
    let h_r = paraboloid_height(eval_voronoi_field(pixel + vec2f(eps, 0.0), n), eff_radius);
    let h_u = paraboloid_height(eval_voronoi_field(pixel + vec2f(0.0, eps), n), eff_radius);
    let dhdx = (h_r - h_c) / eps;
    let dhdy = (h_u - h_c) / eps;
    return normalize(vec3f(-dhdx, -dhdy, NORMAL_Z_SCALE));
}

// ============================================================================
// Main biofield evaluation
// ============================================================================

struct BioFieldHit {
    color:   vec3f,
    alpha:   f32,
    visible: bool,
}

fn eval_biofield(pixel: vec2f, uv: vec2f) -> BioFieldHit {
    let n = min(i32(u.cell_count), MAX_CELLS);

    var hit: BioFieldHit;
    hit.visible = false;

    if (n == 0) { return hit; }

    // ---- Voronoi scan O(n) ----
    var f1 = voronoi_dist(pixel, 0);
    var f2 = 1e6;
    var id0 = 0i;
    var id1 = 0i;

    for (var i = 1i; i < n; i++) {
        let d = voronoi_dist(pixel, i);
        if (d < f1) {
            f2 = f1; id1 = id0;
            f1 = d;  id0 = i;
        } else if (d < f2) {
            f2 = d; id1 = i;
        }
    }

    // ---- Two-level distance (cell within a cell) ----
    // Softmax-weighted blend — wider, smoother transition than linear f1/(f1+f2)
    const SOFT_T: f32 = 0.15;
    let w0 = exp(-f1 / SOFT_T);
    let w1 = exp(-f2 / SOFT_T);
    let vor_blend = w1 / (w0 + w1);
    let eff_radius = mix(cells[id0].radius, cells[id1].radius, vor_blend);
    let body_d = (f1 - 1.0) * eff_radius;              // organism body SDF (pixels)
    let edge_d = -(f2 - f1) * 0.5 * eff_radius;        // Voronoi edge SDF (pixels)
    let vor_d  = max(edge_d, body_d);                   // combined signed distance

    // ---- Trail paint (sampled viewport-wide, BEFORE early exit) ----
    let trail = textureSample(trail_tex, vel_sampler, uv);
    let trail_alpha = saturate(trail.a * 3.0);

    var trail_color = vec3f(0.0);
    if (trail_alpha > 0.001) {
        trail_color = trail.rgb / max(trail.a, 0.001);
    }

    // Early exit — beyond body range, show trail paint directly
    // (RD dissolution is baked into the trail texture via decay modulation)
    if (vor_d > cells[id0].shape_amplitude + EXIT_MARGIN + 15.0) {
        let far_alpha = trail_alpha * 0.8;
        if (far_alpha > 0.001) {
            hit.visible = true;
            hit.color = trail_color * far_alpha;
            hit.alpha = far_alpha;
        }
        return hit;
    }
    hit.visible = true;

    // ---- Organism colors (HSL → sRGB) ----
    let col_a = hsl_to_rgb(cells[id0].hue, 0.82, 0.52);
    let col_b = hsl_to_rgb(cells[id1].hue, 0.82, 0.52);

    // Spectral mix — intensity increases toward Voronoi edge where neighbors meet
    var base_color: vec3f;
    if (n == 1) {
        base_color = col_a;
    } else {
        base_color = spectral_mix(col_a, col_b, vor_blend);
    }

    // ---- Paraboloid specular (height still needed for screen-space normals) ----
    let height = paraboloid_height(vor_d, eff_radius);
    var col = base_color;

    // Normal-based specular (screen-space derivatives — free, no Voronoi re-scan)
    if (vor_d < 3.0) {
        let dhdx = dpdx(height);
        let dhdy = dpdy(height);
        let normal = normalize(vec3f(-dhdx, -dhdy, NORMAL_Z_SCALE));
        // Overhead light + slight offset for visual interest
        let light_dir = normalize(vec3f(0.15, -0.1, 1.0));
        let view_dir  = vec3f(0.0, 0.0, 1.0);
        let half_vec  = normalize(light_dir + view_dir);
        let spec = pow(max(dot(normal, half_vec), 0.0), SPECULAR_POWER) * SPECULAR_INTENSITY;

        // Audio energy makes organisms look "wetter" (brighter specular)
        let energy = mix(cells[id0].audio_energy, cells[id1].audio_energy, vor_blend);
        let wet_boost = 1.0 + energy * 0.4;

        col += vec3f(spec * wet_boost);
    }

    // Audio shimmer: slow luminance pulse
    let energy = mix(cells[id0].audio_energy, cells[id1].audio_energy, vor_blend);
    let phase = f32(cells[id0].cell_id) * 0.7;
    let pulse = 1.0 + energy * 0.12 * (0.5 + 0.5 * sin(u.time * 1.5 + phase));
    col *= pulse;

    // ---- Noise-dissolved body edge (computed first — ring_mask depends on `inside`) ----
    let edge_phase = f32(cells[id0].cell_id) * 1.7;
    let edge_p = pixel * cells[id0].shape_frequency + vec2f(cells[id0].ring_phase * 0.25 + edge_phase,
                                                           cells[id0].ring_phase * 0.175 - edge_phase);
    let edge_noise = fbm2(edge_p);
    let dissolve_amp = cells[id0].shape_amplitude + energy * 15.0;
    let noisy_body_d = body_d - edge_noise * dissolve_amp;
    let inside = smoothstep(DISSOLVE_WIDTH, -DISSOLVE_WIDTH, noisy_body_d);

    // ---- Identity ring (fills entire body interior up to dissolved edge) ----
    let ring_mask = inside;

    if (ring_mask > 0.001) {
        let ring_a = identity_band_color(pixel, cells[id0].pos, cells[id0].radius,
                        cells[id0].hue, cells[id0].cell_id, cells[id0].audio_energy, cells[id0].ring_phase);
        let ring_b = identity_band_color(pixel, cells[id1].pos, cells[id1].radius,
                        cells[id1].hue, cells[id1].cell_id, cells[id1].audio_energy, cells[id1].ring_phase);

        var ring_color: vec3f;
        if (n == 1) {
            ring_color = ring_a;
        } else {
            ring_color = mix(ring_a, ring_b, vor_blend);
        }

        col = ring_color;
    }

    // ---- Membrane band at Voronoi edge (territory boundary) ----
    let mem_dist = abs(edge_d);
    let membrane = 1.0 - smoothstep(1.0, MEMBRANE_HALF_W, mem_dist);
    let mem_hue = mix(cells[id0].hue, cells[id1].hue, 0.5);
    let mem_color = hsl_to_rgb(mem_hue, 0.6, MEMBRANE_BRIGHT + energy * 0.15);
    col = mix(col, mem_color, membrane * 0.85);

    // ---- Fresnel rim glow ----
    // Use paraboloid height as proxy for surface angle (thin = rim)
    let rim_t = smoothstep(-20.0, 5.0, noisy_body_d);  // 1 at edge, 0 deep inside
    let fresnel = pow(rim_t, FRESNEL_POWER) * FRESNEL_STRENGTH * inside;
    let fresnel_color = hsl_to_rgb(cells[id0].hue, 0.6, 0.75);

    // ---- Ridged shimmer at dissolve edge ----
    let shimmer_p = pixel * SHIMMER_SCALE + vec2f(u.time * 0.4, -u.time * 0.25);
    let shimmer_raw = ridged_noise(shimmer_p);
    // Gaussian peak at the dissolve boundary (strongest right at the edge)
    let edge_proximity = exp(-noisy_body_d * noisy_body_d / 300.0);
    let shimmer = shimmer_raw * edge_proximity * (0.15 + energy * SHIMMER_INTENSITY);
    let shimmer_color = hsl_to_rgb(cells[id0].hue, 0.7, 0.72);

    // ---- Subsurface translucency at thin edges ----
    let thickness = smoothstep(-cells[id0].shape_amplitude, cells[id0].shape_amplitude * 0.3, noisy_body_d);
    let sss = thickness * (1.0 - thickness) * 4.0;  // peaks at mid-thickness
    let sss_glow = sss * SSS_STRENGTH * (0.5 + energy * 0.5);
    let sss_color = hsl_to_rgb(cells[id0].hue, 0.4, 0.65);

    // ---- Final compositing ----
    // Body with rim + shimmer + SSS
    let body_lit = col + fresnel_color * fresnel + shimmer_color * shimmer * inside
                 + sss_color * sss_glow * inside;

    // Paint (trail, visible outside body — RD dissolution baked into trail decay)
    let outer_paint_alpha = trail_alpha * 0.8 * (1.0 - inside);
    let outer_paint_part = trail_color * outer_paint_alpha;

    // Compose: body over paint trail
    let body_part = body_lit * inside;

    hit.color = body_part + outer_paint_part;
    hit.alpha = max(inside, outer_paint_alpha);
    return hit;
}

// ============================================================================
// Fragment — biofield pass (renders to intermediate RGBA16Float texture)
// Output: premultiplied alpha on transparent background
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * u.viewport;
    let hit   = eval_biofield(pixel, in.uv);

    if (!hit.visible) {
        return vec4f(0.0);
    }

    return vec4f(hit.color, hit.alpha);
}

// ============================================================================
// Capture fragment shader — same as fs but for Rgba8Unorm capture target
// ============================================================================

@fragment
fn fs_capture(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * u.viewport;
    let hit   = eval_biofield(pixel, in.uv);

    if (!hit.visible) {
        return vec4f(0.0);
    }

    return vec4f(hit.color, hit.alpha);
}
