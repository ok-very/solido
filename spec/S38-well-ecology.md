# S38 — Well Ecology: Chladni Plate Physics

**Status**: Complete (Mar 2026)
**Depends on**: S36 (physics hardening — 120Hz fixed timestep)
**Blocks**: S39 (navigation reward — trajectory events reference LJ well model)

---

## Goal

Replace the current quadratic-falloff well attractor with a vibrating Chladni plate model. Wells become resonant structures with orbital trenches, nodal patterns, energy budgets, and beat-pulsed dynamics. Organisms settle into standing-wave patterns determined by their harmonic compatibility with the well, orbit at a natural trench radius instead of falling to the center, and synchronize direction via boid alignment. The Orbit and Glob organism interaction modes are subsumed by this model and removed from DNA.

---

## Context

### What exists today

Gravity wells (`GravityWell` in `src/tuning/gravity_well.rs`) are spatial harmonic attractors. `GravityField` holds 1-6 wells with circle-of-fifths root pitch classes. The physics loop in `app.rs` applies quadratic-falloff forces `strength * (1 - (dist/radius)^2)` weighted by `consonance_weight(interval) * scale_affinity * 12.0`. This pulls organisms toward the well center, where they stagnate — equilibrium at `r=0` is stable but boring.

Organism interactions use DNA-specified `interaction_rules` with modes: Repel, Bounce, Attach, Glob, Orbit, IntegratePropose. Currently all 7 organisms have Orbit rules (tangential bell-curve force between organisms), and HOSO/ACID/ISAO have Glob rules (mutual attraction + centroid pull + viscous drag). These create ad-hoc orbiting and clumping behavior that this spec replaces with physics-grounded alternatives.

The Hebbian satisfaction system (`port_satisfaction()` on `OrganismModule`) already returns per-port quality scores for pitch deviation and beat timing. `spectral_centroid` is available from `DspAnalysis` on each organism. `OrganismState` has `chladni_m` and `chladni_n` fields (currently visual-only SDF parameters). The 120Hz fixed timestep (S36) provides the stable integration foundation.

### What this spec changes

1. **Well force model**: Quadratic falloff replaced by softened Lennard-Jones (LJ) — creates orbital trench at equilibrium radius instead of center-seeking.
2. **Chladni spatial patterning**: Each well's pitch class determines a vibration mode. Nodal lines create preferred angular positions; consonant organisms lock to stable nodes.
3. **Boid alignment**: Organisms in the same well average velocities. No Chladni awareness — pure local rule that composes independently.
4. **Beat-pulsed plate**: Audio energy modulates the well's repulsion radius, causing rhythmic "bounce" within nodes.
5. **Energy drain/regen**: Wells are finite resources. Occupied wells drain; empty wells recharge. Creates temporal cycling and dispersion pressure.
6. **Spectral niche pressure**: Organisms with similar spectral centroids compete for the same well positions.
7. **Satisfaction integration**: Well ecology feeds into Hebbian learning via `port_satisfaction()`.
8. **Orbit/Glob removal**: These interaction modes are removed from DNA. LJ orbits + Chladni nodes + boid alignment replace them entirely.

### What is preserved from previous S38

- `WellInfluence`, `WellProximity` struct concepts (adapted for LJ model)
- `WellEnergy`, `RegenState` (energy drain/regen model, constants unchanged)
- Spectral niche pressure (frequency-ratio log2 model)
- Satisfaction integration path (`port_satisfaction()` bonus)
- `consonance_weight()` extraction as free function

---

## Architecture

### 1. Softened Lennard-Jones Well Force

The current quadratic falloff creates a single equilibrium at `r=0` (well center). Lennard-Jones creates a **trench** — an equilibrium ring at a specific radius where attraction and repulsion balance. Organisms naturally orbit this ring.

**Force model:**

```
F_radial(r) = F_pull(r) - F_push(r)

F_pull = G * M_well / (r^2 + eps^2)           — softened inverse-square attraction
F_push = K_repulse / (r^2 + eps_inner^2)       — softened inner repulsion

where:
  r         = distance from organism to well center
  G         = gravitational constant (tunable)
  M_well    = well mass (= well.energy * well.strength) — drains dynamically
  eps       = LJ_SOFTENING — prevents infinity at r=0
  K_repulse = LJ_REPULSE_K — inner repulsion strength
  eps_inner = LJ_INNER_SOFTENING — inner repulsion softening
```

**Trench radius** is where `F_pull = F_push`:

