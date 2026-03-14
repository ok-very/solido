# S36 — Physics Hardening

**Status**: Complete (Mar 2026)
**Depends on**: Nothing — this is foundational
**Blocks**: S38 (well ecology), S39 (navigation reward), organism union/merge (force balance during fusion)

---

## Goal

Fix frame-rate-dependent force application so the physics simulation produces consistent organism behavior regardless of display refresh rate. Currently, forces are applied as raw velocity deltas without dt scaling, while position integration and drag ARE dt-scaled -- creating a 2:1 acceleration difference between 30fps and 60fps machines.

---

## Context: The dt-Scaling Problem

### What works (frame-rate independent)

| System | Code | dt handling |
|--------|------|-------------|
| Drag | `sim.rs:232-234` | `self.drag.powf(dt)` -- exponential decay per second |
| Position integration | `sim.rs:247-248` | `velocity * dt` -- Euler step |
| Wander thrust | `sim.rs:262-263` | `wander_strength * dt` -- correctly scaled |
| Wall heading deflection | `sim.rs:298-310` | `2.0 * dt` -- correctly scaled |
| Smoothing rates | `sim.rs:280,293,224,228,314` | All use `rate * dt` pattern |

### What is broken (frame-rate dependent)

| System | Code | Problem |
|--------|------|---------|
| `apply_force()` | `sim.rs:385-389` | `velocity += force * inv_mass` -- NO dt |
| Audio impulse | `sim.rs:269-271` | `velocity += heading * impulse` -- NO dt |
| Boundary forces | `registry.rs:267-288` | Direct velocity += with no dt |
| Curiosity forces | `registry.rs:101-104` via `sonar.rs:112-117` | `apply_force(curiosity)` -- no dt |
| Gravity well pull | `app.rs:1108-1114` | `apply_force([total_fx, total_fy])` -- no dt |
| Pairwise interactions | `registry.rs:246-248` | `apply_force(accumulated)` -- no dt |
| Continuous pull | `registry.rs:222-233` | `apply_force(continuous_pull_result)` -- no dt |

### The arithmetic

Forces add to velocity via `apply_force()`. Each call does:
```rust
self.velocity[0] += force[0] * inv_mass;  // once per frame, no dt
```

At 60fps: 60 applications/sec, total impulse = `force * inv_mass * 60`
At 30fps: 30 applications/sec, total impulse = `force * inv_mass * 30`

The 60fps machine delivers **twice the acceleration per second**. Meanwhile, `position += velocity * dt` correctly integrates position -- so the velocity accumulation error compounds into position divergence every frame.

### Why it hasn't been noticed

Current forces are gentle (well pull x12, continuous_pull max 15N, curiosity 8, boundary 50) and drag dominates behavior. Organisms reach terminal velocity quickly, and the exponential drag (correctly dt-scaled) masks the force asymmetry. But:

- Gravity wells as ecological features (S38) will use stronger forces
- Reward-driven navigation (S39) needs deterministic trajectories
- Different frame rates producing different ecological experiences undermines the entire ecology system

---

## Force Audit

Every site where velocity is modified outside of `OrganismState::tick()`:

