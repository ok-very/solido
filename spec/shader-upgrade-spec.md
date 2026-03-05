<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# that sounds a lot more lightweight. please draft a spec

Here is the full spec. I've referenced the canonical resistor color palette (digit → color mapping: 0=Black, 1=Brown, 2=Red, 3=Orange, 4=Yellow, 5=Green, 6=Blue, 7=Violet, 8=Gray, 9=White)  in the identity encoding section.[^1]

***

# BioField Shader System — Technical Specification v0.1

## 1. Overview

A 2.5D metaball shader for 20+ animated organisms with per-cell identity encoding via resistor band patterns and audio-reactive behavior. Designed to run at 60fps on mid-range WebGL2 hardware by constraining per-fragment work to O(n) with a fixed smin budget of 1 operation per fragment.

**Primary output:** A `mainImage`-compatible GLSL fragment shader + a JS/TS host class.

***

## 2. Constraints \& Targets

| Parameter | Target |
| :-- | :-- |
| Max organisms | 64 (uniform array limit) |
| smin calls per fragment | 1 (nearest-pair only) |
| totalField calls for gradient | 8 (2 cells × 4 epsilon samples) |
| Audio analysis | CPU-side only, result as uniforms |
| Antialiasing | Optional 2×2 MSAA toggle via `#define` |
| GLSL version | `#version 300 es` (WebGL2) |


***

## 3. Data Structures

### 3.1 Cell UBO Layout (CPU → GPU)

```glsl
// Packed as vec4 arrays for UBO alignment
// cellData[i].xy  = position (normalized screen space, -aspect..+aspect, -1..1)
// cellData[i].z   = radius
// cellData[i].w   = audioEnergy (0..1, mapped from FFT band)
// cellMeta[i].x   = int id (0..63, cast to int in shader)
// cellMeta[i].y   = dominantHue (0..1, HSL hue)
// cellMeta[i].zw  = reserved
uniform vec4 cellData[^64];
uniform vec4 cellMeta[^64];
uniform int  cellCount;
uniform float iTime;
uniform vec2  iResolution;
uniform sampler2D iChannel0; // blue noise for dithering/AO
```


### 3.2 Cell Identity

Each organism has a **2-digit resistor ID** (0–63, but encoded as two base-10 digits 0–9 for the visual band system). This maps directly to the 10-color resistor palette:[^1]

```
0=Black  1=Brown  2=Red    3=Orange  4=Yellow
5=Green  6=Blue   7=Violet 8=Gray    9=White
```

An ID of `27` displays a Violet band + a Violet band. An ID of `53` displays a Green + Orange band. The third band encodes **audio energy** as a brightness multiplier — this is where the "live" signal becomes visible.

***

## 4. Shader Pipeline

### 4.1 Stage 1 — Nearest-K Scan (O(n))

```glsl
// Output: indices and SDFs of 2 nearest cells
int id0 = -1, id1 = -1;
float d0 = 1e9, d1 = 1e9;

for (int i = 0; i < cellCount; i++) {
    vec2  cpos = cellData[i].xy;
    float cr   = cellData[i].z;
    float d    = length(uv - cpos) - cr;    // SDF: negative inside

    if (d < d0) {
        d1 = d0; id1 = id0;
        d0 = d;  id0 = i;
    } else if (d < d1) {
        d1 = d;  id1 = i;
    }
}
```

No branching on the output path; the loop is uniform-count and cache-coherent across fragments.

### 4.2 Stage 2 — smin Blend (1 call)

```glsl
float k = 0.05;                     // blend radius, expose as uniform
vec2 merged = smin(d0, d1, k);      // .x = merged dist, .y = blend weight m
// m=0.0 means fully cell id0, m=1.0 means fully cell id1
```

Use the **cubic polynomial smin** from IQ — it costs ~6 ALU ops and is not the bottleneck.[^2]

### 4.3 Stage 3 — 2.5D Normal From Gradient (8 ops)

