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

const GLOW_RANGE: f32 = 60.0;    // aura falloff distance (pixels)
const MAX_CELLS: i32  = 128;
const PI: f32         = 3.14159265;
const TAU: f32        = 6.28318530;

const RING_EDGE_OUTER: f32 = 15.0;
const RING_FADE_OUTER: f32 = 50.0;
const RING_OPACITY: f32    = 0.92;

const MEMBRANE_HALF_W: f32 = 3.0;    // half-width of membrane band (pixels)
const MEMBRANE_BRIGHT: f32 = 0.75;   // luminance of membrane

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
    pos:          vec2f,
    radius:       f32,
    audio_energy: f32,
    cell_id:      u32,
    hue:          f32,
    vel:          vec2f,
}

@group(0) @binding(0) var<uniform>       u:     BioFieldUniforms;
@group(0) @binding(1) var<storage, read> cells: array<CellData>;

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
// Voronoi distance — weighted by radius, noise-wobbled for organic edges
// Returns normalized distance: 0 at center, 1.0 at body boundary, grows beyond
// ============================================================================

fn voronoi_dist(p: vec2f, idx: i32) -> f32 {
    let delta = p - cells[idx].pos;
    let dist  = length(delta);
    let r     = cells[idx].radius;

    let phase = f32(cells[idx].cell_id) * 0.7;
    let energy = cells[idx].audio_energy;
    let noise_coord = delta / r * 3.0 + vec2f(u.time * 0.4 + phase, u.time * 0.25 - phase);
    let wobble = snoise2(noise_coord) * (0.03 + energy * 0.05);

    return dist / (r * (1.0 + wobble));
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
                        hue: f32, cell_id: u32, energy: f32) -> vec3f {
    let variation = fract(f32(cell_id) * 0.618034);
    let phase = f32(cell_id) * 1.3;

    let delta = pixel - cell_pos;
    let nd = length(delta) / cell_radius;   // 0 at center, ~1 at boundary

    // Large-scale simplex noise distortion of the radial field
    let noise = snoise2(delta / cell_radius * 1.2 + vec2f(phase, phase * 0.7 + u.time * 0.08));
    let d = nd + noise * 0.2;

    // Concentric ring pattern — sqrt compresses outer rings (tighter at boundary)
    // Subtract time → crests move outward. Energy drives speed — loud organisms pulse faster.
    let circles = sqrt(abs(d) * 8.0) * 6.0 - u.time * (0.5 + energy * 1.5);

    // Three rhythmic drivers at offset phases (à la ShaderToy reference)
    let r0 = sin(circles * 1.2 + 2.0);
    let r1 = abs(sin(circles - 1.0) - sin(circles * 0.8));
    let r2 = abs(sin(circles * 0.9));

    // Single HSL buffer: hue from identity, saturation from energy, luminance from rings
    let h = hue + variation * 0.2 * r0;
    let s = 0.7 + 0.2 * r1 * energy;
    let l = 0.25 + 0.35 * (1.0 - r2) + energy * 0.2 * r0;

    return hsl_to_rgb(fract(h), clamp(s, 0.3, 1.0), clamp(l, 0.08, 0.8));
}

// ============================================================================
// Spectral paint mixing (Kubelka-Munk, 38-wavelength)
//
// Ported from spectral.js v3.0.0 by Ronald van Wijnen (MIT License).
// https://github.com/rvanwijnen/spectral.js
//
// Decomposes sRGB → linear → spectral reflectance (38 bands, 380-750nm),
// mixes in K/S (Kubelka-Munk) space, then integrates back to XYZ → sRGB.
// Blue + Yellow = Green, not gray.
// ============================================================================

const SPECTRAL_SIZE: i32 = 38;
const SPECTRAL_EPSILON: f32 = 0.0000000000000001;

