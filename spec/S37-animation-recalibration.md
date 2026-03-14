# S37 — Animation Recalibration

## Status: Complete (Mar 2026)

## Goal

Retune visual timing constants and animation curves so organisms express ecological dynamics — slingshot bursts, well capture, directed transit — without losing their organic, meditative quality at rest.

## Context

### Current timing constants

The animation system has two halves: CPU-side smoothing (sim.rs) that drives `ring_phase`, `smooth_speed`, and `visual_dir`, and GPU-side shader parameters (biofield.wgsl) that turn those into Chladni lobe geometry, crawl bias, and concentric ring patterns.

**CPU (sim.rs):**

| Signal | Smoothing | 95% convergence | Code |
|--------|-----------|-----------------|------|
| `smooth_speed` | `(8.0 * dt).min(1.0)` | ~375ms (8 Hz EMA) | `sim.rs:293` |
| `visual_dir` | `(6.0 * dt).min(1.0)` | ~500ms (6 Hz EMA) | `sim.rs:280` |
| `scale_drift_blend` | `0.4 * dt` | ~7.5s | `sim.rs:314` |

Note: 95% convergence for a first-order EMA with rate `k` Hz is `3/(k)` seconds (three time constants). At 60fps with `dt=0.0167`, `(8.0*dt)=0.133` per frame, so convergence in ~22 frames = ~370ms. `(6.0*dt)=0.1` per frame, ~30 frames = ~500ms.

**CPU crawl rate derivation (sim.rs:316-320):**

```
crawl_rate = clamp(smooth_speed / 50.0, 0.05, 2.0)    // rad/s
audio_mod  = 1.0 + audio_energy * 0.8                  // [1.0, 1.8]
ring_phase += dt * crawl_rate * audio_mod               // unbounded accumulator
```

Effective ring_phase rate: [0.05, 3.6] rad/s. At 60fps, one full 2pi cycle takes [1.7s, 125.7s]. The useful animation range is roughly 0.1-3.0 rad/s; below 0.1 is imperceptibly slow, above 3.0 starts feeling mechanical.

**GPU (biofield.wgsl:142-206):**

| Parameter | Expression | Range | Saturates at |
|-----------|-----------|-------|-------------|
| `speed_factor` | `min(speed / 50.0, 1.0)` | [0, 1] | 50 px/s |
| `crawl_bias` | `cos(theta - heading) * speed_factor * 0.6` | [-0.6, 0.6] | 50 px/s |
| `dance_intensity` | `0.15 + energy * 0.85` | [0.15, 1.0] | energy=1.0 |
| `extension` | `amp * chladni * dance_intensity + crawl_bias * amp` | varies | — |
| `reach` | `r * (0.25 + extension * 0.45)` | r*[0.25, 0.70] | — |

**GPU ring pattern (biofield.wgsl:370-396):**

```wgsl
let circles = sqrt(abs(d) * 6.0) * 5.0 - ring_phase;
let r0 = sin(circles * 1.0 + 2.0);
let r1 = abs(sin(circles - 1.0) - sin(circles * 0.7));
```

Ring animation speed is directly coupled to `ring_phase` rate. At crawl_rate=2.0 with audio_mod=1.8, the rings cycle at 3.6 rad/s. This is already at the edge of "meditative" — faster would look stroboscopic.

**Radius swell (registry.rs:442):**

```rust
let energy_swell = 1.0 + org.audio_energy * 0.3;
```

Purely audio-driven. Speed has no influence on visual size.

### Species physical profiles

| Species | max_speed | harmonic_count | m_mode | n_mode | Character |
|---------|-----------|----------------|--------|--------|-----------|
| DRON | 80 | 2 (ellipse) | 2 | 1 | Slow drift, gentle breathing |
| HOSO | 60 | 3 (trefoil) | 3 | 2 | Moderate, precise |
| ACID | 100 | 4 (square) | 3 | 1 | Fast, aggressive |
| SPGL | 40 | 5 (starfish) | 5 | 3 | Slowest, most complex pattern |
| TBLK | 120 | 3 (trefoil) | 2 | 1 | Fastest, snappy |
| KKIT | 80 | 2 (ellipse) | 4 | 2 | Mechanical |

