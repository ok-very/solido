# S39 — Navigation Reward

**Status**: Ready (decisions made, thresholds tunable)
**Depends on**: S36 (physics hardening). Self-contained — does not require S38 (well ecology) to ship first.
**Blocks**: None directly (feeds into organism personality/expressiveness)

---

## Goal

Organisms receive valence rewards and penalties based on how they navigate the gravity well landscape. Successful slingshots, well-to-well transitions, and exploratory departures produce dopamine (positive valence delta). Trapping produces gradually increasing distress (negative valence delta). Navigation becomes a visible emotional event — organisms that move purposefully through the harmonic field are visibly happier than those stuck in a rut.

---

## Context

### What exists today

Gravity wells pull organisms via quadratic attraction scaled by `scale_affinity * consonance * 12.0` (see `app.rs:1108`). The pull modifies velocity but nothing detects or responds to the *outcome* of that pull. Valence is driven exclusively by homeostatic throughput (`ModuleEmotion::update()` in `emotion.rs`) — signal delivery rate vs target activity, plus error fraction. An organism that brilliantly surfs between wells gets the same emotional state as one that sits motionless.

### What's missing

1. **No trajectory awareness.** Nothing detects "arrived at a well," "departed a well," or "trapped in a well." The physics pulls, but there is no event layer.
2. **Valence ignores navigation.** Homeostatic throughput is the only valence driver. Spatial behavior has zero emotional impact.
3. **No slingshot reward.** The user's vision: organisms get dopamine from successful gravity assists — approaching, matching pitch, departing with momentum. Getting trapped feels bad.

---

## Architecture

### New struct: `WellTracker` (per-organism)

Tracks each organism's relationship with each gravity well. Lives alongside `OrganismState` in the registry, not inside the organism itself (organisms don't know about wells — the registry/app layer does).

```rust
/// Per-organism navigation state for a single gravity well.
#[derive(Debug, Clone)]
pub struct WellVisit {
    pub well_id: u32,
    /// Tick count when organism entered this well's influence radius.
    pub entry_tick: u64,
    /// Organism speed at the moment of entry.
    pub entry_speed: f32,
    /// Minimum distance to well center achieved during this visit.
    pub min_distance: f32,
    /// Current distance to well center (updated each tick while inside).
    pub current_distance: f32,
    /// Consonance between organism root and well root (cached at entry).
    pub consonance: f32,
}

/// Per-organism navigation tracker across all wells.
#[derive(Debug, Clone)]
pub struct WellTracker {
    /// Active visits: organism is currently inside this well's radius.
    /// Key: well_id. Typically 0-2 entries (organism rarely overlaps >2 wells).
    pub active_visits: HashMap<u32, WellVisit>,

    /// Last well departed (id + tick). Used to detect transitions.
    pub last_departure: Option<(u32, u64)>,
    /// Speed at last departure. Used for slingshot detection.
    pub last_departure_speed: f32,

    /// Accumulated navigation valence delta this tick (reset each frame).
    pub nav_valence_delta: f32,

    /// Trapping accumulator per well [0, 1]. Grows while trapped, decays otherwise.
    pub trap_stress: HashMap<u32, f32>,
}
```

### Where it lives

```rust
pub struct OrganismRegistry {
    organisms: Vec<OrganismState>,
    // ... existing fields ...

    // NEW: per-organism well navigation tracking
    well_trackers: HashMap<OrganismId, WellTracker>,
}
```

Alternatively, `WellTracker` could be a field on `OrganismState` directly. See Open Question 2.

---

## Event Definitions

All events are detected in the gravity well dispatch loop in `app.rs` (the same loop that currently computes `effective_weights` and applies steering forces). Detection runs once per frame.

### E1: Well Arrival

| Property | Value |
|----------|-------|
| **Trigger** | Organism distance to well center crosses from `>= radius` to `< radius` |
| **Valence delta** | `+0.05 * consonance * scale_affinity` |
| **Rationale** | Small positive bump — "I found something interesting." Consonant wells feel better. |
| **Side effect** | Creates a `WellVisit` entry in `active_visits` |

### E2: Well Departure

| Property | Value |
|----------|-------|
| **Trigger** | Organism distance to well center crosses from `< radius` to `>= radius` AND organism speed `>= departure_speed_threshold` |
| **Valence delta** | `+0.10 * consonance * scale_affinity * speed_factor` |
| **`speed_factor`** | `(exit_speed / max_speed).clamp(0.1, 1.0)` |
| **`departure_speed_threshold`** | See Open Question 3 |
| **Rationale** | Leaving a well with velocity means the organism chose to move on, not just drifted. Higher speed = more deliberate departure. |
| **Side effect** | Records `last_departure = (well_id, current_tick)` and `last_departure_speed = exit_speed`. Removes `WellVisit` from `active_visits`. Resets `trap_stress` for this well. |