| # | Site | File:Line | Current behavior | dt status | Category |
|---|------|-----------|------------------|-----------|----------|
| 1 | `apply_force()` | `sim.rs:385-389` | `vel += force * inv_mass` | BROKEN | Core method -- all callers inherit this bug |
| 2 | Audio impulse | `sim.rs:269-271` | `vel += heading * rms_delta * 80.0` | BROKEN | Per-frame event impulse |
| 3 | Boundary forces | `registry.rs:267-288` | `vel += force * penetration^2` | BROKEN | Direct velocity modification (bypasses `apply_force`) |
| 4 | Curiosity forces | `registry.rs:101-104` | Calls `apply_force()` with sonar result | BROKEN (via #1) | Continuous force |
| 5 | Gravity well pull | `app.rs:1108-1114` | Calls `apply_force([total_fx, total_fy])` | BROKEN (via #1) | Continuous force |
| 6 | Pairwise interactions | `registry.rs:246-248` | Calls `apply_force(accumulated)` | BROKEN (via #1) | Continuous force |
| 7 | Continuous pull | `registry.rs:222-233` | Calls `apply_force(continuous_pull_result)` | BROKEN (via #1) | Continuous force |
| 8 | Wander thrust | `sim.rs:262-263` | `vel += heading * wander * dt` | OK | Already dt-scaled |
| 9 | Drag | `sim.rs:232-234` | `vel *= drag.powf(dt)` | OK | Exponential decay |
| 10 | Position integration | `sim.rs:247-248` | `pos += vel * dt` | OK | Euler step |

### Interaction function force outputs (all flow through #6)

| Function | File | Returns | Notes |
|----------|------|---------|-------|
| `repel()` | `interaction.rs:102-120` | Force magnitude | Quadratic falloff, no dt |
| `attract()` | `interaction.rs:125-143` | Force magnitude | Mirror of repel |
| `bounce()` | `interaction.rs:146-186` | Force + velocity reflection | Bounce impulse is intentionally per-event but mixed with repel force |
| `slow()` | `interaction.rs:189-211` | Viscous drag force | Velocity-proportional, should scale with dt |
| `attach()` | `interaction.rs:216-238` | Spring force | Displacement-proportional |
| `orbit()` | `interaction.rs:297-320` | Tangential force | Bell-curve magnitude |
| `continuous_pull()` | `interaction.rs:327-372` | Pull + damping | Includes velocity damping term |
| `glob()` | `interaction.rs:244-290` | Attraction + centroid pull + viscous drag | Multiple force components |

---

## Chosen Approach: Fixed Timestep Substep (Option 2)

**Decision**: 120Hz fixed timestep with accumulator. This is the industry-standard approach (Box2D, Bullet, Rapier).

```rust
// app.rs — update loop
const PHYS_DT: f32 = 1.0 / 120.0;  // 120Hz fixed step
let mut accumulator = /* carried from last frame */ + delta;
accumulator = accumulator.min(0.1);  // cap at 100ms to prevent spiral

while accumulator >= PHYS_DT {
    self.organism_registry.tick(PHYS_DT);
    accumulator -= PHYS_DT;
}
// Store remainder for next frame
self.phys_accumulator = accumulator;
```

**Why Option 2:**
- **Deterministic** — same initial state always produces same trajectory, regardless of display rate
- **No force constant retuning** — calibrate once for 120Hz, every machine gets the same result
- **Handles dt spikes gracefully** — accumulator capped at 100ms means max 12 substeps, never an explosion
- **apply_force() unchanged** — no dt parameter needed, forces are applied at constant rate

**Rendering interpolation**: Skip for now. Physics jumps at 120Hz are invisible for SDF blob rendering. Can add position lerp later if needed.

**Gravity well forces**: Compute once per frame, apply per substep. Well positions are static within a frame, so per-substep recomputation adds cost for no benefit. The approximation is acceptable for gentle well forces.

---

## dt Handling

The accumulator cap (`0.1`) handles dt spikes naturally — no separate clamp needed. Max 12 substeps per frame at the worst case. Constants stay as-is, calibrated for the fixed 120Hz timestep.

---

## Audio Impulse: Per-Event vs Continuous

The audio impulse (`sim.rs:266-272`) deserves special treatment:

```rust
let rms_delta = (self.audio_energy - self.prev_audio_energy).max(0.0);
if rms_delta > 0.05 {
    let impulse = rms_delta * 80.0;
    self.velocity[0] += self.heading.cos() * impulse;
    self.velocity[1] += self.heading.sin() * impulse;
}
```

**Question**: Is this a continuous force or a discrete event?

- **If continuous**: It should scale by dt. At 60fps, small rms_deltas accumulate; at 30fps, larger rms_deltas arrive less often. dt scaling normalizes total impulse per second.
- **If discrete event**: It should NOT scale by dt. Each audio energy spike is a single "kick" regardless of frame rate. But then at 60fps the organism gets 2x as many kicks per second as at 30fps (because audio_energy is sampled more often).
- **Compromise**: Scale by `dt / reference_dt` where `reference_dt = 1/60`. At 60fps this is 1.0 (no change). At 30fps this is ~2.0 (double the impulse to compensate for half the samples). This normalizes total impulse per second while preserving the "kick" feel.

**This is an aesthetic question** -- the "correct" answer depends on whether audio-driven movement should feel like continuous propulsion or discrete percussive kicks.

---

## Bounce Impulse: Per-Event Treatment

The bounce interaction (`interaction.rs:174`) computes a velocity reflection:

```rust
let bounce_impulse = -vel_dot_normal * (1.0 + friction);
```

This is a **collision response**, not a continuous force. With the fixed 120Hz timestep, bounce impulses fire at consistent granularity. Acceptable — no special treatment needed.

---

## Integration: How the Update Loop Changes

### Current flow (app.rs)

```
egui frame callback:
  1. compute delta from wall clock
  2. reactor.tick(delta)           -- module signals
  3. gravity well forces           -- apply_force() on organisms
  4. organism_registry.tick(delta) -- boundary forces, interactions, sonar, sim.tick()
```

### Fixed timestep loop

```
egui frame callback:
  1. compute delta
  2. reactor.tick(delta)           -- module signals (still variable rate)
  3. gravity well forces           -- compute once per frame (well positions static)
  4. accumulator += delta
  5. while accumulator >= PHYS_DT:
       a. apply gravity forces     -- pre-computed from step 3
       b. organism_registry.tick(PHYS_DT)
       accumulator -= PHYS_DT
  6. store accumulator for next frame
```

Gravity well forces are computed once per frame (step 3) and applied per substep (step 5a). Well positions don't change within a frame, so per-substep recomputation would waste cycles for no accuracy gain.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/organism/sim.rs` | `apply_force()`: no signature change. Audio impulse: treat as discrete event (no dt scaling). |
| `src/organism/registry.rs` | `tick()`: receives fixed PHYS_DT. Boundary forces: unchanged (run at fixed rate). |
| `src/organism/interaction.rs` | No changes needed (force magnitudes unchanged at fixed rate). |
| `src/organism/sonar.rs` | No changes needed (curiosity forces applied at fixed rate). |
| `src/app.rs` | Add `phys_accumulator: f32` field. Restructure update loop: gravity well forces computed once, physics substep loop with accumulator. |
| `assets/dna/*.json` | No changes needed (constants calibrated for fixed timestep). |

---

## Dependencies

This spec has no dependencies and should ship **before**:
- S37 (animation recalibration) — smoothing rates depend on physics dt
- S38 (well ecology) — needs deterministic force response
- S39 (navigation reward) — needs consistent trajectories across machines
- Organism union/merge — force balance during fusion events

---

## Verification

### Determinism test (all options)

1. Set up two organisms at known positions with known velocities
2. Run physics for N seconds at 30fps (dt=0.033)
3. Run physics for N seconds at 60fps (dt=0.016)
4. Run physics for N seconds at 144fps (dt=0.0069)
5. Compare final positions

With fixed timestep, positions should be **identical** regardless of display frame rate (same number of physics steps per second).

### Unit test sketch

```rust
#[test]
fn force_framerate_independence() {
    let make_org = || {
        let mut org = OrganismState::new(0, [500.0, 500.0], 4, 20.0);
        org.drag = 0.9;
        org.mass = 1.0;
        org
    };

    // Simulate 1 second at different frame rates
    let mut org_30 = make_org();
    for _ in 0..30 {
        org_30.apply_force([10.0, 0.0], 1.0/30.0);
        org_30.tick(1.0/30.0, [0.0, 0.0, 2000.0, 2000.0]);
    }

    let mut org_60 = make_org();
    for _ in 0..60 {
        org_60.apply_force([10.0, 0.0], 1.0/60.0);
        org_60.tick(1.0/60.0, [0.0, 0.0, 2000.0, 2000.0]);
    }

    // Positions should be within 5% of each other
    let diff = (org_30.position[0] - org_60.position[0]).abs();
    let avg = (org_30.position[0] + org_60.position[0]) * 0.5;
    assert!(diff / avg < 0.05, "30fps={} vs 60fps={}", org_30.position[0], org_60.position[0]);
}
```

### Visual smoke test

1. Run app on 60Hz monitor, note organism orbital behavior near gravity wells
2. Force 30fps with frame limiter or `std::thread::sleep`
3. Orbits, speeds, and boundary behavior should look identical
4. Drag/minimize window, release -- organisms should not teleport or explode

---

## What This Spec Does NOT Cover

- New forces or ecological mechanics (S38, S39)
- Animation timing or visual interpolation (S37)
- Audio thread timing (already frame-rate independent by design via cpal callback)
- Reactor tick rate (module signal routing -- currently variable, acceptable)