### Why recalibration is needed

Ecology specs (S36, S38, S39) introduce gravity-well dynamics that create speed profiles the current animation system cannot express:

1. **Slingshot bursts.** Organisms escaping a well can briefly exceed `max_speed`. A TBLK slingshot at 200 px/s is visually identical to TBLK at 100 px/s because `crawl_rate` ceilings at 2.0 and `speed_factor` saturates at 50 px/s.

2. **Rapid acceleration/deceleration.** Well capture creates approach-at-speed → decelerate-to-orbit → settle cycles that happen in 0.5-1.5s. `smooth_speed` at 8 Hz takes ~370ms to track, which is borderline. `visual_dir` at 6 Hz takes ~500ms — a sharp well-slingshot turn would visually lag.

3. **Directed transit.** Organisms navigating between wells 400-700px apart at full speed should look purposeful. The current crawl_bias saturates at 50 px/s, making a 120 px/s TBLK look no more directional than one at 50 px/s.

4. **Phase precision.** `ring_phase` is an unbounded `f32` accumulator. After 10 hours at average 1.0 rad/s, phase reaches ~36,000 — still fine for f32. After 100 hours (a gallery installation), it reaches 360,000. The `sin(circles)` in the shader operates on `sqrt(d)*5.0 - ring_phase`, where sqrt(d)*5.0 is small (~0-15) and ring_phase is large. Catastrophic cancellation degrades precision. Not urgent but worth fixing while touching this code.

## Crawl Rate Curve

### Current: linear with hard clamp

```
crawl_rate = clamp(speed / 50.0, 0.05, 2.0)
```

This is a straight line from 0 to 100 px/s (where it hits the 2.0 ceiling), then flat. Every px/s above 100 looks identical.

```
crawl_rate
  2.0 |                 _______________
      |                /
  1.0 |              /
      |            /
  0.5 |          /
      |        /
 0.05 |______/
      +-----|----|----|----|----|------→ speed (px/s)
      0    25   50   75  100  150  200
```

### Proposed: logarithmic compression with raised ceiling

```
crawl_rate = 0.05 + CRAWL_GAIN * ln(1 + speed / CRAWL_REF)
```

Where `CRAWL_REF` controls the knee (speed at which compression becomes noticeable) and `CRAWL_GAIN` controls the overall scale. A soft ceiling replaces the hard clamp — very high speeds produce diminishing but nonzero increases.

**Candidate values:** `CRAWL_REF = 30.0`, `CRAWL_GAIN = 0.7`

```
speed → crawl_rate
   0       0.05          (idle breathing — unchanged)
  10       0.28          (ambling — slightly faster than current 0.20)
  25       0.55          (walking — similar to current 0.50)
  50       0.88          (current saturation point — similar to current 1.00)
  80       1.15          (DRON/KKIT max_speed — current gives 1.6, proposed is slower)
 100       1.30          (ACID max_speed — current is capped at 2.0, proposed is gentler)
 120       1.42          (TBLK max_speed — current is capped at 2.0, proposed distinguishable)
 200       1.72          (slingshot — current still 2.0, proposed is 1.72, visually distinct from 120)
 400       2.15          (extreme overshoot — still well-behaved)
```

```
crawl_rate
  2.5 |
      |                                         ___...
  2.0 |                                    ____/
      |                              _____/
  1.5 |                        _____/
      |                   ____/
  1.0 |              ____/
      |         ____/
  0.5 |     ___/
      |   _/
 0.05 |__/
      +-----|----|----|----|----|----|----|------→ speed (px/s)
      0    25   50   75  100  150  200  400
```