```glsl
float eps = 0.003;

// Only sample the 2 nearest cells, not all n
float fieldAt(vec2 p, int a, int b) {
    float da = length(p - cellData[a].xy) - cellData[a].z;
    float db = length(p - cellData[b].xy) - cellData[b].z;
    return smin(da, db, k).x;
}

float nx = fieldAt(uv+vec2(eps,0.),id0,id1) - fieldAt(uv-vec2(eps,0.),id0,id1);
float ny = fieldAt(uv+vec2(0.,eps),id0,id1) - fieldAt(uv-vec2(0.,eps),id0,id1);
vec3 nor = normalize(vec3(nx, ny, 0.25));  // z-bias prevents flat normals at cell edges
```


### 4.4 Stage 4 — Identity Band Rendering

Each cell surface is projected into a local band coordinate:

```glsl
vec3 resistorColor(int digit) {
    // Canonical 10-color resistor palette
    if (digit == 0) return vec3(0.02, 0.02, 0.02); // Black
    if (digit == 1) return vec3(0.55, 0.27, 0.07); // Brown
    if (digit == 2) return vec3(1.00, 0.00, 0.00); // Red
    if (digit == 3) return vec3(1.00, 0.50, 0.00); // Orange
    if (digit == 4) return vec3(1.00, 1.00, 0.00); // Yellow
    if (digit == 5) return vec3(0.00, 0.80, 0.00); // Green
    if (digit == 6) return vec3(0.00, 0.30, 1.00); // Blue
    if (digit == 7) return vec3(0.55, 0.00, 0.83); // Violet
    if (digit == 8) return vec3(0.55, 0.55, 0.55); // Gray
    return             vec3(1.00, 1.00, 1.00);      // White (9)
}

vec3 decodeCellColor(int cellIdx, vec2 uv, float audioEnergy) {
    int id     = int(cellMeta[cellIdx].x);
    int digit0 = id / 10;     // tens band
    int digit1 = id - digit0*10; // units band

    // Project UV into polar bands relative to cell center
    vec2  delta = uv - cellData[cellIdx].xy;
    float angle = atan(delta.y, delta.x);     // -PI..PI
    float t     = fract(angle / (2.0*PI) + 0.5); // 0..1 around cell

    // 3 bands: tens digit | units digit | audio band
    float bandWidth = 1.0 / 3.0;
    vec3  col;
    if      (t < bandWidth)       col = resistorColor(digit0);
    else if (t < 2.0*bandWidth)   col = resistorColor(digit1);
    else                          col = vec3(audioEnergy);   // live signal band

    return col;
}
```

Blend between the two nearest cells using `m`:

```glsl
vec3 colA = decodeCellColor(id0, uv, cellData[id0].w);
vec3 colB = decodeCellColor(id1, uv, cellData[id1].w);
vec3 col  = mix(colA, colB, merged.y); // merged.y == m from smin
```


### 4.5 Stage 5 — Lighting

```glsl
float diffuse = clamp(0.5 + 0.5 * nor.y, 0.0, 1.0);
float fresnel = clamp(1.0 + dot(nor, vec3(0.,0.,-1.)), 0.0, 1.0);

vec3 lit = col * (0.6*diffuse + 0.4*fresnel);
lit      += 0.15 * fresnel * vec3(1.0, 0.9, 0.7); // warm rim

// Inside threshold: only light pixels where merged field < 0
float inside = smoothstep(0.01, -0.01, merged.x);
col = mix(vec3(0.0), lit, inside);
```


***

## 5. CPU / JS Host

### 5.1 Audio Analysis

```ts
class BioFieldAudio {
    analyser: AnalyserNode;
    fftData: Uint8Array;
    // Map FFT bands → per-cell energy (round-robin or by frequency range)
    getCellEnergy(cellIndex: number, cellCount: number): number {
        const binStart = Math.floor((cellIndex / cellCount) * this.fftData.length / 2);
        const binEnd   = binStart + 4;
        let sum = 0;
        for (let b = binStart; b < binEnd; b++) sum += this.fftData[b];
        return (sum / (4 * 255));
    }
}
```

**Rule:** No FFT work in GLSL. The host writes `cellData[i].w` once per frame before the draw call.

### 5.2 Organism Update Loop

