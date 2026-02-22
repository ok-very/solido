# S07 — Affinity Graph Core

> Connections form, strengthen, weaken, and die. The system learns.

## Goal

Build the Hebbian learning graph that drives all routing decisions.
Edges between modules evolve based on delivery success, homeostatic
emotion, and arousal-driven exploration. No audio or rendering yet —
pure data structure with a tick loop.

## Ancestry

The blob_affinity_implementation_plan.md defines this entire system.
This session implements Phases 1-2 of that plan: Typed Contract Core +
Affinity Graph learning layer.

## Depends On

Nothing. This is a standalone data layer. Can be unit-tested in isolation.

## Tasks

### 7.1 Create `src/affinity/signal.rs`

```rust
pub enum Signal {
    Float(f32),
    Bool(bool),
    // AudioBlock and Embedding deferred to later sessions
}

impl Signal {
    pub fn matches_type(&self, port_type: &str) -> bool;
    pub fn magnitude(&self) -> f32;
}
```

Start with Float and Bool only — enough for pitch, gravity, triggers.

### 7.2 Create `src/affinity/edge.rs`

```rust
pub struct EdgeAffinity {
    pub weight: f32,          // [0, 1] routing strength
    pub eligibility: f32,     // trace: did this edge fire recently?
    pub goodput: f32,         // EWMA: fraction of valid deliveries
    pub impact: f32,          // EWMA: downstream signal magnitude
    pub age_blocks: u64,
}
```

Methods:
- `tick_decay()` — weight drifts toward 0.5, eligibility decays
- `on_delivery(type_valid, magnitude)` — update goodput/impact/eligibility
- `apply_reward(valence)` — Hebbian: `δw = LR × eligibility × valence × goodput`

Constants (from blob_affinity plan):
- DECAY: 0.001
- LR: 0.02
- ELIG_DECAY: 0.85
- EWMA_ALPHA: 0.05

### 7.3 Create `src/affinity/emotion.rs`

```rust
pub struct ModuleEmotion {
    pub valence: f32,          // [-1, 1] happy/unhappy
    pub arousal: f32,          // [0, 1] bored/overstimulated
    pub activity: f32,         // EWMA throughput
    pub target_activity: f32,  // homeostatic setpoint
    pub error_rate: f32,       // EWMA error fraction
}
```

Methods:
- `update(signals_this_block, errors_this_block)`
- `homeostatic_gain() -> f32` — amplify when starved, suppress when overdriven

Key formula:
```
valence = -(homeostatic_error²) - error_rate * 2.0
arousal = EWMA(surprise / (activity + 1))
```

### 7.4 Create `src/affinity/graph.rs`

```rust
pub struct AffinityGraph {
    pub edges: HashMap<EdgeId, EdgeAffinity>,
    pub emotions: HashMap<ModuleId, ModuleEmotion>,
    rng: Xoshiro256StarStar,
}
```

Methods:
- `tick(deliveries, module_events)` — the full update cycle:
  1. Update module emotions
  2. All edges decay
  3. Record deliveries with homeostatic gain
  4. Reward-modulated Hebbian update
  5. Softmax normalize per output port
- `maybe_explore(module_id, candidates)` — arousal-gated new edges
- `prune_weak_edges(min_age, threshold)` — remove stale weak edges
- `routing_weights(candidates) -> Vec<f32>` — softmax routing

### 7.5 Create `src/affinity/ledger.rs`

```rust
pub struct LedgerEvent {
    pub tick: u64,
    pub edge_id: EdgeId,
    pub event_type: LedgerEventType,
    pub weight_before: f32,
    pub weight_after: f32,
    pub reason: LedgerReason,
}

pub struct LedgerRingBuffer {
    events: VecDeque<LedgerEvent>,
    capacity: usize,  // 1000
}
```

Every weight change writes a LedgerEvent. The ledger is the
explainability spine — it answers "why is this connection strong?"

### 7.6 Add dependencies

```toml
rand = "0.8"
rand_xoshiro = "0.6"
```

### 7.7 Unit tests

Test the full tick cycle:
1. Create 3 modules, 4 edges
2. Run 100 ticks with random deliveries
3. Assert: weights stay in [0,1]
4. Assert: edges with good deliveries strengthen
5. Assert: edges with bad deliveries weaken
6. Assert: pruning removes old weak edges
7. Assert: emotions respond to activity levels
8. Assert: homeostatic gain amplifies starved modules
9. Assert: ledger captures all weight changes
10. Assert: softmax normalization caps total outflow

## Files Created

```
src/affinity/mod.rs     — pub mod signal, edge, emotion, graph, ledger;
src/affinity/signal.rs  — Signal enum
src/affinity/edge.rs    — EdgeAffinity
src/affinity/emotion.rs — ModuleEmotion
src/affinity/graph.rs   — AffinityGraph
src/affinity/ledger.rs  — LedgerEvent, LedgerRingBuffer
```

## Files Modified

```
src/main.rs             — add `mod affinity;`
Cargo.toml              — add rand, rand_xoshiro
```

## Verification

1. `cargo test` — all 10+ unit tests pass
2. Run a 1000-tick simulation, dump weights → verify convergence
3. Good edges (type-valid, high magnitude, positive valence): weight → ~0.8+
4. Bad edges (type-invalid): weight → ~0.1, eventually pruned
5. Module with zero input: arousal rises, explore triggers new edges
6. Ledger query: can trace any edge's full weight history

## Design Notes

This session has zero interaction with audio or rendering.
The affinity graph is a pure data layer that will be wired into
both the audio system (routing decisions) and the visual system
(blob thermal/pulse) in S08.

The graph runs on the main thread at UI frame rate (~60Hz).
This is much slower than the block-rate described in the plan,
but sufficient for a first implementation. It can be moved to a
dedicated control thread later if needed.