**Key property:** Every speed increase produces a visible crawl rate increase. 200 px/s is distinguishable from 120 px/s (1.72 vs 1.42 — 21% faster, producing one Chladni cycle every 3.6s vs 4.4s). The curve compresses gently so slingshots look energetic but not frantic.

**With audio modulation** (audio_mod up to 1.8): Maximum effective ring_phase rate is `2.15 * 1.8 = 3.87 rad/s` at extreme overshoot, which is comparable to the current maximum of 3.6. At normal slingshot speeds (200 px/s), effective rate is `1.72 * 1.8 = 3.10 rad/s` — fast but not stroboscopic.

### Ring phase decoupling (decided: audio-driven rings)

The ring concentric pattern and the Chladni lobe animation are decoupled. Rings use a separate audio-driven accumulator; Chladni lobes use the speed-driven `ring_phase`:

```rust
// Ring pattern: audio-driven (meditative breathing)
let ring_crawl = 0.3 + self.audio_energy * 1.5;  // [0.3, 1.8] rad/s
self.ring_phase_visual += dt * ring_crawl;

// Chladni lobes: speed-driven (directional expression)
self.ring_phase += dt * crawl_rate * audio_mod;
```

This adds `ring_phase_visual: f32` to `OrganismState` and one extra float to the GPU uniform buffer. The shader uses `ring_phase_visual` for the concentric ring pattern (`sqrt(d)*5.0 - ring_phase_visual`) and `ring_phase` for Chladni mode animation.

## Smoothing Rates

### Current problem

Fixed-rate EMAs cannot serve both states well:
- **At rest / slow drift:** Low smoothing prevents jitter from physics noise. Good.
- **During slingshot / well escape:** Same low smoothing causes visual lag. The organism is already decelerating before the visual catches up. Bad.

### Proposed: speed-adaptive smoothing

Replace fixed-rate EMAs with rates that scale with current speed. Faster movement = faster visual tracking = more responsive animation. Slow movement = gentle smoothing = no jitter.

**smooth_speed:**

```rust
// Current:
self.smooth_speed += (actual_speed - self.smooth_speed) * (8.0 * dt).min(1.0);

// Proposed:
let speed_alpha = lerp(SMOOTH_SPEED_LO, SMOOTH_SPEED_HI, (actual_speed / SPEED_ADAPT_REF).min(1.0));
self.smooth_speed += (actual_speed - self.smooth_speed) * (speed_alpha * dt).min(1.0);
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `SMOOTH_SPEED_LO` | 6.0 | Smoothing rate when idle (Hz) — slightly slower than current 8.0 for calmer rest |
| `SMOOTH_SPEED_HI` | 20.0 | Smoothing rate at full speed (Hz) — 95% convergence in ~150ms |
| `SPEED_ADAPT_REF` | 100.0 | Speed at which smoothing is fully fast (px/s) |

At TBLK slingshot (200 px/s): rate = 20.0 Hz, convergence ~150ms. A 0.5s maneuver is clearly visible.
At DRON idle (5 px/s): rate = 6.7 Hz, convergence ~450ms. Gentle, no jitter.

**visual_dir:**

```rust
// Current:
let rate = (6.0 * dt).min(1.0);

// Proposed:
let dir_alpha = lerp(DIR_SMOOTH_LO, DIR_SMOOTH_HI, (actual_speed / SPEED_ADAPT_REF).min(1.0));
let rate = (dir_alpha * dt).min(1.0);
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `DIR_SMOOTH_LO` | 4.0 | Direction smoothing at rest (Hz) — slightly slower than current 6.0 |
| `DIR_SMOOTH_HI` | 14.0 | Direction smoothing at full speed (Hz) — 95% in ~215ms |

At TBLK slingshot: rate = 14.0 Hz, direction tracks in ~215ms. Sharp turns around wells visible.
At SPGL drift: rate = 4.8 Hz, direction smooths over ~625ms. No twitchy starfish.

**Implementation note:** `lerp(a, b, t)` is simply `a + (b - a) * t`. This is a free function, no allocation.