// Reflectance-to-XYZ integration weights (CIE 1931 2° observer, 10nm steps)
// Each vec3 = (X, Y, Z) contribution for that wavelength band.
const CIE_XYZ: array<vec3f, 38> = array<vec3f, 38>(
    vec3f(0.0000646919989576, 0.0000018442894440, 0.0003050171476380),
    vec3f(0.0002194098998132, 0.0000062053235865, 0.0010368066663574),
    vec3f(0.0011205743509343, 0.0000310096046799, 0.0053131363323992),
    vec3f(0.0037666134117111, 0.0001047483849269, 0.0179543925899536),
    vec3f(0.0118805536037990, 0.0003536405299538, 0.0570775815345485),
    vec3f(0.0232864424191771, 0.0009514714056444, 0.1136516189362870),
    vec3f(0.0345594181969747, 0.0022822631748318, 0.1733587261835500),
    vec3f(0.0372237901162006, 0.0042073290434730, 0.1962065755586570),
    vec3f(0.0324183761091486, 0.0066887983719014, 0.1860823707062960),
    vec3f(0.0212332056093810, 0.0098883960193565, 0.1399504753832070),
    vec3f(0.0104909907685421, 0.0152494514496311, 0.0891745294268649),
    vec3f(0.0032958375797931, 0.0214183109449723, 0.0478962113517075),
    vec3f(0.0005070351633801, 0.0334229301575068, 0.0281456253957952),
    vec3f(0.0009486742057141, 0.0513100134918512, 0.0161376622950514),
    vec3f(0.0062737180998318, 0.0704020839399490, 0.0077591019215214),
    vec3f(0.0168646241897775, 0.0878387072603517, 0.0042961483736618),
    vec3f(0.0286896490259810, 0.0942490536184085, 0.0020055092122156),
    vec3f(0.0426748124691731, 0.0979566702718931, 0.0008614711098802),
    vec3f(0.0562547481311377, 0.0941521856862608, 0.0003690387177652),
    vec3f(0.0694703972677158, 0.0867810237486753, 0.0001914287288574),
    vec3f(0.0830531516998291, 0.0788565338632013, 0.0001495555858975),
    vec3f(0.0861260963002257, 0.0635267026203555, 0.0000923109285104),
    vec3f(0.0904661376847769, 0.0537414167568200, 0.0000681349182337),
    vec3f(0.0850038650591277, 0.0426460643574120, 0.0000288263655696),
    vec3f(0.0709066691074488, 0.0316173492792708, 0.0000157671820553),
    vec3f(0.0506288916373645, 0.0208852059213910, 0.0000039406041027),
    vec3f(0.0354739618852640, 0.0138601101360152, 0.0000015840125870),
    vec3f(0.0214682102597065, 0.0081026402038399, 0.0000000000000000),
    vec3f(0.0125164567619117, 0.0046301022588030, 0.0000000000000000),
    vec3f(0.0068045816390165, 0.0024913800051319, 0.0000000000000000),
    vec3f(0.0034645657946526, 0.0012593033677378, 0.0000000000000000),
    vec3f(0.0014976097506959, 0.0005416465221680, 0.0000000000000000),
    vec3f(0.0007697004809280, 0.0002779528920067, 0.0000000000000000),
    vec3f(0.0004073680581315, 0.0001471080673854, 0.0000000000000000),
    vec3f(0.0001690104031614, 0.0000610327472927, 0.0000000000000000),
    vec3f(0.0000952245150365, 0.0000343873229523, 0.0000000000000000),
    vec3f(0.0000490309872958, 0.0000177059860053, 0.0000000000000000),
    vec3f(0.0000199961492222, 0.0000072209749130, 0.0000000000000000)
);

