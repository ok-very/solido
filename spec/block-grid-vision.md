# Block Grid Vision — Organism Sight via Substrate Sampling

**Status**: Spec
**Depends on**: energy_grid.rs (done), substrate-encoding.md
**Blocks**: S33-rewrite (bridge), nav-reward-rewrite

---

## Goal

Replace global CV feature broadcast with local substrate sampling. Each organism sees its neighborhood of energy grid cells — block triangulation is the vision system. DNA controls what each species perceives. The 160px CV analysis becomes redundant; the grid IS the sensory field.

---

## Architecture

### Current (Global Broadcast)
```
VideoAnalysisModule computes 4 global scalars (brightness, warmth, motion, edge)
  → SetVideoFeatures command broadcast to ALL organisms at 30Hz
  → video_cv_cell smooths and routes to modulation targets
```

Problems: all organisms see the same thing. No spatial awareness. Global averages are meaningless when organisms are at different positions on the substrate.

### Proposed (Local Grid Sampling)
```
SubstrateGrid holds per-cell energy (RGB + derived pitch/rhythm)
  → Each organism samples grid in a radius around its position
  → Local features computed per-organism (unique to their location)
  → video_cv_cell receives per-organism features via SetVideoFeatures
  → Modulation wires route to DSP as before
```

---

## Per-Organism Feature Extraction

### Sampling Window

Each organism samples a rectangular neighborhood of grid cells:

```rust
pub struct LocalSight {
    pub brightness: f32,       // Mean energy of sampled cells
    pub warmth: f32,           // Red-blue ratio in neighborhood
    pub motion: f32,           // Energy delta from previous frame's sample
    pub edge: f32,             // Energy variance across neighborhood (high variance = edge)
    pub dominant_pc: u8,       // Most energetic pitch class in neighborhood
    pub pitch_diversity: f32,  // How many distinct pitch classes are available [0, 1]
    pub rhythm_energy: f32,    // Mean brightness (rhythm driver)
}
```

### Sampling Radius (DNA-Driven)

```rust
// New DNA fields:
pub sight_radius: f32,       // Grid cells visible [2, 8]. Default: 4 (64px at 16px blocks)
pub sight_sensitivity: f32,  // How strongly vision affects behavior [0, 1]
```

Species defaults:
| Species | sight_radius | sight_sensitivity | Personality |
|---------|-------------|-------------------|-------------|
| DRON | 6 | 0.3 | Far-sighted, slow responder |
| HOSO | 4 | 0.7 | Normal range, attentive |
| ACID | 3 | 0.9 | Near-sighted, hyper-reactive |
| SPGL | 8 | 0.2 | Panoramic, detached |
| ISAO | 4 | 0.6 | Normal, moderate |
| TBLK | 3 | 0.8 | Close-range, rhythmically alert |
| KKIT | 2 | 0.9 | Very near-sighted, instant reaction |
| RECH | 5 | 0.5 | Medium, balanced |

### Motion Detection (Frame-to-Frame Delta)

Each organism caches its previous local brightness. Motion = `|current_brightness - prev_brightness|`. This is TRUE local motion detection — not a global frame diff, but the actual change at the organism's position. If the organism moved, it sees different substrate. If the video changed, same effect. Both contribute to perceived motion.

### Edge Detection (Local Variance)

Sample the neighborhood and compute variance:
```
edge = stddev(cell_energies_in_neighborhood) / mean(cell_energies_in_neighborhood)
```

High variance = the organism is near a boundary between bright and dark substrate regions. This is structurally identical to Sobel edge detection but computed on the block grid — more meaningful for organism behavior than raw pixel edges.

---

## video_cv_cell Integration

The existing video_cv_cell stays but its inputs change:

**Before**: `SetVideoFeatures { brightness, warmth, motion, edge }` — global, same for all

**After**: `SetVideoFeatures { brightness, warmth, motion, edge }` — per-organism, computed from their local grid sample

The command is still broadcast-style, but app.rs computes different features for each organism:

```rust
for org in organisms {
    let sight = substrate_grid.local_sight(org.position, org.sight_radius);
    org_mod.send_command(DspCommand::SetVideoFeatures {
        brightness: sight.brightness,
        warmth: sight.warmth,
        motion: sight.motion,
        edge: sight.edge,
    });
}
```

This replaces the single global broadcast with per-organism dispatch. video_cv_cell code is unchanged — it just receives different data per organism.

---

## Substrate Grid Extensions

### New Methods on SubstrateGrid

```rust
/// Sample local features at a viewport position with given sight radius (in grid cells).
pub fn local_sight(&self, x: f32, y: f32, radius: u32) -> LocalSight { ... }

/// Sample the RGB energy at a position (for nutrient feeding).
pub fn sample_rgb(&self, x: f32, y: f32) -> [f32; 3] { ... }

/// Get the dominant pitch class in a neighborhood.
pub fn dominant_pitch_class(&self, x: f32, y: f32, radius: u32) -> u8 { ... }
```

### Cached Previous State

For motion detection, the grid needs a delta buffer:
```rust
pub struct SubstrateGrid {
    // ... existing fields ...
    prev_energy: Vec<f32>,  // Previous frame's per-cell energy (for motion delta)
}
```

Updated each frame after replenish+deplete: `prev_energy[i] = cells[i].energy`.

---

## Removing Global CV

Once local sight is implemented:
1. `VideoAnalysisModule` no longer needs to broadcast global features
2. The global `SetVideoFeatures` broadcast in app.rs is replaced by per-organism dispatch
3. `compute_brightness()`, `compute_warmth()`, etc. in video_analysis.rs become dead code for organism feeding (still useful for the video panel display)
4. The 160px analysis resolution becomes purely about video panel monitoring, not organism vision

---

## Critical Files

| File | Change |
|------|--------|
| `src/substrate/energy_grid.rs` | Add `local_sight()`, `sample_rgb()`, `dominant_pitch_class()`, prev_energy delta buffer |
| `src/app.rs` | Replace global video broadcast with per-organism local sight dispatch |
| `src/organism/dna.rs` | Add `sight_radius`, `sight_sensitivity` DNA fields |
| `assets/dna/*.json` | Add sight params to all 8 organisms |
| `src/modules/video_analysis.rs` | Keep for video panel monitoring, no longer drives organism vision |

---

## Verification

1. Two organisms at different positions see different features → move one to bright area, other to dark → different audio response
2. ACID (near-sighted, hyper-reactive) responds to small fast changes. DRON (far-sighted, slow) responds to large gradual shifts.
3. Organism at substrate edge → high edge feature → edge_density drives filter/chaos
4. Video panel still shows global averages (monitoring), organisms act on local data
5. Motion feature responds to organism movement through heterogeneous substrate, not just video changes