### scale_drift_blend

Current rate (0.4 Hz, ~7.5s convergence) is intentionally slow — it controls pitch drift into wells, which is a musical phenomenon that should feel gradual. **No change proposed.**

## Shader Changes

### speed_factor rescaling

**Current (biofield.wgsl:164):**
```wgsl
let speed_factor = min(speed / 50.0, 1.0);
```

Saturates at 50 px/s. A TBLK at 120 px/s has the same crawl_bias as one at 50 px/s.

**Proposed:**
```wgsl
let speed_factor = min(speed / SPEED_VISUAL_REF, 1.0);
```

Where `SPEED_VISUAL_REF` is a new uniform or a constant baked into the shader.

| Candidate value | Effect |
|-----------------|--------|
| 100.0 | Saturation at 100 px/s. ACID/TBLK at max_speed reach full extension. Slingshots above 100 are clamped. |
| 150.0 | Saturation at 150 px/s. Slingshots still produce growing visual response up to 150. Most species never saturate at normal max_speed. |
| Species max_speed | Per-organism normalization. Each species saturates at its own max. Requires passing max_speed as a per-cell uniform. |

**Recommendation:** 120.0 as a universal constant. This means:
- SPGL at max (40 px/s): speed_factor = 0.33 — visible movement but restrained.
- HOSO at max (60 px/s): speed_factor = 0.50 — moderate extension.
- DRON/KKIT at max (80 px/s): speed_factor = 0.67 — noticeable crawl.
- ACID at max (100 px/s): speed_factor = 0.83 — strong directional lobe.
- TBLK at max (120 px/s): speed_factor = 1.0 — full extension.
- Slingshot at 200 px/s: speed_factor = 1.0 — same as TBLK max (no further stretching, but crawl_rate is higher, so the animation is faster).

This makes speed_factor express species character — SPGL is always restrained, TBLK is always full-reach. See Q2 for the per-species alternative.

### crawl_bias amplitude

**Current:** `cos(theta - heading) * speed_factor * 0.6`

The 0.6 amplitude controls maximum directional lobe extension. At speed_factor=1.0, the forward-facing node extends 60% more, the backward-facing node retracts 60%.

**Proposed:** Increase to 0.75 to make directional intent more visible at high speeds.

```wgsl
let crawl_bias = cos(theta_i - heading_angle) * speed_factor * 0.75;
```

Effect on reach at full speed (speed_factor=1.0, amp=typical 0.5):
- Current: reach = r * (0.25 + 0.5 * chladni * dance + 0.3) = r * [0.25, 0.85] (forward node)
- Proposed: reach = r * (0.25 + 0.5 * chladni * dance + 0.375) = r * [0.25, 0.925] (forward node)

The forward lobe stretches visibly further. The organism looks like it's reaching toward its destination.

### Velocity-axis elongation (optional)

The `elongation` DNA parameter currently packs Chladni mode numbers (m.n encoding). However, a separate speed-dependent body stretch could sell high-velocity motion:

```wgsl
// After computing core body SDF
let stretch_factor = 1.0 + speed_factor * VELOCITY_STRETCH;
let stretch_dir = normalize(dir);
let stretched_delta = delta - stretch_dir * dot(delta, stretch_dir) * (1.0 - 1.0/stretch_factor);
var result = length(stretched_delta) - core_r;
```

Where `VELOCITY_STRETCH` might be 0.15 (at full speed, 15% elongation along velocity). This is flagged as Q3 — it may look mechanical rather than organic.

## Ring Phase Overflow Fix

### Problem

`ring_phase` is an `f32` that grows without bound. After extended runtime:

| Duration | Approximate phase | f32 mantissa bits for fractional part |
|----------|-------------------|--------------------------------------|
| 1 hour | ~3,600 | 11 bits (precision ~0.002) |
| 10 hours | ~36,000 | 8 bits (precision ~0.015) |
| 100 hours | ~360,000 | 4 bits (precision ~0.25) |