```
G * M_well / (r_eq^2 + eps^2) = K_repulse / (r_eq^2 + eps_inner^2)

At equilibrium: r_eq = sqrt((K_repulse * (r^2 + eps^2)) / (G * M_well) - eps_inner^2)
```

In practice, tune `G`, `K_repulse`, and the softening values so that `r_eq` falls at roughly `well.radius * LJ_TRENCH_FRACTION` (default 0.6 of the well's visual radius). Organisms beyond `well.radius` feel negligible force (both terms fall off as ~1/r^2).

**Consonance modulation**: The gravitational constant `G` is multiplied by `consonance_weight(interval) * scale_affinity`. Consonant organisms feel stronger pull and orbit deeper in the trench. Dissonant organisms (consonance=0.2) barely feel the well.

**Radial force clamping**: The net radial force is clamped to `[-MAX_WELL_FORCE, MAX_WELL_FORCE]` to prevent catapulting. With softened denominators, division-by-zero is impossible, but clamping provides defense-in-depth.

**Implementation (replaces `apply_gravity_well_forces` in `app.rs`):**

```rust
fn apply_well_forces(&mut self) {
    if self.effects_bypass.gravity_bypassed { return; }

    for i in 0..self.well_dispatch_buf.len() {
        let (mod_id, org_id, pos, scale_affinity, _fidelity) = self.well_dispatch_buf[i];
        if scale_affinity < 0.001 { continue; } // KKIT exempt

        let org_root = self.organism_registry.get(org_id)
            .map(|o| o.root_pitch_class).unwrap_or(0);

        let mut total_fx = 0.0_f32;
        let mut total_fy = 0.0_f32;

        for (wi, well) in self.gravity_field.wells().iter().enumerate() {
            let dx = well.position[0] - pos[0];
            let dy = well.position[1] - pos[1];
            let r_sq = dx * dx + dy * dy;
            let r = r_sq.sqrt().max(0.001);

            if r > well.radius * 1.2 { continue; } // beyond influence

            let interval = ((well.root_pitch_class as i8 - org_root as i8)
                .rem_euclid(12)) as u8;
            let consonance = consonance_weight(interval);

            let m_well = self.well_energy[wi].energy * well.strength;
            let g_eff = LJ_GRAVITY * consonance * scale_affinity;

            let f_pull = g_eff * m_well / (r_sq + LJ_SOFTENING * LJ_SOFTENING);
            let f_push = LJ_REPULSE_K / (r_sq + LJ_INNER_SOFTENING * LJ_INNER_SOFTENING);

            let f_net = (f_pull - f_push).clamp(-MAX_WELL_FORCE, MAX_WELL_FORCE);

            // Positive f_net = toward well (attraction dominates)
            total_fx += (dx / r) * f_net;
            total_fy += (dy / r) * f_net;
        }

        if let Some(org) = self.organism_registry.get_mut(org_id) {
            org.apply_force([total_fx, total_fy]);
        }
    }
}
```

**Slingshots emerge naturally:** A high-energy organism entering the well gains kinetic energy from the LJ potential drop. If its kinetic energy exceeds the well depth, it exits faster than it entered — a gravity assist. No special slingshot code needed.

### 2. Chladni Spatial Patterning

Each well is a vibrating plate. Its root pitch class determines the Chladni vibration mode `(m, n)`. The nodal pattern creates preferred angular positions on the orbital trench.

**Chladni mode function** (polar form, evaluated at organism position relative to well center):

```
C(theta, r) = cos(m * theta) * J_n(k * r)   — circular Chladni pattern
```

where `J_n` is a Bessel function of the first kind (for a circular plate) and `k` is the wave number. For computational simplicity, approximate:

```rust
/// Chladni node strength at a point relative to well center.
/// Returns [-1, 1]: positive = near anti-node (high vibration),
///                   negative = near node (low vibration / stable).
fn chladni_node_value(theta: f32, r_norm: f32, m: u32, n: u32) -> f32 {
    let angular = (m as f32 * theta).cos();
    // Simplified radial: use cos(n * pi * r_norm) as stand-in for Bessel zeros
    let radial = (n as f32 * std::f32::consts::PI * r_norm).cos();
    angular * radial
}
```

**Pitch class to Chladni mode mapping:**

| Pitch class | Note | (m, n) | Pattern character |
|-------------|------|--------|-------------------|
| 0 (C) | C | (2, 1) | 2-fold symmetric, 1 radial ring |
| 1 (C#) | C# | (3, 2) | 3-fold, 2 rings |
| 2 (D) | D | (2, 2) | 2-fold, 2 rings |
| 3 (D#) | D# | (4, 1) | 4-fold, 1 ring |
| 4 (E) | E | (3, 1) | 3-fold, 1 ring |
| 5 (F) | F | (2, 3) | 2-fold, 3 rings |
| 6 (F#) | F# | (5, 1) | 5-fold, 1 ring |
| 7 (G) | G | (3, 2) | 3-fold, 2 rings |
| 8 (G#) | G# | (4, 2) | 4-fold, 2 rings |
| 9 (A) | A | (2, 1) | 2-fold, 1 ring (same as C — octave equivalence) |
| 10 (A#) | A# | (5, 2) | 5-fold, 2 rings |
| 11 (B) | B | (3, 3) | 3-fold, 3 rings |

Store as `const CHLADNI_MODES: [(u32, u32); 12]`.

**Angular force from Chladni pattern:**

Organisms near nodal lines (where `C(theta, r) ~ 0`) experience a tangential restoring force toward the nearest node. Organisms near anti-nodes (where `|C| ~ 1`) are in unstable equilibrium — perturbations push them toward nodes.

```rust
/// Tangential force from Chladni nodal pattern.
/// Consonant organisms are attracted to nodes (stable positions).
/// Dissonant organisms are pushed toward anti-nodes (unstable positions).
fn chladni_tangential_force(
    theta: f32, r_norm: f32, m: u32, n: u32, consonance: f32,
) -> f32 {
    // Gradient of angular component: -m * sin(m * theta) * radial
    let radial = (n as f32 * std::f32::consts::PI * r_norm).cos();
    let angular_gradient = -(m as f32) * (m as f32 * theta).sin() * radial;

    // Consonant organisms seek nodes (go with gradient to zero-crossings).
    // Dissonant organisms repelled from nodes (reversed gradient).
    let affinity = consonance * 2.0 - 1.0; // map [0.2, 1.0] to [-0.6, 1.0]
    angular_gradient * affinity * CHLADNI_FORCE_STRENGTH
}
```

This force is applied **tangentially** (perpendicular to the radial direction) so it steers the organism around the trench ring without affecting its radial equilibrium.

**Multiple radial rings:** The `n` parameter creates multiple nodal rings at different radii. Energetic organisms that oscillate radially (bouncing through the trench) pass through different ring patterns. This creates complex behavior without complex code — the Chladni function naturally produces multiple rings.

### 3. Boid Alignment Within Wells

Organisms orbiting the same well synchronize their orbital direction via velocity alignment (Reynolds boid rule). This is a **pure local rule** — it does not need to know about Chladni modes, LJ forces, or well energy. It composes independently.

**Algorithm (per-frame, inside well dispatch):**

```rust
// For each well, collect organisms within influence radius
// Compute average velocity of co-occupants
// Blend each organism's velocity toward the average

for well_idx in 0..well_count {
    let occupants: ArrayVec<(OrganismId, [f32; 2]), 12> = /* organisms in this well */;
    if occupants.len() < 2 { continue; }

    let avg_vel = occupants.iter()
        .map(|(_, v)| *v)
        .fold([0.0, 0.0], |a, v| [a[0] + v[0], a[1] + v[1]]);
    let n = occupants.len() as f32;
    let avg_vel = [avg_vel[0] / n, avg_vel[1] / n];

    for &(org_id, vel) in &occupants {
        let steer = [
            (avg_vel[0] - vel[0]) * BOID_ALIGN_STRENGTH,
            (avg_vel[1] - vel[1]) * BOID_ALIGN_STRENGTH,
        ];
        if let Some(org) = self.organism_registry.get_mut(org_id) {
            // Modulate by viscosity DNA param (higher viscosity = stronger alignment)
            let vis = org.viscosity;
            org.apply_force([steer[0] * vis, steer[1] * vis]);
        }
    }
}
```

**Effect:** All organisms in a well gradually orbit in the same direction (CW or CCW), determined by whichever direction the majority was already moving. New arrivals are pulled into the flow.

**Viscosity gating:** The `viscosity` DNA param (already on `OrganismState`) modulates alignment strength. Low-viscosity organisms (DRON: slow, independent) barely align. High-viscosity organisms (KKIT: rhythmic, social) snap into formation quickly. Since KKIT has `scale_affinity=0`, it is exempt from well forces entirely — but if future drum organisms use wells, viscosity provides the knob.

### 4. Beat-Pulsed Plate

Audio energy (especially from low-frequency organisms like DRON and TBLK) pulses the well's inner repulsion boundary. Beat hits momentarily inflate the repulsion radius, pushing organisms outward from the trench before they spring back. This creates the "dance" — rhythmic oscillation within the spatial pattern.

**Mechanism:**

```rust
// Per-frame: compute global audio energy (already available as sum of organism RMS)
let beat_energy = global_audio_energy; // or low-pass filtered version

// Modulate inner repulsion softening per well:
let pulse = beat_energy * BEAT_PULSE_AMPLITUDE;
let pulsed_eps_inner = LJ_INNER_SOFTENING * (1.0 + pulse);
// Use pulsed_eps_inner instead of LJ_INNER_SOFTENING in F_push calculation
```

When a beat hits, `pulsed_eps_inner` increases, which makes `F_push` smaller at the same `r` — wait, that is backwards. We want beats to push organisms outward. Two options:

**Option A: Modulate repulsion strength.** `K_repulse_pulsed = K_repulse * (1.0 + pulse)`. Stronger repulsion on beat = organisms pushed outward.

**Option B: Modulate trench radius.** Shift the equilibrium radius outward on beats.

**Chosen: Option A.** Simpler, more direct, and the visual effect is immediate — organisms "bounce" on downbeats.

```rust
let k_pulsed = LJ_REPULSE_K * (1.0 + beat_energy * BEAT_PULSE_AMPLITUDE);
let f_push = k_pulsed / (r_sq + LJ_INNER_SOFTENING * LJ_INNER_SOFTENING);
```

**`pulse_response` DNA param:** Each organism already has `pulse_response` (from `RenderDna`). Use it to gate how much the organism responds to the beat pulse. Organisms with high `pulse_response` bounce more on beats; low `pulse_response` organisms barely move.

Implementation: multiply the organism's perception of the pulsed force by its `pulse_response`:

```rust
let f_push_perceived = f_push_base + (f_push_pulsed - f_push_base) * org.pulse_response;
```

### 5. Well Energy Model

Wells are finite, regenerating stores of energy. This creates competitive pressure and temporal cycling without explicit niche penalty formulas.

**Structs (in `src/tuning/gravity_well.rs`):**

```rust
/// Per-well energy store, updated each frame.
#[derive(Clone, Debug)]
pub struct WellEnergy {
    pub well_id: u32,
    /// Current energy level [0, 1]. Drains when occupied, regenerates when empty.
    pub energy: f32,
    /// Regeneration state machine.
    pub regen_state: RegenState,
    /// Ticks spent in current regen state (for state transitions).
    pub state_ticks: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegenState {
    /// energy > 0.5 or unoccupied. Constant regen proportional to emptiness.
    Healthy,
    /// energy <= 0.5 AND occupied. Stochastic regen — increasingly unreliable.
    Wavering,
    /// Extended wavering while crowded. Well shuts off for cooldown.
    Dormant { cooldown_remaining: u32 },
}
```

**Regeneration dynamics:**

- **Healthy** (energy > 50% or unoccupied): `regen = REGEN_RATE * (1.0 - energy)`. Fuller wells regen slower.
- **Wavering** (energy <= 50% AND occupied): Each tick has probability `energy` of regenerating. As energy drops, regen becomes unreliable. Transition to Dormant after `DORMANT_ONSET_TICKS` of wavering while still crowded.
- **Dormant** (extended crowding at low energy): No energy output, no regen. After `cooldown_remaining` reaches 0, return to Healthy with seed energy `DORMANT_SEED_ENERGY`.

**Drain:**

```
drain_per_organism = BASE_DRAIN * influence * (1.0 / sqrt(occupant_count))
```

The square-root division means 4 organisms drain 2x total (not 4x). Crowding is costly but not catastrophic per-organism.

**How energy feeds the LJ model:** `M_well = well.energy * well.strength`. As energy drops, `M_well` decreases, the trench shallows, and organisms spiral outward. A completely drained well has zero attraction. This creates natural dispersion — organisms leave depleted wells, the well recharges, and they (or others) return later.

**Node Well Union (pre-union hardening):** `merge_node_wells()` in registry.rs: union all wells, dedup within 20px, cap at 12.

### 6. WellProximity — Per-Organism Ecological State

Computed each frame inside the well dispatch loop. Feeds into satisfaction and is stored on `OrganismState` for renderer/debug access.

```rust
/// Per-organism ecological snapshot for a single well.
#[derive(Clone, Copy, Debug, Default)]
pub struct WellInfluence {
    pub well_id: u32,
    /// LJ influence strength [0, ~1]: how deep in the trench.
    pub influence: f32,
    /// Consonance between organism root and well root [0.2, 1.0].
    pub consonance: f32,
    /// Chladni node value at organism position [-1, 1].
    /// Negative = near node (stable), positive = near anti-node (unstable).
    pub chladni_value: f32,
    /// Combined ecological quality: influence * consonance * stability.
    pub quality: f32,
}

/// Per-organism summary of all well interactions this frame.
#[derive(Clone, Debug, Default)]
pub struct WellProximity {
    /// Up to 6 well influences, sorted by quality descending.
    pub influences: [WellInfluence; 6],
    pub influence_count: u8,
    /// Best single quality score across all wells.
    pub best_quality: f32,
    /// Niche penalty [0, 1]: spectral overlap with co-occupants.
    pub niche_penalty: f32,
    /// Net ecological score: best_quality * well_energy * (1 - niche_penalty).
    pub net_score: f32,
}
```

Uses fixed array `[WellInfluence; 6] + count: u8` to avoid `arrayvec` dependency. Max 6 wells is a hard constraint.

### 7. Spectral Niche Pressure

After computing raw `WellProximity` for all organisms, a second pass computes niche penalties using the frequency-ratio model from the previous spec (preserved unchanged).

**Algorithm:**

```
For each well W:
    Collect organisms O_i with influence > 0.0 in W
    For each pair (O_i, O_j):
        ratio_dist = |log2(centroid_i / centroid_j)|    // in octaves
        spectral_overlap = (1.0 - ratio_dist / OCTAVE_THRESHOLD).clamp(0, 1)
        influence_overlap = min(O_i.influence_at_W, O_j.influence_at_W)
        pair_pressure = spectral_overlap * influence_overlap
    O_i.niche_penalty_at_W = sum(pair_pressure for all j != i).clamp(0, 1)
```

**Constants:**
- `OCTAVE_THRESHOLD = 1.5` — distance in octaves at which spectral overlap = 0.

**Data source:** `spectral_centroid` from `OrganismModule.current_spectral_centroid` (already available, emitted via `DspAnalysis`). Add `spectral_centroid: f32` to the `well_dispatch_buf` tuple (becomes 6-tuple).

### 8. Satisfaction Integration

**Decision: satisfaction, not valence.** Well ecology modifies `port_satisfaction()` on the pitch port, targeting exactly the right Hebbian edges.

```rust
fn port_satisfaction(&self, port: PortId) -> f32 {
    if port == self.pitch_hz_port && self.musical_context.scale_active {
        let cents = self.musical_context.pitch_deviation_cents.abs();
        let tolerance = 50.0 / self.musical_context.scale_blend.max(0.01);
        let pitch_sat = (1.0 - (cents / tolerance)).clamp(0.0, 1.0);

        // Ecological bonus: well_energy * quality * (1 - niche_penalty)
        let eco_bonus = self.well_proximity.net_score * WELL_SAT_WEIGHT;
        return (pitch_sat + eco_bonus).clamp(0.0, 1.0);
    }
    // ... rest unchanged
}
```

`WELL_SAT_WEIGHT = 0.2` — a 20% maximum bonus at perfect well alignment. Net score already incorporates well energy, quality, and niche penalty.

**Data flow:** `app.rs` calls `org_mod.set_well_proximity(proximity)` during the dispatch loop (same pattern as `send_command()` / gravity weight dispatch).

### 9. Orbit/Glob Interaction Mode Removal

The following are **replaced** by the LJ + Chladni + boid alignment model and should be removed from organism DNA files:

| Organism | Current Orbit/Glob rules | Replacement behavior |
|----------|--------------------------|----------------------|
| DRON | `Orbit * range=500 str=4` | LJ trench orbit (scale_affinity=0.3, gentle) |
| HOSO | `Orbit * range=300 str=8`, `Glob acid range=200 str=5` | LJ trench + Chladni nodes (scale_affinity=0.8, strong locking) |
| ACID | `Orbit * range=380 str=10`, `Glob hoso range=200 str=5` | LJ trench + Chladni nodes |
| SPGL | `Orbit * range=600 str=3` | LJ trench (wide, gentle) |
| TBLK | `Orbit kkit range=250 str=8`, `Orbit * range=350 str=14` | LJ trench + boid alignment |
| KKIT | `Orbit tblk range=250 str=8`, `Orbit * range=300 str=12` | **Exempt** (scale_affinity=0). Keep Repel only. |
| ISAO | `Orbit * range=350 str=8`, `Glob acid/hoso range=200 str=5` | LJ trench + Chladni nodes |

**DNA changes:** Remove all `"Orbit"` and `"Glob"` rules from `assets/dna/*.json`. Keep `"Repel"`, `"Bounce"`, `"Attach"`, and `"IntegratePropose"` rules unchanged. The `InteractionMode::Orbit` and `InteractionMode::Glob` enum variants remain in code (for backward compatibility with any saved DNA) but are no longer used by shipped organisms.

**Code changes:** The `orbit()` and `glob()` functions in `interaction.rs` remain (not deleted) but are effectively dead code for shipped organisms. The `continuous_pull()` function remains as well (used by Attach mode).

---

## DNA Schema Changes

### New field: `physics.well_response`

Optional object on `PhysicsDna`, controlling per-species well behavior:

```json
"physics": {
    "mass": 1.0,
    "drag": 0.9,
    "max_speed": 150.0,
    "viscosity": 0.5,
    "well_response": {
        "lj_gravity_scale": 1.0,
        "chladni_lock_strength": 1.0,
        "beat_pulse_sensitivity": 0.5
    },
    "interaction_rules": [
        { "with_species": "*", "mode": "Repel", "range": 100.0, "strength": 8.0 }
    ]
}
```

| Field | Default | Meaning |
|-------|---------|---------|
| `lj_gravity_scale` | 1.0 | Multiplier on LJ gravitational pull. <1 = looser orbit, >1 = tighter. |
| `chladni_lock_strength` | 1.0 | How strongly organism locks to Chladni nodes. 0 = ignores pattern. |
| `beat_pulse_sensitivity` | 0.5 | How much beat pulses displace this organism. 0 = deaf to beats. |

**Backward compatibility:** If `well_response` is missing from DNA JSON, use defaults `(1.0, 1.0, 0.5)`.

### Removed rules (per organism)

All `"Orbit"` and `"Glob"` interaction rules removed from DNA files. See table in Section 9 above.

---

## Constants

All named constants in `src/tuning/gravity_well.rs` (or a new `src/tuning/well_physics.rs` if the file grows large). All are tunable — initial values are starting points for aesthetic tuning.

| Constant | Default | Units | Purpose |
|----------|---------|-------|---------|
| `LJ_GRAVITY` | 800.0 | px^2/tick | Base gravitational constant for LJ pull |
| `LJ_SOFTENING` | 30.0 | px | Prevents pull singularity at r=0 |
| `LJ_REPULSE_K` | 12000.0 | px^2/tick | Inner repulsion strength |
| `LJ_INNER_SOFTENING` | 15.0 | px | Prevents repulsion singularity at r=0 |
| `LJ_TRENCH_FRACTION` | 0.6 | ratio | Target trench radius as fraction of well.radius |
| `MAX_WELL_FORCE` | 50.0 | force | Per-well force clamp (defense-in-depth) |
| `CHLADNI_FORCE_STRENGTH` | 3.0 | force | Tangential force from Chladni nodal pattern |
| `BOID_ALIGN_STRENGTH` | 0.15 | ratio/tick | Velocity alignment blend rate |
| `BEAT_PULSE_AMPLITUDE` | 0.8 | ratio | Maximum beat-driven repulsion boost |
| `OCTAVE_THRESHOLD` | 1.5 | octaves | Spectral niche overlap bandwidth |
| `WELL_SAT_WEIGHT` | 0.2 | ratio | Satisfaction bonus weight from well ecology |
| `REGEN_RATE` | 0.01 | /tick | Full regen from 0 to 1 in ~100 ticks when unoccupied |
| `BASE_DRAIN` | 0.005 | /tick | Single organism drain rate per tick |
| `WAVER_THRESHOLD` | 0.5 | energy | Energy below which wavering begins |
| `DORMANT_ONSET_TICKS` | 300 | ticks | Wavering ticks before dormancy (~2.5s at 120Hz) |
| `DORMANT_COOLDOWN` | 600 | ticks | Dormancy duration before reactivation (~5s at 120Hz) |
| `DORMANT_SEED_ENERGY` | 0.1 | energy | Energy level when waking from dormancy |

**Tuning note:** `LJ_GRAVITY` and `LJ_REPULSE_K` must be tuned together so the trench radius lands at `LJ_TRENCH_FRACTION * well.radius`. The relationship is approximately:

```
r_eq^2 ≈ (K_repulse / (G * M_well)) * eps^2 - eps_inner^2
```

With `M_well ~ 0.6` (healthy well, strength=0.6), `G=800`, `K=12000`, `eps=30`, `eps_inner=15`:
`r_eq^2 ≈ (12000 / (800 * 0.6)) * 900 - 225 = 25 * 900 - 225 = 22275`, so `r_eq ≈ 149 px`.
For a well with radius 250px, that is `149/250 = 0.60` — matches `LJ_TRENCH_FRACTION`.

---

## Implementation Phases

### Phase A: LJ Force Model + Energy (core physics)

1. Add `WellEnergy`, `RegenState` structs to `gravity_well.rs`.
2. Add `well_energy: Vec<WellEnergy>` to `SolidoApp`, initialized alongside `GravityField`.
3. Extract `consonance_weight(interval: u8) -> f32` as a public free function in `gravity_well.rs`.
4. Replace `apply_gravity_well_forces()` with `apply_well_forces()` using the LJ model.
5. Add well energy drain/regen tick (called once per frame, before force application).
6. Add `MAX_WELL_FORCE` clamping.
7. Verify: organisms orbit at trench radius instead of falling to center.

### Phase B: Chladni Patterning

1. Add `CHLADNI_MODES` constant and `chladni_node_value()` function.
2. Add `chladni_tangential_force()` and apply in well force loop.
3. Verify: organisms cluster at angular positions determined by well pitch class.

### Phase C: Boid Alignment + Beat Pulse

1. Add boid alignment pass after well force computation.
2. Add beat pulse modulation to LJ repulsion.
3. Verify: organisms orbit in same direction; beat hits cause visible bounce.

### Phase D: Ecology Integration

1. Add `WellInfluence`, `WellProximity` structs.
2. Compute `WellProximity` per organism each frame.
3. Add spectral niche penalty pass.
4. Wire `WellProximity` into `OrganismModule` via `set_well_proximity()`.
5. Add ecological bonus to `port_satisfaction()`.
6. Add `well_response` DNA field parsing with defaults.

### Phase E: DNA Cleanup

1. Remove Orbit and Glob rules from all `assets/dna/*.json` files.
2. Verify: all organisms behave correctly with only Repel + LJ well physics.

---

## Critical Files

| File | Role | Changes |
|------|------|---------|
| `src/tuning/gravity_well.rs` | Well physics core | Add `WellEnergy`, `RegenState`, `WellInfluence`, `WellProximity`, `consonance_weight()`, `chladni_node_value()`, `chladni_tangential_force()`, `CHLADNI_MODES`, all LJ/ecology constants |
| `src/app.rs` | Main loop | Replace `apply_gravity_well_forces()` with `apply_well_forces()`. Add well energy tick. Add boid alignment pass. Add beat pulse. Compute + distribute `WellProximity`. Expand `well_dispatch_buf` to 6-tuple. |
| `src/organism/module/mod.rs` | Organism module | Add `well_proximity: WellProximity` field + `set_well_proximity()` setter. Modify `port_satisfaction()` for ecological bonus. |
| `src/organism/sim.rs` | Organism state | Add `well_proximity: WellProximity` to `OrganismState` (for renderer). |
| `src/organism/dna.rs` | DNA types | Add `WellResponseDna` struct with defaults. Parse in `PhysicsDna`. |
| `src/organism/interaction.rs` | Interaction forces | No changes (orbit/glob remain as dead code). |
| `src/organism/registry.rs` | Organism registry | No changes (interaction dispatch unchanged). |
| `assets/dna/*.json` | DNA files | Remove Orbit/Glob rules. Add `well_response` objects. |
| `src/dsp/command.rs` | DSP analysis | No changes (`spectral_centroid` already present). |
| `src/affinity/emotion.rs` | Emotion system | No changes (satisfaction path used, not valence). |

---

## Dependencies

- **S36 (physics hardening)**: Required. LJ forces at variable timestep would produce frame-rate-dependent orbits. The 120Hz fixed timestep is foundational.
- **No new crate dependencies.** Fixed arrays instead of arrayvec. Chladni approximation uses `cos()` and basic trig, no Bessel library needed.
- **All required infrastructure exists:** `spectral_centroid` on `DspAnalysis`, `port_satisfaction()` override on `OrganismModule`, `consonance_weight` computation in `app.rs`, `well_dispatch_buf` pre-allocation, `pulse_response` and `viscosity` DNA params.

---

## Verification

### Unit tests (in `gravity_well.rs`)

1. **`consonance_weight` correctness**: unison=1.0, fifth=0.8, tritone=0.2.
2. **LJ force at trench radius**: Construct well with known parameters. Verify `F_pull ≈ F_push` at expected trench radius (net force near zero).
3. **LJ force direction**: Organism inside trench radius → net repulsion (outward). Organism outside trench → net attraction (inward).
4. **LJ force clamping**: Extreme inputs produce force within `[-MAX_WELL_FORCE, MAX_WELL_FORCE]`.
5. **LJ force with zero energy**: Well energy=0 → F_pull=0, net force is pure repulsion (organism expelled).
6. **Chladni node value symmetry**: `chladni_node_value(0, r, 2, 1) == chladni_node_value(PI, r, 2, 1)` (2-fold symmetry).
7. **Chladni node value at nodal line**: For m=2, theta=PI/4 → `cos(2 * PI/4) = cos(PI/2) = 0` → node.
8. **Chladni tangential force sign**: Consonant organism near anti-node → force pushes toward nearest node. Dissonant organism → force pushes away from node.
9. **WellEnergy drain**: Single occupant with influence=1.0 drains at `BASE_DRAIN` per tick.
10. **WellEnergy regen**: Unoccupied well regenerates at `REGEN_RATE * (1 - energy)`.
11. **RegenState transitions**: Healthy → Wavering when energy drops below threshold while occupied. Wavering → Dormant after `DORMANT_ONSET_TICKS`. Dormant → Healthy after cooldown with seed energy.
12. **Niche penalty with identical centroids**: Two organisms at same position, same centroid → penalty approaches 1.0.
13. **Niche penalty with distant centroids**: Centroids 3 octaves apart → penalty = 0.0.
14. **WellProximity outside all wells**: All fields zero/default.

### Integration tests

15. **Satisfaction bonus**: Create `OrganismModule` with `well_proximity.net_score = 0.5`. Verify `port_satisfaction(pitch_hz_port)` increases by `0.5 * WELL_SAT_WEIGHT = 0.1` relative to baseline.
16. **Hebbian flow**: Over many ticks, organism near consonant energized well should see pitch edges strengthen faster than organism far from wells.

### Visual / manual tests

17. **Trench orbit**: Spawn single organism near a well. It should settle into a circular orbit at the trench radius, NOT fall to the center.
18. **Chladni locking**: Spawn 3 organisms near a well with m=3 mode. They should distribute ~120 degrees apart on the trench ring.
19. **Boid alignment**: Spawn 2 organisms in same well moving in opposite directions. Within seconds, both should orbit in the same direction.
20. **Beat pulse dance**: Play rhythmic audio (KKIT or TBLK). Organisms near wells should visibly bounce on downbeats.
21. **Energy depletion**: Pack 4+ organisms into one well. Watch well energy drain. Organisms should gradually spiral outward as the trench shallows. After organisms leave, well should recharge.
22. **Slingshot**: Launch an organism at high speed toward a well from outside. It should curve through the well and exit with at least its entry speed (gravity assist).
23. **KKIT immunity**: KKIT (scale_affinity=0) should be completely unaffected by well forces.
24. **Consonance differentiation**: Place DRON (root=A) near a G well (fifth relationship, consonance=0.8). Place SPGL (root=F#) near the same well (tritone, consonance=0.2). DRON should orbit tighter and lock to nodes more strongly.

---

## Resolved Decisions

### Quadratic falloff vs LJ
LJ (softened). Quadratic has equilibrium at center — boring. LJ creates an orbital trench — organisms move.

### Orbit/Glob interaction modes
Removed from DNA. LJ trench replaces Orbit (radial equilibrium). Chladni nodes + boid alignment replace Glob (angular positioning + velocity sync).

### Chladni implementation
Simplified trigonometric approximation: `cos(m*theta) * cos(n*pi*r_norm)`. No Bessel functions needed for the aesthetic effect. Can upgrade later if visual fidelity demands it.

### Beat pulse mechanism
Modulate repulsion strength (Option A), not softening. Stronger repulsion on beat = organisms pushed outward = visible bounce.

### Satisfaction vs valence for ecological feedback
Satisfaction. Well ecology is about quality of a specific interaction (spatial positioning relative to harmonic field), which maps to the pitch port. Valence is whole-module mood driven by homeostatic throughput — wrong granularity.

### Niche penalty model
Frequency-ratio (log2) with `OCTAVE_THRESHOLD = 1.5`. Combined with well energy drain, competition emerges from two independent mechanisms: spectral overlap reduces satisfaction directly, and occupant drain depletes the shared resource.

### Fixed array vs ArrayVec
Fixed array `[WellInfluence; 6] + count: u8`. Avoids new dependency. 6 wells max is a hard constraint.
