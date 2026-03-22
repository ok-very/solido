# Navigation Reward Rewrite — Substrate Richness

**Status**: Spec (rewrite of S39 Navigation Reward)
**Depends on**: well-lens-rewrite, block-grid-vision.md, substrate-encoding.md
**Blocks**: organism-satisfaction rewrite

---

## What Changes

**Before (S39)**: WellTracker per organism tracks 6 trajectory events (arrival, departure, slingshot, trapping, transition, passive exit). Events are defined by crossing well radius boundaries at various speeds. Valence/arousal deltas reward exploration and penalize stasis.

**After**: Reward comes from substrate quality, not well geography. The 6 event types map to substrate conditions rather than well boundaries. Organisms are rewarded for finding rich substrate, penalized for staying in depleted areas. Movement through diverse substrate is rewarded. Stasis in uniform or depleted substrate is penalized.

---

## Event Reinterpretation

### 1. Discovery (was: Arrival)

**Before**: Enter well radius → +0.05 × consonance

**After**: Move into a grid region where energy is significantly higher than where you were:

```rust
let energy_delta = current_local_energy - previous_local_energy;
if energy_delta > DISCOVERY_THRESHOLD {  // 0.15
    valence += energy_delta * 0.1;
    arousal -= 0.02;  // Slight calming — found food
}
```

Organism discovers a rich patch of substrate. Positive valence, reduced arousal (relief).

### 2. Departure Boost (was: Departure)

**Before**: Exit well at speed > threshold → +0.10 × speed_factor

**After**: Leave a region at speed after feeding (local energy depleted below mean):

```rust
let local_energy = substrate_grid.sample_energy(pos);
let was_depleted = local_energy < DEPARTURE_THRESHOLD;  // 0.3
let speed = velocity.length();
if was_depleted && speed > 20.0 {
    valence += 0.08;
    arousal += 0.05;  // Energized departure — seeking new ground
}
```

Organism exhausted the local substrate and moves on. Rewarded for not clinging to depleted ground.

### 3. Grazing Run (was: Slingshot)

**Before**: Deep well entry + exit at >1.2× entry speed → +0.20 × speed_gain

**After**: Sustained movement through energy-rich substrate (eating while moving):

```rust
// Track rolling window of energy consumed while speed > threshold
if speed > 15.0 && consumed_this_tick > 0.01 {
    grazing_run_timer += dt;
    grazing_run_energy += consumed_this_tick;
}
if grazing_run_timer > 1.0 {  // 1 second of sustained grazing while moving
    valence += 0.15 * (grazing_run_energy / grazing_run_timer).min(1.0);
    grazing_run_timer = 0.0;
    grazing_run_energy = 0.0;
}
```

Rewards nomadic grazing — moving through rich substrate is better than sitting still. The "slingshot" energy comes from covering ground.

### 4. Starvation Pressure (was: Trapping)

**Before**: Stuck slow for >150 frames in well → −0.002/frame

**After**: Stuck in depleted substrate (low energy AND low speed):

```rust
let local_energy = substrate_grid.sample_energy(pos);
if local_energy < STARVATION_THRESHOLD && speed < 10.0 {  // 0.2, 10 px/s
    starvation_timer += dt;
    if starvation_timer > 1.0 {
        valence -= 0.003;  // Growing discomfort
        arousal += 0.005;  // Growing urgency to move
    }
} else {
    starvation_timer = (starvation_timer - dt * 2.0).max(0.0);  // Recover faster than accumulate
}
```

Sitting in darkness = bad. Arousal builds, eventually triggering the existing wanderlust system.

### 5. Substrate Transition (was: Well Transition)

**Before**: Well-to-well within 5 seconds → +0.15 × directness

**After**: Significant change in consumed pitch class within a time window:

```rust
// Track dominant pitch class over last 60 frames
let prev_dominant = pitch_history.mode(30..60);  // older half
let curr_dominant = pitch_history.mode(0..30);   // recent half
if prev_dominant != curr_dominant {
    // Organism transitioned between pitch regions
    valence += 0.12;
    transition_cooldown = 3.0;  // 3 second cooldown
}
```

Rewards harmonic travel — moving through different pitch regions of the substrate. The musical equivalent of exploring new territory.

### 6. Drift (was: Passive Exit)

**Before**: Slow exit from well → no delta

**After**: Slow movement through uniform substrate (no energy gradient):

```rust
let gradient = substrate_grid.energy_gradient(pos);
if gradient.length() < 0.01 && speed < 10.0 {
    // Drifting in flat substrate — neither rewarded nor penalized
    // But monotony timer ticks (existing wanderlust system handles this)
}
```

No explicit valence/arousal change. The existing wanderlust system (15s of satiation + low arousal → half nutrients + spike arousal) handles the exit from monotony.

---

## New OrganismState Fields

```rust
// Replace WellTracker with SubstrateTracker
pub previous_local_energy: f32,      // For discovery detection
pub grazing_run_timer: f32,          // Sustained moving+eating timer
pub grazing_run_energy: f32,         // Energy consumed during grazing run
pub starvation_timer: f32,           // Time stuck in depleted substrate
pub pitch_history: [u8; 60],         // Rolling pitch class history (1 per frame)
pub pitch_history_pos: usize,        // Ring buffer position
pub transition_cooldown: f32,        // Prevent rapid transition spam
```

---

## What Gets Removed

- `WellTracker` struct and all 6 well-specific event types
- `well_proximity` checks for navigation events
- `WellEvent` enum (Arrival, Departure, Slingshot, Trapping, Transition, PassiveExit)
- Consonance-weighted arrival reward

## What Stays

- Valence/arousal modulation (same hooks, different triggers)
- Wanderlust pulse (unchanged — still triggers on 15s satiation + low arousal)
- Navigation reward constants (ARRIVAL_BONUS etc. renamed but similar magnitudes)
- The principle: reward exploration, penalize stasis

---

## Critical Files

| File | Change |
|------|--------|
| `src/tuning/gravity_well.rs` | Remove WellTracker, keep energy state machine for lens |
| `src/organism/sim.rs` | Add SubstrateTracker fields, remove WellTracker |
| `src/app.rs` | Replace navigate_events() with substrate reward computation |
| `src/substrate/energy_grid.rs` | Add `energy_gradient()` method |
| `src/affinity/emotion.rs` | Valence/arousal deltas from substrate events |

---

## Verification

1. Organism moves from dark to bright substrate → positive valence (discovery)
2. Organism depletes local area, moves on → departure boost
3. Sustained movement through rich substrate → grazing run reward (visible as positive valence trend)
4. Organism stuck in depleted area → arousal climbs → wanderlust eventually fires
5. Organism crosses from red to green substrate → pitch transition reward
6. No well-specific behavior — organisms don't "know" wells exist, they just see energy