At 100 hours, the `sin(sqrt(d)*5.0 - ring_phase)` computation in the shader subtracts a large number (~360,000) from a small number (~0-15). The result has ~4 bits of precision — visible banding in the ring pattern.

### Fix

Wrap `ring_phase` at a large multiple of 2pi to reset precision without visual discontinuity.

```rust
const RING_PHASE_WRAP: f32 = 256.0 * std::f32::consts::TAU;  // ~1608.5

// After incrementing ring_phase:
self.ring_phase += dt * crawl_rate * audio_mod;
if self.ring_phase > RING_PHASE_WRAP {
    self.ring_phase -= RING_PHASE_WRAP;
}
```

**Why 256 * TAU?** Any multiple of 2pi would maintain continuity for the `sin`/`cos` consumers. 256*TAU (~1608) keeps the value small enough for good f32 precision (mantissa covers fractional part with ~10 bits = precision ~0.001) while being large enough that the wrap happens infrequently (every ~1608/crawl_rate seconds, minimum ~450s at max speed, ~9 hours at idle).

The `omega1 * crawl_phase` and `omega2 * crawl_phase` terms in the Chladni computation also consume `ring_phase`. Since omega values are irrational-ish (species-specific + per-instance jitter), the wrap point of 256*TAU is not commensurate with any omega, so the visual pattern does not repeat at the wrap boundary. The phase simply continues its pseudo-random walk across the Chladni mode space.

**Shader impact:** None. The shader receives the already-wrapped value via the GPU uniform buffer.

## Energy Swell Under High Dynamics

### Current

```rust
let energy_swell = 1.0 + org.audio_energy * 0.3;  // [1.0, 1.3]
```

Radius pulses up to 30% with audio energy. Speed has no effect on visual size.

### Proposed: speed-based swell component