### E3: Passive Exit (drift-out)

| Property | Value |
|----------|-------|
| **Trigger** | Organism distance crosses from `< radius` to `>= radius` AND speed `< departure_speed_threshold` |
| **Valence delta** | `0.0` (neutral — neither reward nor punishment) |
| **Rationale** | Organism drifted out without intent. No reward, but no penalty either. |
| **Side effect** | Same bookkeeping as E2 but no valence change. |

### E4: Slingshot

| Property | Value |
|----------|-------|
| **Trigger** | Well Departure (E2) fires AND `exit_speed >= entry_speed * slingshot_ratio` AND `min_distance < radius * depth_threshold` |
| **`slingshot_ratio`** | See Open Question 4 |
| **`depth_threshold`** | See Open Question 5 |
| **Valence delta** | `+0.20 * consonance * scale_affinity * speed_gain_factor` |
| **`speed_gain_factor`** | `((exit_speed / entry_speed) - 1.0).clamp(0.0, 1.0)` |
| **Rationale** | The organism dove deep into the well and emerged faster. This is the gold-star navigation event — maximum dopamine. The deeper the dive and the greater the speed gain, the better it feels. |
| **Side effect** | Slingshot replaces the normal departure delta (does not stack with E2). |

### E5: Trapping

| Property | Value |
|----------|-------|
| **Trigger** | Organism has been inside a well for `>= trap_onset_ticks` AND current speed `< trap_speed_threshold` |
| **Valence delta** | `-trap_rate * scale_affinity` per tick (continuous, not one-shot) |
| **`trap_onset_ticks`** | See Open Question 6 |
| **`trap_speed_threshold`** | See Open Question 7 |
| **`trap_rate`** | See Open Question 8 |
| **Rationale** | Gradual discomfort, not instant punishment. The organism slowly realizes it is stuck. Low arousal initially (bored), then arousal rises (agitated). |
| **Side effect** | `trap_stress` for this well accumulates. When `trap_stress > 0.5`, arousal boost `+= trap_stress * 0.3` (drives exploration behavior — the organism tries to escape). |

### E6: Transition

| Property | Value |
|----------|-------|
| **Trigger** | Well Arrival (E1) fires AND `last_departure` exists AND `(current_tick - last_departure_tick) < transition_window_ticks` AND `last_departure_well_id != arriving_well_id` |
| **`transition_window_ticks`** | See Open Question 9 |
| **Valence delta** | `+0.15 * avg_consonance * scale_affinity * directness_factor` |
| **`avg_consonance`** | Average of departure well consonance and arrival well consonance |
| **`directness_factor`** | `(1.0 - elapsed_ticks / transition_window_ticks).max(0.3)` — faster transitions are more rewarding |
| **Rationale** | Deliberate movement between wells is the highest-quality navigation. The organism traversed the landscape with purpose. Fast transitions between consonant wells are peak dopamine. |
| **Side effect** | Stacks with E1 (arrival delta + transition delta both apply). |

### Event priority and stacking

- E1 (arrival) always fires when entering a well.
- E6 (transition) stacks on top of E1 if conditions are met.
- E4 (slingshot) replaces E2 (departure) — it is a strictly better departure.
- E3 (passive exit) replaces E2 when speed is below threshold.
- E5 (trapping) is continuous and independent of discrete events.

---

## Reward Integration with Existing Emotion

### Additive modulation

Navigation reward is additive to existing valence, applied as a post-update adjustment:

```rust
impl ModuleEmotion {
    /// Apply navigation valence after the normal homeostatic update.
    /// `nav_delta` is the accumulated navigation reward this tick.
    /// `nav_weight` controls how much navigation matters vs homeostasis.
    pub fn apply_navigation_reward(&mut self, nav_delta: f32, nav_weight: f32) {
        self.valence = (self.valence + nav_delta * nav_weight).clamp(-1.0, 1.0);
    }
}
```

### Weight parameter

`nav_weight` controls the balance between homeostatic valence and navigation valence. See Open Question 10 for the default value.

### Arousal modulation from trapping

Trapping stress directly boosts arousal (independent of the normal surprise-based arousal):

```rust
pub fn apply_trap_arousal(&mut self, trap_stress: f32) {
    let arousal_boost = trap_stress * 0.3;
    self.arousal = (self.arousal + arousal_boost).clamp(0.0, 1.0);
}
```