// Per-wavelength basis coefficients for the 7 CMY-RGB decomposition channels.
// Each row = [White, Cyan, Magenta, Yellow, Red, Green, Blue] for one wavelength.
// From spectral.js v3.0.0 spectral_linear_to_reflectance().
const BASIS: array<array<f32, 7>, 38> = array<array<f32, 7>, 38>(
    array<f32, 7>(1.0011607271876400, 0.9705850013229620, 0.9906735573199880, 0.0210523371789306, 0.0315605737777207, 0.0095560747554212, 0.9794047525020140),
    array<f32, 7>(1.0011606515972800, 0.9705924981434250, 0.9906715249619790, 0.0210564627517414, 0.0315520718330149, 0.0095581580120851, 0.9794007068431300),
    array<f32, 7>(1.0011603192274700, 0.9706253487298910, 0.9906625823534210, 0.0210746178695038, 0.0315148215513658, 0.0095673245444588, 0.9793829034702610),
    array<f32, 7>(1.0011586727078900, 0.9707868061190170, 0.9906181076447950, 0.0211649058448753, 0.0313318044982702, 0.0096129126297349, 0.9792943649455940),
    array<f32, 7>(1.0011525984455200, 0.9713686732282480, 0.9904514808787100, 0.0215027957272504, 0.0306729857725527, 0.0097837090401843, 0.9789630146085700),
    array<f32, 7>(1.0011325252899800, 0.9731632306212520, 0.9898710814002040, 0.0226738799041561, 0.0286480476989607, 0.0103786227058710, 0.9778144666940430),
    array<f32, 7>(1.0010850066332700, 0.9767402231587650, 0.9882866087596400, 0.0258235649693629, 0.0246450407045709, 0.0120026452378567, 0.9747243211338360),
    array<f32, 7>(1.0009968788945300, 0.9815876054913770, 0.9842906927975040, 0.0334879385639851, 0.0192960753663651, 0.0160977721473922, 0.9671984823439730),
    array<f32, 7>(1.0008652515227400, 0.9862802656529490, 0.9739349056253060, 0.0519069663740307, 0.0142066612220556, 0.0267061902231680, 0.9490796575305750),
    array<f32, 7>(1.0006962900094000, 0.9899491476891340, 0.9418178384601450, 0.1007490148334730, 0.0102942608878609, 0.0595555440185881, 0.9008501289409770),
    array<f32, 7>(1.0005049611488800, 0.9924927015384200, 0.8173903261951560, 0.2391298997068470, 0.0076191460521811, 0.1860398265328260, 0.7631504454622400),
    array<f32, 7>(1.0003080818799200, 0.9941456804052560, 0.4324728050657290, 0.5348043122727480, 0.0058980410835420, 0.5705798201161590, 0.4659221716493190),
    array<f32, 7>(1.0001196660201300, 0.9951839750332120, 0.1384539782588700, 0.7978075786430300, 0.0048233247781713, 0.8614677684002920, 0.2012632804510050),
    array<f32, 7>(0.9999527659684070, 0.9957567501108180, 0.0537347216940033, 0.9114498940673840, 0.0042298748350633, 0.9458790897676580, 0.0877524413419623),
    array<f32, 7>(0.9998218368992970, 0.9959128182867100, 0.0292174996673231, 0.9537979630045070, 0.0040599171299341, 0.9704654864743050, 0.0457176793291679),
    array<f32, 7>(0.9997386095575930, 0.9956061578345280, 0.0213136517508590, 0.9712416154654290, 0.0043533695594676, 0.9784136302844500, 0.0284706050521843),
    array<f32, 7>(0.9997095516396120, 0.9945976009618540, 0.0201349530181136, 0.9793031238075880, 0.0053434425970201, 0.9795890314112240, 0.0205271767569850),
    array<f32, 7>(0.9997319302106270, 0.9922157154923700, 0.0241323096280662, 0.9833801195075750, 0.0076917201010463, 0.9755335369086320, 0.0165302792310211),
    array<f32, 7>(0.9997994363461950, 0.9862364527832490, 0.0372236145223627, 0.9854612465677550, 0.0135969795736536, 0.9622887553978130, 0.0145135107212858),
    array<f32, 7>(0.9999003303166710, 0.9679433372645410, 0.0760506552706601, 0.9864350469766050, 0.0316975442661115, 0.9231215745131200, 0.0136003508637687),
    array<f32, 7>(1.0000204065261100, 0.8912850042449430, 0.2053754719423990, 0.9867382506701410, 0.1078611963552490, 0.7934340189431110, 0.0133604258769571),
    array<f32, 7>(1.0001447879365800, 0.5362024778620530, 0.5412689034604390, 0.9866178824450320, 0.4638126031687040, 0.4592701359024290, 0.0135488943145680),
    array<f32, 7>(1.0002599790341200, 0.1541081190018780, 0.8158416850864860, 0.9862777767586430, 0.8470554052720110, 0.1855741036663030, 0.0139594356366992),
    array<f32, 7>(1.0003557969708900, 0.0574575093228929, 0.9128177041239760, 0.9858605924440560, 0.9431854093939180, 0.0881774959955372, 0.0144434255753570),
    array<f32, 7>(1.0004275378026900, 0.0315349873107007, 0.9463398301669620, 0.9854749276762100, 0.9688621506965580, 0.0543630228766700, 0.0148854440621406),
    array<f32, 7>(1.0004762334488800, 0.0222633920086335, 0.9599276963319910, 0.9851769347655580, 0.9780306674736030, 0.0406288447060719, 0.0152254296999746),
    array<f32, 7>(1.0005072096750800, 0.0182022841492439, 0.9662605952303120, 0.9849715740141810, 0.9820436438543060, 0.0342215204316970, 0.0154592848180209),
    array<f32, 7>(1.0005251915637300, 0.0162990559732640, 0.9693259700584240, 0.9848463034157120, 0.9839236237187070, 0.0311185790956966, 0.0156018026485961),
    array<f32, 7>(1.0005350960689600, 0.0153656239334613, 0.9708545367213990, 0.9847753518111990, 0.9848454841543820, 0.0295708898336134, 0.0156824871281936),
    array<f32, 7>(1.0005402209748200, 0.0149111568733976, 0.9716050665281280, 0.9847380666252650, 0.9852942758145960, 0.0288108739348928, 0.0157248764360615),
    array<f32, 7>(1.0005427281678400, 0.0146954339898235, 0.9719627697573920, 0.9847196483117650, 0.9855072952198250, 0.0284486271324597, 0.0157458108784121),
    array<f32, 7>(1.0005438956908700, 0.0145964146717719, 0.9721272722745090, 0.9847110233919390, 0.9856050715398370, 0.0282820301724731, 0.0157556123350225),
    array<f32, 7>(1.0005444821215100, 0.0145470156699655, 0.9722094177458120, 0.9847066833006760, 0.9856538499335780, 0.0281988376490237, 0.0157605443964911),
    array<f32, 7>(1.0005447695999200, 0.0145228771899495, 0.9722495776784240, 0.9847045543930910, 0.9856776850338830, 0.0281581655342037, 0.0157629637515278),
    array<f32, 7>(1.0005448988776200, 0.0145120341118965, 0.9722676219987420, 0.9847035963093700, 0.9856883918061220, 0.0281398910216386, 0.0157640525629106),
    array<f32, 7>(1.0005449625468900, 0.0145066940939832, 0.9722765094621500, 0.9847031240775520, 0.9856936646900310, 0.0281308901665811, 0.0157645892329510),
    array<f32, 7>(1.0005449892705800, 0.0145044507314479, 0.9722802433068740, 0.9847029256150900, 0.9856958798482050, 0.0281271086805816, 0.0157648147772649),
    array<f32, 7>(1.0005449969930000, 0.0145038009464639, 0.9722813248265600, 0.9847028681227950, 0.9856965214637620, 0.0281260133612096, 0.0157648801149616)
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

fn eval_biofield(pixel: vec2f) -> BioFieldHit {
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
    let vor_blend = f1 / (f1 + f2);        // 0 at center, 0.5 at Voronoi edge
    let eff_radius = mix(cells[id0].radius, cells[id1].radius, vor_blend);
    let body_d = (f1 - 1.0) * eff_radius;              // organism body SDF (pixels)
    let edge_d = -(f2 - f1) * 0.5 * eff_radius;        // Voronoi edge SDF (pixels)
    let vor_d  = max(edge_d, body_d);                   // combined signed distance

    // Early exit
    if (vor_d > GLOW_RANGE * 2.0) { return hit; }
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

    // ---- Paraboloid depth + specular ----
    let height = paraboloid_height(vor_d, eff_radius);

    // Depth shading: richer color deeper in paint
    let depth_factor = 0.55 + 0.45 * height;
    var col = base_color * depth_factor;

    // Normal-based specular
    if (vor_d < 5.0) {
        let normal = compute_normal(pixel, n, eff_radius);
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

    // ---- Identity ring (body interior, peaks at centroid) ----
    let ring_outer = smoothstep(RING_FADE_OUTER, RING_EDGE_OUTER, body_d);
    let center_boost = 1.0 - smoothstep(-50.0, 0.0, body_d);   // 1 deep inside, 0 at body boundary
    let ring_mask = ring_outer * (0.45 + 0.55 * center_boost);  // 0.45 at edge, 1.0 at centroid

    if (ring_mask > 0.001) {
        let ring_a = identity_band_color(pixel, cells[id0].pos, cells[id0].radius,
                        cells[id0].hue, cells[id0].cell_id, cells[id0].audio_energy);
        let ring_b = identity_band_color(pixel, cells[id1].pos, cells[id1].radius,
                        cells[id1].hue, cells[id1].cell_id, cells[id1].audio_energy);

        // Spectral paint mixing at territory boundaries (blue+yellow=green, not mud)
        var ring_color: vec3f;
        if (n == 1) {
            ring_color = ring_a;
        } else {
            ring_color = spectral_mix(ring_a, ring_b, vor_blend);
        }

        col = spectral_mix(col, ring_color, ring_mask * RING_OPACITY);
    }

    // ---- Membrane band at Voronoi edge (territory boundary) ----
    let mem_dist = abs(edge_d);
    let membrane = 1.0 - smoothstep(MEMBRANE_HALF_W * 0.5, MEMBRANE_HALF_W, mem_dist);
    let mem_color = hsl_to_rgb(cells[id0].hue, 0.95, MEMBRANE_BRIGHT + energy * 0.2);
    col = mix(col, mem_color, membrane * 0.8);

    // ---- Body / aura boundary ----
    let inside = smoothstep(10.0, -5.0, body_d);

    // ---- Fluid aura zone (between body surface and Voronoi edge) ----
    let in_cell = smoothstep(0.0, -5.0, edge_d);       // 1 inside territory, fade at edge
    let aura_t = smoothstep(GLOW_RANGE, 0.0, body_d) * (1.0 - inside) * in_cell;

    var aura_color = vec3f(0.0);
    var aura_alpha = 0.0;

    if (aura_t > 0.001) {
        // Flow field: curl noise + velocity bias + collision compression
        let base_flow = curl_noise(pixel, u.time);

        // Velocity wake — stretch aura in direction of travel
        let vel0 = cells[id0].vel;
        let vel_bias = vel0 * VEL_STRETCH;

        // Dome dampening — flow silent at body center, full at aura edge
        let dome_mask = smoothstep(-5.0, GLOW_RANGE * 0.7, body_d);

        // Collision compression — perpendicular flow near Voronoi boundary
        let overlap_strength = smoothstep(0.1, 0.3, vor_blend) * smoothstep(0.9, 0.7, vor_blend);
        let to_b = normalize(cells[id1].pos - cells[id0].pos + vec2f(0.001, 0.001));
        let perp_flow = vec2f(-to_b.y, to_b.x) * overlap_strength * 1.5;

        // Combined flow displacement
        let flow = (base_flow + vel_bias + perp_flow) * dome_mask * 15.0;

        // Advect ring pattern through flow field
        let advected_pixel = pixel - flow * aura_t;

        let aura_ring_a = identity_band_color(advected_pixel, cells[id0].pos, cells[id0].radius,
                            cells[id0].hue, cells[id0].cell_id, cells[id0].audio_energy);
        let aura_ring_b = identity_band_color(advected_pixel, cells[id1].pos, cells[id1].radius,
                            cells[id1].hue, cells[id1].cell_id, cells[id1].audio_energy);

        // Spectral paint mixing in aura (not opacity blending)
        if (n == 1) {
            aura_color = aura_ring_a;
        } else {
            aura_color = spectral_mix(aura_ring_a, aura_ring_b, vor_blend);
        }

        // Ring dissolution — fade ring contrast with distance, keep color richness
        let dissolution = aura_t * aura_t;
        let ring_fade = 1.0 - dissolution * 0.5;
        aura_color = mix(aura_color, base_color, 1.0 - ring_fade);

        aura_alpha = aura_t * 0.5;
    }

    // ---- Final compositing ----
    let alpha = max(inside, aura_alpha);
    let body_part = col * inside;
    let aura_part = aura_color * aura_alpha * (1.0 - inside);
    hit.color = body_part + aura_part;
    hit.alpha = alpha;
    return hit;
}

// ============================================================================
// Fragment — biofield pass (renders to intermediate RGBA16Float texture)
// Output: premultiplied alpha on transparent background
// ============================================================================

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * u.viewport;
    let hit   = eval_biofield(pixel);

    if (!hit.visible) {
        return vec4f(0.0);
    }

    return vec4f(hit.color * hit.alpha, hit.alpha);
}

// ============================================================================
// Capture fragment shader — same as fs but for Rgba8Unorm capture target
// ============================================================================

@fragment
fn fs_capture(in: VSOut) -> @location(0) vec4f {
    let pixel = in.uv * u.viewport;
    let hit   = eval_biofield(pixel);

    if (!hit.visible) {
        return vec4f(0.0);
    }

    return vec4f(hit.color * hit.alpha, hit.alpha);
}