```rust
let speed_norm = (org.smooth_speed / SPEED_SWELL_REF).min(1.0);
let speed_swell = speed_norm * SPEED_SWELL_AMOUNT;
let energy_swell = 1.0 + org.audio_energy * 0.3 + speed_swell;
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `SPEED_SWELL_REF` | 120.0 | Speed at which swell is maximized (px/s) |
| `SPEED_SWELL_AMOUNT` | 0.08 | Maximum speed-based radius increase (8%) |

Effect: A TBLK at full speed (120 px/s) would appear 8% larger than at rest. Combined with audio energy at maximum: `1.0 + 0.3 + 0.08 = 1.38` (38% swell). This is subtle — the organism inflates slightly when sprinting, deflates when resting. It complements the crawl_bias elongation (shape) with overall size expansion (volume).

**Concern:** This interacts with the SDF radius used for collision/proximity. If energy_swell is only used for the GPU payload radius (not physics), there is no gameplay concern. Currently, `energy_swell` is applied only in `build_gpu_payload()` (registry.rs:442), not in the physics tick. **Safe to add speed_swell here.**

## Constants Table

### Before / After Summary

| Constant | Location | Current | Proposed | Notes |
|----------|----------|---------|----------|-------|
| crawl_rate formula | sim.rs:318 | `(speed/50).clamp(0.05,2.0)` | `0.05 + 0.7*ln(1+speed/30)` | Log compression, soft ceiling |
| smooth_speed rate | sim.rs:293 | `8.0` (fixed) | `lerp(6.0, 20.0, speed/100)` | Speed-adaptive |
| visual_dir rate | sim.rs:280 | `6.0` (fixed) | `lerp(4.0, 14.0, speed/100)` | Speed-adaptive |
| speed_factor ref | biofield.wgsl:164 | `50.0` | `120.0` | Higher saturation threshold |
| crawl_bias amp | biofield.wgsl:182 | `0.6` | `0.75` | Stronger directional expression |
| ring_phase wrap | sim.rs:320 | none (unbounded) | `256 * TAU` | Precision maintenance |
| speed_swell | registry.rs:442 | none | `(speed/120).min(1) * 0.08` | Subtle speed-based inflation |
| scale_drift_blend | sim.rs:314 | `0.4` | `0.4` (unchanged) | Intentionally slow |
| energy_swell | registry.rs:442 | `audio * 0.3` | `audio * 0.3` (unchanged) | Audio coupling unchanged |

### Derived behavior comparison

| Scenario | Current crawl_rate | Proposed crawl_rate | Current speed_factor | Proposed speed_factor |
|----------|-------------------|--------------------|--------------------- |----------------------|
| SPGL idle (5 px/s) | 0.10 | 0.16 | 0.10 | 0.04 |
| DRON cruising (30 px/s) | 0.60 | 0.56 | 0.60 | 0.25 |
| HOSO at max (60 px/s) | 1.20 | 0.88 | 1.00 | 0.50 |
| ACID at max (100 px/s) | 2.00 | 1.30 | 1.00 | 0.83 |
| TBLK at max (120 px/s) | 2.00 | 1.42 | 1.00 | 1.00 |
| Slingshot (200 px/s) | 2.00 | 1.72 | 1.00 | 1.00 |
| Extreme burst (400 px/s) | 2.00 | 2.15 | 1.00 | 1.00 |

| Scenario | Current smooth_speed 95% | Proposed smooth_speed 95% | Current visual_dir 95% | Proposed visual_dir 95% |
|----------|--------------------------|---------------------------|-----------------------|------------------------|
| Idle (5 px/s) | 375ms | 450ms | 500ms | 750ms |
| Cruising (50 px/s) | 375ms | 250ms | 500ms | 330ms |
| Max speed (120 px/s) | 375ms | 150ms | 500ms | 215ms |
| Slingshot (200 px/s) | 375ms | 150ms | 500ms | 215ms |

**Key observation:** The proposed crawl_rate is actually *slower* than current at mid-to-high speeds (e.g., HOSO max: 0.88 vs 1.20). This is intentional — the current values are arguably too fast in the mid-range, making organisms look frantic. The log curve trades mid-range speed for continued expressiveness at high speeds. If this feels too sluggish, `CRAWL_GAIN` can be raised from 0.7 to 0.85.

## Critical Files

| File | Role | Changes |
|------|------|---------|
| `src/organism/sim.rs:293` | `smooth_speed` EMA | Speed-adaptive smoothing rate |
| `src/organism/sim.rs:280` | `visual_dir` EMA | Speed-adaptive smoothing rate |
| `src/organism/sim.rs:316-320` | `crawl_rate` + `ring_phase` | Log curve + phase wrapping |
| `src/renderer/biofield.wgsl:164` | `speed_factor` | Raise reference from 50 to 120 |
| `src/renderer/biofield.wgsl:182` | `crawl_bias` | Raise amplitude from 0.6 to 0.75 |
| `src/organism/registry.rs:442` | `energy_swell` | Add speed_swell component |

## Dependencies

- **Ships AFTER S36 (physics hardening).** No point tuning animation constants for speeds that are frame-rate-dependent. S36's fixed 120Hz timestep means visual smoothing uses frame dt (variable) while physics uses PHYS_DT (fixed). EMA rates here use frame dt directly — they adapt automatically.
- **Independent of S38 and S39.** This spec is about visual expression, not ecological mechanics. Can implement alongside well ecology/navigation reward without conflicts.
- **Note on dt source**: `smooth_speed` and `visual_dir` use frame dt (display rate), not physics dt. They smooth the *visual representation* of physics state, so they should track the display update cadence, not the physics substep rate.

## Verification

### Visual criteria (manual)

1. **Speed differentiation at high speeds.** Spawn a TBLK and push it to max_speed. Then trigger a slingshot at ~200 px/s. The slingshot should be visibly faster (more rapid Chladni cycling, slightly larger body). Currently these look identical.

2. **Responsive direction tracking.** Apply a sharp 90-degree force to a fast-moving organism. Its visual_dir should track the turn within ~200ms (2-3 frames at the new rate), not ~500ms (the current rate). At rest, the same organism should not twitch from physics noise.

3. **Speed-responsive tracking.** A slow SPGL (5 px/s) should visually smooth over ~450ms. A fast TBLK (120 px/s) should smooth over ~150ms. Both should feel natural — the SPGL is languid, the TBLK is snappy.

4. **No stroboscopic rings.** At maximum slingshot speed with full audio energy, the concentric ring pattern should still be readable (individual rings distinguishable). If rings become a blur, CRAWL_GAIN is too high or ring decoupling (Q1) is needed.

5. **Directional lobe at speed.** A TBLK at 120 px/s should have its forward Chladni lobe reaching noticeably further than its backward lobe (crawl_bias at 0.75 * 1.0 = 0.75). At 50 px/s, the effect should be less pronounced (0.75 * 0.42 = 0.31).

6. **Organic quality at rest.** With all changes applied, a resting organism (0 px/s) should look identical to current behavior: gentle Chladni breathing at 0.05 rad/s, no directional bias, no speed swell. The changes should be invisible at rest.

7. **Phase wrap invisibility.** Run the engine for long enough to trigger a ring_phase wrap (or artificially set ring_phase near the wrap point). Verify no visual discontinuity — ring pattern should be continuous across the wrap.

### Automated tests (in sim.rs)

8. **Crawl rate curve monotonicity.** For speeds [0, 10, 50, 100, 200, 500], verify crawl_rate is strictly increasing.

9. **Crawl rate idle minimum.** At speed=0, crawl_rate = 0.05 (unchanged from current).

10. **Ring phase wrapping.** Set ring_phase to `256 * TAU + 1.0`, tick once. Verify ring_phase is now near 1.0 + increment.

11. **Adaptive smoothing range.** At speed=0, verify smooth_speed rate is SMOOTH_SPEED_LO. At speed=200, verify rate is SMOOTH_SPEED_HI.

## Resolved Decisions

### Q1: Ring / Chladni decoupling → Audio-driven rings, speed-driven Chladni

Ring concentric pattern driven by audio energy (not speed). Chladni lobes driven by crawl_rate (speed). Decoupled.

```rust
// Ring phase: audio-driven
let ring_crawl = 0.3 + audio_energy * 1.5;  // [0.3, 1.8] rad/s
self.ring_phase_visual += dt * ring_crawl;