This is intentional: a trapped organism becomes agitated (high arousal) which drives the affinity graph's exploration behavior — it tries new connections, which may produce forces that help it escape. The loop closes: trapping -> stress -> arousal -> exploration -> new edges -> new forces -> possible escape -> departure reward -> valence recovery.

### Flow diagram

```
             Gravity well physics (existing)
                       |
                       v
    WellTracker detects trajectory events (new)
                       |
                       v
    Events produce nav_valence_delta (new)
                       |
                       v
    ModuleEmotion.apply_navigation_reward() (new)
                       |
                       v
    Valence feeds Hebbian reward (existing)
                       |
                       v
    Arousal drives exploration (existing)
```

---

## Species-Dependent Sensitivity

`scale_affinity` gates ALL navigation reward magnitudes. This produces natural species differentiation:

| Species | `scale_affinity` (DNA) | Navigation reward strength | Behavior |
|---------|----------------------|---------------------------|----------|
| KKIT | 0.0 | **Zero** — completely immune | Ignores wells entirely. No events fire (the `scale_affinity < 0.01` guard in the force loop already skips KKIT). |
| DRON | 0.3 | **Weak** | Barely notices wells. Being "trapped" barely registers. Slow, ambient drift is fine for a drone. |
| TBLK | 0.2 | **Very weak** | Tabla has slight pitch awareness but is primarily rhythmic. Wells are background flavor. |
| ACID | 0.7 | **Strong** | Acid lines surf wells aggressively. Slingshots are exciting. Trapping is uncomfortable. |
| HOSO | 0.8 | **Very strong** | Hosono is deeply pitch-aware. Well navigation is a primary emotional driver. |
| SPGL | 0.9 | **Dominant** | Sparkle is the most scale-sensitive. Navigation events are peak emotional experiences. |

### KKIT exemption

KKIT (`scale_affinity = 0.0`) gets no `WellTracker` allocated. The early-out `if scale_affinity < 0.01 { continue; }` in the gravity dispatch loop (already in `app.rs:1078`) ensures zero overhead. Future rhythm wells (if added) would use `rhythm_affinity` instead — a completely separate system.

---

## Per-Organism State Budget

Each `WellTracker` contains:

| Field | Size | Max entries |
|-------|------|-------------|
| `active_visits` | ~48 bytes per entry | 1-2 (rarely >2 overlapping wells) |
| `last_departure` | 12 bytes | 1 |
| `last_departure_speed` | 4 bytes | 1 |
| `nav_valence_delta` | 4 bytes | 1 |
| `trap_stress` | ~12 bytes per entry | 1-2 |

Total per organism: ~100-200 bytes. With 6 organisms max, this is negligible (~1 KB total).

---

## Tick Integration Point

The navigation reward computation slots into the existing gravity well dispatch loop in `app.rs`. The current structure is:

```
Phase 1: Collect (mod_id, org_id, pos, scale_affinity, fidelity) into well_dispatch_buf
Phase 2: Compute effective_weights, dispatch to reactor
Phase 3: Compute gravity steering forces, apply to organisms
```

Navigation reward adds a Phase 3b:

```
Phase 3b: For each organism in well_dispatch_buf:
    For each well:
        Compute distance
        Detect entry/exit transitions (compare against WellTracker state)
        Fire events, accumulate nav_valence_delta
    Apply nav_valence_delta to ModuleEmotion via reactor
    Reset nav_valence_delta for next frame
```

This runs at frame rate (60Hz), same as the steering forces. Event detection is pure distance comparison — no allocation, no complex state machine.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/tuning/gravity_well.rs` | Add `WellTracker`, `WellVisit` structs, event detection logic |
| `src/organism/registry.rs` | Add `well_trackers: HashMap<OrganismId, WellTracker>`, lifecycle (spawn/despawn cleanup) |
| `src/affinity/emotion.rs` | Add `apply_navigation_reward()`, `apply_trap_arousal()` methods |
| `src/app.rs` | Phase 3b in gravity dispatch loop: distance tracking, event detection, reward dispatch |
| `src/organism/sim.rs` | No changes (navigation reward flows through ModuleEmotion, not OrganismState directly) |
| `src/organism/dna.rs` | No changes (`scale_affinity` already exists and is sufficient) |

---

## Dependencies

```
S36 (Physics Hardening)
    |
    ├──→ S37 (Animation)
    ├──→ S38 (Well Ecology)
    └──→ S39 (Navigation Reward) <-- THIS SPEC (self-contained)
