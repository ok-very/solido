# Well Lens Rewrite — Convex Optics on Substrate

**Status**: Spec (rewrite of gravity_well.rs well model)
**Depends on**: video-substrate Phase 4, substrate-encoding.md
**Blocks**: nav-reward-rewrite

---

## What Changes

**Before**: Gravity wells are passive harmonic attractors. Each well has a root_pitch_class. Organisms near a well hear the scale transposed to that well's root. LJ force profile pulls organisms inward. Well energy drains under occupancy, regenerates when empty. Navigation reward tracks arrival/departure/slingshot events.

**After**: Wells are convex lenses that focus substrate energy. No pitch class identity — the lens concentrates whatever substrate color is in its region. Organisms follow food, not harmony. Well energy = lens strength (depleted lens = weak focus, regenerated = strong). The existing energy state machine (Healthy → Wavering → Dormant) stays but drives lens power instead of harmonic influence.

---

## Lens Model

### UV Warp in Composite Shader

Wells bend the substrate texture sampling, concentrating nearby pixels toward the well center:

```wgsl
fn well_lens(uv: vec2f, well_pos: vec2f, well_radius: f32, lens_power: f32) -> vec2f {
    let offset = uv - well_pos;
    let dist = length(offset);
    let t = clamp(dist / well_radius, 0.0, 1.0);
    // Quadratic focus: strongest at center, no effect at radius edge
    let focus = mix(lens_power, 1.0, t * t);
    return well_pos + offset * focus;
}
```

- `lens_power` [0.2, 1.0]: lower = stronger concentration. 0.2 = 5× energy density at center.
- At well edge (t=1): `focus = 1.0` → no distortion
- At well center (t=0): `focus = lens_power` → UV compressed, sampling from wider area
- Well energy modulates lens_power: `lens_power = 0.2 + 0.8 × (1.0 - well.energy)`. Full energy = strongest lens. Depleted = weak.

### Visual Effect

Looking at a well from above:
- Substrate pixels near well center appear magnified and brighter (concentrated)
- Ring of compression at the well edge (slight stretching)
- When well energy depletes (organisms drained it), the lens weakens → pixels spread back out
- Dormant well = lens_power ≈ 1.0 → flat, no concentration

### Shader Integration

The composite shader iterates over wells (passed as uniform data):

```wgsl
// After computing video_uv from aspect ratio:
var substrate_uv = video_uv;
for (var i = 0u; i < well_count; i++) {
    substrate_uv = well_lens(substrate_uv, wells[i].pos, wells[i].radius, wells[i].power);
}
// Then sample substrate at the warped UV
```

Wells compose — overlapping wells create compound lensing. An organism between two wells sees a distorted substrate field shaped by both.

---

## Well Data on GPU

### New Uniform/Storage

```rust
#[repr(C)]
pub struct WellGpuData {
    pub pos: [f32; 2],     // Viewport-space position (normalized UV)
    pub radius: f32,        // Lens radius in UV space
    pub power: f32,         // Lens strength [0.2, 1.0]
}
```

Up to 6 wells. Passed as a small storage buffer or packed into the CompositeUniforms (6 × 16 bytes = 96 bytes, fits in a uniform).

### CompositeUniforms Extension

```rust
pub struct CompositeUniforms {
    pub viewport:   [f32; 2],
    pub time:       f32,
    pub ca_amount:  f32,
    // NEW: well lens data (packed, up to 6 wells)
    pub well_count: u32,
    pub _pad:       [f32; 3],
    pub wells:      [WellGpuData; 6],  // 6 × 16 = 96 bytes
}
```

---

## Energy State Machine (Preserved, Reinterpreted)

The three-state machine stays exactly as is:

| State | Energy | Behavior |
|-------|--------|----------|
| **Healthy** | 0.5–1.0 | Strong lens, fast regen when unoccupied |
| **Wavering** | 0.1–0.5 | Weakening lens, stochastic regen |
| **Dormant** | <0.1 | Flat (no lensing), 5s cooldown before reactivation |

### Drain Model

**Before**: `drain = BASE_DRAIN × total_influence / sqrt(occupant_count)`

**After**: Same math, but "occupancy" is measured by substrate depletion in the well region rather than organism count:

```rust
// Depletion pressure = how much substrate energy organisms have consumed in well radius
let depletion_pressure = substrate_grid.mean_depletion_in_region(well.pos, well.radius);
well.energy -= BASE_DRAIN * depletion_pressure;
```

Heavy feeding in a well region → energy drains → lens weakens → concentration drops → organisms must seek richer areas. This IS the navigational pressure — no separate force model needed.

### Regen Model

When no organisms are feeding in the well region:
```rust
let local_energy = substrate_grid.mean_energy_in_region(well.pos, well.radius);
if local_energy > 0.5 {  // Substrate has replenished (video still playing)
    well.energy += REGEN_RATE;
}
```

Wells only regenerate when the substrate beneath them has recovered. No video = no regeneration = permanently dormant wells.

---

## What Gets Removed

- `well.root_pitch_class` — wells no longer have harmonic identity
- `transpose_to_key()` — key changes affect substrate encoding, not wells
- `effective_weights()` — pitch comes from substrate consumption, not well blending
- LJ force profile for organism attraction — organisms follow substrate energy gradient, wells just concentrate it
- `WellProximity` navigation events driven by pitch consonance

## What Stays

- Well positions in the viewport (6 deterministic positions)
- Energy state machine (Healthy/Wavering/Dormant)
- Visual rendering of wells (energy tick indicators in viewport)
- `GravityField` struct (reinterpreted as lens array)
- Well energy drain/regen rates from DNA

---

## Organism Attraction (Emergent, Not Forced)

**Before**: LJ force pulls organisms toward wells based on consonance.

**After**: No explicit force. Organisms naturally migrate toward wells because:
1. Well lens concentrates substrate → more energy per area
2. Organisms follow energy gradient (hunger-driven movement, existing system)
3. Higher energy = more nutrition = positive valence = stay
4. When well depletes, energy drops, organism gets hungry, wanders away

The existing hunger → arousal → wander cycle handles everything. Wells are just geography — energy oases, not harmonic attractors.

---

## Critical Files

| File | Change |
|------|--------|
| `src/renderer/composite.wgsl` | Add well_lens() UV warp loop, WellGpuData |
| `src/renderer/biofield_renderer.rs` | Extend CompositeUniforms with well data, upload per frame |
| `src/tuning/gravity_well.rs` | Remove pitch-related code, keep energy state machine, add lens_power |
| `src/app.rs` | Feed well positions/energy to renderer, remove effective_weights dispatch |
| `src/substrate/energy_grid.rs` | Add `mean_energy_in_region()`, `mean_depletion_in_region()` |

---

## Verification

1. Well visible as bright concentrated region in substrate — like a magnifying glass on the video
2. Organism moves toward well → richer feeding → stays
3. Multiple organisms at well → substrate depletes faster → well energy drops → lens weakens → organisms scatter
4. Dormant well = flat, no concentration → indistinguishable from surrounding substrate
5. Well regenerates when organisms leave and video replenishes the area
6. Two overlapping wells create compound lensing — extra bright intersection