// Chladni phase: speed-driven (existing crawl_rate)
self.ring_phase += dt * crawl_rate * audio_mod;
```

Adds one float to GPU uniform buffer (`ring_phase_visual`). Rings breathe with audio energy, maintaining meditative quality even during slingshots. Chladni lobes respond to speed for directional expression.

### Q2: speed_factor reference → Universal 120.0 (tunable)

Start with `SPEED_VISUAL_REF = 120.0` as a constant. Species-specific visual energy levels emerge naturally (SPGL restrained at 33%, TBLK full at 100%). Can be made per-species later if needed.

### Q3: Body elongation → No stretch, amplified crawl_bias only

Crawl_bias amplitude raised from 0.6 to 0.75. This creates organic directional extension via the Chladni lobe asymmetry — forward lobes reach, rear lobes retract. No separate body-stretch mechanism needed. Avoids mechanical capsule look.

### Q4: CRAWL_GAIN → 0.7 default (tunable)

Start with `CRAWL_GAIN = 0.7`. Mid-range is intentionally slower than current (HOSO max: 0.88 vs 1.20) to trade mid-range speed for high-speed expressiveness. This is an aesthetic knob — tune perceptually after implementation. Range to explore: 0.55 to 1.0.

### Q5: speed_swell blend kernel → Accept side effect

At 8% swell, the blend kernel increase is negligible. Accept the "puffing up" side effect — it reads as a natural part of the speed expression. Revisit only if Chladni crispness is noticeably degraded.