```

### Self-contained design (decided)

This spec defines its own minimal `WellTracker` with distance tracking. The distance tracking needed here is trivial (just `dist < radius` comparison) and does not require S38's well population dynamics or well energy model. If S38 later provides richer proximity state, S39 can migrate to use it.

---

## Verification

### Unit tests (in `gravity_well.rs` or a new `navigation_reward.rs`)

1. **Arrival detection**: Organism at `radius + 10` moves to `radius - 10` — E1 fires, `nav_valence_delta > 0`.
2. **Departure detection**: Organism at `radius - 10` moves to `radius + 10` with speed > threshold — E2 fires, `nav_valence_delta > 0`.
3. **Passive exit**: Same as above but speed < threshold — E3 fires, `nav_valence_delta == 0`.
4. **Slingshot**: Entry speed 20, exit speed 30, min_distance < depth_threshold — E4 fires, delta > E2 delta.
5. **Trapping**: Organism inside well for N ticks with speed < threshold — E5 fires, `nav_valence_delta < 0`, increasing magnitude each tick.
6. **Transition**: Depart well A, arrive well B within window — E6 fires, `nav_valence_delta > E1_alone`.
7. **KKIT immunity**: Organism with `scale_affinity = 0.0` — no events fire, no WellTracker allocated.
8. **Species scaling**: Same event for DRON (0.3) and HOSO (0.8) — HOSO delta is `0.8/0.3 = 2.67x` larger.
9. **Valence clamping**: Extreme navigation reward does not push valence outside `[-1, 1]`.
10. **Trap stress decay**: Organism escapes a well — `trap_stress` for that well decays toward zero.
11. **Multiple wells**: Organism inside two overlapping wells — events fire independently for each.

### Integration tests (manual, visual)

1. Spawn HOSO near a well. Watch it approach. Valence should tick upward on arrival.
2. Apply a force to push HOSO through a well and out the other side. Valence should spike on slingshot.
3. Let HOSO get trapped in a well center (remove wander thrust temporarily). Valence should drift negative over ~5 seconds, then arousal should rise.
4. Spawn KKIT. Verify it has no emotional response to wells whatsoever.
5. Place two consonant wells (e.g., C and G) and watch an ACID organism transition between them. Transition event should produce visible valence spike.

---

## Resolved Decisions

### OQ1: Dependency → Self-contained (Option B)

S39 defines its own `WellTracker` with distance tracking. Does not require S38. See Dependencies section above.

### OQ2: WellTracker location → Registry-owned (Option A)

`HashMap<OrganismId, WellTracker>` on `OrganismRegistry`. Consistent with pairwise_affinities and pairwise_attachments.

### OQ3-OQ12: Thresholds → Sensible defaults, all tunable

All numeric thresholds are implemented as named constants (not magic numbers) so they can be tuned perceptually after implementation. Defaults chosen to be moderate — not too aggressive, not too gentle.

| Parameter | Default | Constant name | Rationale |
|-----------|---------|---------------|-----------|
| Departure speed threshold | `max_speed * 0.1` | `DEPARTURE_SPEED_FRAC` | Species-relative. ACID's "slow" is faster than DRON's. |
| Slingshot speed ratio | `1.2` | `SLINGSHOT_SPEED_RATIO` | 20% speed gain — frequent enough to feel rewarding, rare enough to be exciting. |
| Slingshot depth threshold | `radius * 0.5` | `SLINGSHOT_DEPTH_FRAC` | Halfway to center. Deep enough to be meaningful, achievable enough to occur. |
| Trap onset | `300` ticks (~2.5s at 120Hz) | `TRAP_ONSET_TICKS` | Moderate. DRON's low scale_affinity attenuates naturally. |
| Trap speed threshold | `max_speed * 0.05` | `TRAP_SPEED_FRAC` | Species-relative. Distinguishes "orbiting" from "stuck at center." |
| Trap stress rate | `0.002` per tick | `TRAP_STRESS_RATE` | ~8s to -1.0 from neutral (before scale_affinity). Gradual discomfort, not cliff. |
| Transition window | `600` ticks (~5s at 120Hz) | `TRANSITION_WINDOW_TICKS` | Covers most direct transits at typical speeds. |
| Navigation weight | `0.5` | `NAV_WEIGHT` | Navigation matters as much as homeostatic throughput. |
| Trap stress decay | `*= 0.97` per tick | `TRAP_STRESS_DECAY` | Exponential decay, ~2.5s to near-zero. Organism carries brief "trauma." |
| Slingshot consonance | Same table | — | Use existing consonance_weight(). No separate slingshot table. |

**Note on tick rates**: With S36's 120Hz fixed timestep, tick counts are doubled compared to the original spec (which assumed 60Hz). All tick-based constants above are calibrated for 120Hz.