```ts
function updateUniforms(gl, program, time, organisms, audio) {
    const data = new Float32Array(organisms.length * 4);
    const meta = new Float32Array(organisms.length * 4);
    for (let i = 0; i < organisms.length; i++) {
        organisms[i].update(time);            // position/radius animation
        data[i*4+0] = organisms[i].x;
        data[i*4+1] = organisms[i].y;
        data[i*4+2] = organisms[i].radius;
        data[i*4+3] = audio.getCellEnergy(i, organisms.length);
        meta[i*4+0] = organisms[i].id;        // int, 0..63
        meta[i*4+1] = organisms[i].hue;       // reserved for dominant color overlay
    }
    gl.uniform4fv(gl.getUniformLocation(program, 'cellData'), data);
    gl.uniform4fv(gl.getUniformLocation(program, 'cellMeta'), meta);
    gl.uniform1i (gl.getUniformLocation(program, 'cellCount'), organisms.length);
}
```


***

## 6. Extension Points

These are deliberately **out of scope for v0.1** but have clean integration paths:

- **Spatial grid acceleration** — subdivide screen into 4×4 grid cells, each storing 4–8 nearest organism indices; reduces the O(n) loop to O(k) with k≈4[^3]
- **Dominant color phenotype** — `cellMeta[i].y` (hue) currently reserved; use HSL→RGB to tint the `dominantHue` over the resistor bands in blend zones to show genetic expression
- **DNA grouping** — add a `groupId` byte packed into `cellMeta[i].x` high bits; organisms sharing a group get a wider smin `k`, visually clustering them
- **6-band encoding** — extending to 6 polar bands encodes a full byte per organism (0–255), allowing richer identity at the cost of tighter stripe geometry at small radii

***

## 7. File Structure

```
/src
  /shader
    biofield.frag.glsl     ← all stages above
    biofield.vert.glsl     ← passthrough quad
  /host
    BioFieldRenderer.ts    ← WebGL2 setup, uniform writes
    BioFieldAudio.ts       ← AudioContext, FFT mapping
    Organism.ts            ← position/animation, id, hue
  /debug
    idInspector.frag.glsl  ← renders cell id as text overlay
```


***

## 8. Open Questions Before Implementation

1. **Are organism IDs static or runtime-assigned?** Affects whether `cellMeta` needs to be a dynamic buffer or can be baked.
2. **Audio source:** microphone, `<audio>` element, or generated tone? Determines `AudioContext` routing.
3. **Band orientation:** polar (around cell center, as specced) vs. screen-aligned stripes — polar is more identity-stable as cells move.
4. **Threshold for "inside" rendering** — should the background always be black, or does the field fade into a scene/texture?
<span style="display:none">[^10][^11][^12][^13][^14][^15][^4][^5][^6][^7][^8][^9]</span>

<div align="center">⁂</div>

[^1]: https://nightshade.net/design-tools/resistor-color-codes/

[^2]: https://stackoverflow.com/questions/76123710/improving-performance-p5-js-metaballs

[^3]: https://www.reddit.com/r/computergraphics/comments/gj0nax/optimize_metaball_rendering_on_shader/

[^4]: https://tympanus.net/codrops/2025/06/09/how-to-create-interactive-droplet-like-metaballs-with-three-js-and-glsl/

[^5]: https://community.khronos.org/t/fragment-shader-optimization/52361

[^6]: https://developer.nvidia.cn/gpugems/gpugems3/part-i-geometry/chapter-7-point-based-visualization-metaballs-gpu

[^7]: https://resistorcolorcodecalc.com

[^8]: https://www.youtube.com/watch?v=WDDu1DBIWtE

[^9]: https://valenciacollege.edu/academics/departments/engineering/documents/understanding-the-resistor-color-code.pdf

[^10]: https://www.youtube.com/watch?v=YyUHQCyHE0w

[^11]: https://stackoverflow.com/questions/38767472/what-causes-those-performance-breaking-points-with-glsl-shaders

[^12]: https://learn.sparkfun.com/tutorials/resistors/decoding-resistor-markings

[^13]: https://eepower.com/resistor-guide/resistor-standards-and-codes/resistor-color-code/

[^14]: https://gist.github.com/87481d74750fd710b11af458889407cc

[^15]: https://resistorcolorcodes.com

