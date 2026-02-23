# L1-S02 — Routing Backbone

> Connections form, strengthen, weaken, and die. The system learns.

## Status: IMPLEMENTED

Completed with audit fixes applied. See `spec/audits/L1-S02-audit.md`.

## Goal

Build the AffinityGraph and SeedReactor that drive all routing decisions.
Edges between modules evolve based on delivery success, homeostatic
emotion, and arousal-driven exploration. This is the central nervous
system — every module added after this session routes through it.

This is blob_affinity_implementation_plan.md Phases 1-2, built SECOND
(right after Module contract) so no module ever does "direct parameter
passing" that needs refactoring later.

## Ancestry

The Max/MSP patch had hardwired signal paths: `cycle~` → `tapin~` →
`dac~`. The affinity graph replaces hardwiring with learned connections.
Signals find their own paths.

## Depends On

- L0-S01 (ModuleCore trait, Signal types, PortId, port schemas)

## Implemented

### 2.1 `src/affinity/edge.rs` — EdgeAffinity

```rust
pub type EdgeId = (ModuleId, PortId, ModuleId, PortId);

pub struct EdgeAffinity {
    pub weight: f32,          // [0, 1] routing strength
    pub eligibility: f32,     // trace: did this edge fire recently?
    pub goodput: f32,         // EWMA: fraction of valid deliveries
    pub impact: f32,          // EWMA: downstream signal magnitude
    pub age_blocks: u64,
}
```

Methods:
- `new()` — starts at weight=0.5, eligibility=0.0, goodput=1.0
- `tick_decay()` — weight drifts toward 0.5 (DECAY=0.001), eligibility decays (ELIG_DECAY=0.85)
- `on_delivery(type_valid, magnitude)` — EWMA update goodput/impact; eligibility spikes (1.0 for valid, 0.5 for invalid — so negative valence can weaken bad edges)
- `apply_reward(valence)` — Hebbian: `dw = LR * eligibility * valence * goodput`
- `should_prune(min_age, threshold)` — true if old and weak

Constants:
```rust
const DECAY: f32 = 0.001;
const LR: f32 = 0.02;
const ELIG_DECAY: f32 = 0.85;
const EWMA_ALPHA: f32 = 0.05;
```

### 2.2 `src/affinity/emotion.rs` — ModuleEmotion

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
- `new(target_activity)` — starts neutral (valence=0, arousal=0)
- `update(signals, errors)` — EWMA activity/error_rate, compute valence and arousal
- `homeostatic_gain()` — multiplier: >1.0 when starved, <1.0 when flooded, clamped [0.1, 3.0]

Key formulas (updated from original spec during implementation):
```
homeostatic_error = (activity - target_activity) / (target_activity + 1.0)
valence = 1.0 - homeostatic_error^2 * 4.0 - error_rate * 2.0   // clamped [-1, 1]
arousal = EWMA(surprise / (activity + 1))                        // clamped [0, 1]
```

**Design note:** The original spec formula `valence = -(error^2) - error_rate * 2.0`
was always ≤ 0, preventing Hebbian strengthening. The revised formula starts at 1.0
(happy baseline) and penalizes deviation, allowing both positive reward (strengthening
good edges) and negative reward (weakening bad ones).

### 2.3 `src/affinity/graph.rs` — AffinityGraph

```rust
pub struct AffinityGraph {
    pub edges: HashMap<EdgeId, EdgeAffinity>,
    pub emotions: HashMap<ModuleId, ModuleEmotion>,
    pub ledger: LedgerRingBuffer,
    rng: Xoshiro256StarStar,
    tick_count: u64,
    pub topology_dirty: bool,  // only rebuild routing table on topology change
}
```

Methods:
- `new(seed)` — deterministic RNG for reproducible tests
- `register_module(id, target_activity)` — add emotion state
- `unregister_module(id)` — remove module + all its edges, sets `topology_dirty`
- `add_edge(edge_id)` — create edge if not duplicate, log to ledger, sets `topology_dirty`
- `tick(deliveries, module_stats)` — the full update cycle:
  1. Update module emotions from tick stats
  2. All edges decay (weight toward 0.5, eligibility fades)
  3. Record deliveries (update goodput, impact, eligibility)
  4. Reward-modulated Hebbian update (receiving module's valence × edge eligibility × goodput)
  5. Prune old weak edges (age > 1000 && weight < 0.1), sets `topology_dirty`
- `maybe_explore(module_id, schemas)` — arousal-gated (threshold 0.3, probability 0.1 per tick); finds type-compatible unconnected ports, creates random new edge
- `routing_weights_for_port(source, port)` — softmax-normalized weights per output port

Graph constants:
```rust
const EXPLORE_AROUSAL_THRESHOLD: f32 = 0.3;
const EXPLORE_PROBABILITY: f32 = 0.1;
const PRUNE_MIN_AGE: u64 = 1000;
const PRUNE_WEIGHT_THRESHOLD: f32 = 0.1;
```

### 2.4 `src/affinity/ledger.rs` — LedgerRingBuffer

```rust
pub enum LedgerEventType {
    Created, Strengthened, Weakened, Pruned, Explored,
}

pub enum LedgerReason {
    Delivery, Hebbian, Decay, Exploration, Manual,
}

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
    capacity: usize,  // default 1000
}
```

Methods:
- `record(tick, edge_id, event_type, weight_before, weight_after, reason)` — evicts oldest if at capacity
- `events()` — all events, oldest first
- `events_for_edge(edge_id)` — filter by edge
- `recent(n)` — most recent N events, newest first

Every weight change writes a LedgerEvent. The ledger is the
explainability spine — it answers "why is this connection strong?"

### 2.5 `src/reactor/mod.rs` — SeedReactor

The hub — every module registers with it:

```rust
pub struct SeedReactor {
    modules: HashMap<ModuleId, Box<dyn ModuleCore>>,
    pub graph: AffinityGraph,
    schemas: HashMap<ModuleId, ModuleSchema>,
    routing: RoutingTable,
    next_id: ModuleId,
    tick_count: u64,
    emit_buffer: Vec<(PortId, Signal)>,  // reusable, avoids per-tick allocation
}
```

Methods:
- `register(module)` → `ModuleId` — assigns ID, registers emotion (target_activity=1.0), auto-discovers compatible edges, rebuilds routing table
- `unregister(id)` — removes module, edges, rebuilds routing table
- `tick(dt)` — the full tick cycle (see below)
- `module_count()`, `edge_count()`, `tick_count()`, `schemas()`

**Auto-discovery on register:** When a module registers, the reactor
automatically creates edges between all type-compatible output→input
port pairs across all existing modules. No manual wiring needed.

### 2.6 `src/reactor/routing.rs` — RoutingTable

```rust
pub struct Delivery {
    pub target_module: ModuleId,
    pub target_port: PortId,
    pub target_type: SignalType,
    pub signal: Signal,
}

pub struct RoutingTable {
    routes: HashMap<(ModuleId, PortId), Vec<(ModuleId, PortId, SignalType, f32)>>,
}
```

Methods:
- `rebuild(graph, schemas)` — rebuild from current graph topology + softmax weights
- `route(source, port, signal)` → `Vec<Delivery>` — multi-cast to all targets

**Routing is multi-cast, not probabilistic.** Every connected target
receives the full signal (cloned via Arc for heap-heavy variants).
Softmax weights in the AffinityGraph determine which edges
strengthen/weaken through Hebbian learning — they do NOT scale signal
amplitude. All compatible modules "hear" every signal; the learning
system decides which connections to keep.

**Routing table rebuilds only on topology change.** The graph sets
`topology_dirty = true` when edges are added, removed, or pruned.
The reactor checks this flag each tick and only rebuilds when needed.
Weight-only changes (Hebbian updates, decay) skip the rebuild since
they don't affect which ports are connected, only the softmax
distribution within the routing table. Register/unregister always
rebuild immediately.

### 2.7 The tick cycle

```
SeedReactor::tick(dt)
  1. Tick all modules (advance internal state via module.tick(dt))
  2. Collect emitted signals from all modules into shared buffer
     — borrow checker safe: emit phase completes before deliver phase
  3. Route signals through RoutingTable, deliver to target modules
     — multi-cast: all connected targets receive full signal clone
     — type-check at delivery, record delivery success/failure
     — track per-module stats: signals_received, errors
  4. Update AffinityGraph:
     a. Emotion update (activity EWMA, error EWMA, valence, arousal)
     b. Edge decay (weight→0.5, eligibility fades)
     c. Delivery recording (goodput, impact, eligibility spike)
     d. Hebbian update (dw = LR × eligibility × valence × goodput)
     e. Prune old weak edges (age > 1000 && weight < 0.1)
  5. Exploration: bored modules (arousal > 0.3) try new edges
  6. Rebuild routing table if topology changed
```

### 2.8 Dependencies

```toml
rand = "0.8"
rand_xoshiro = "0.6"
```

## Files Created

```
src/affinity/mod.rs       — pub mod edge, emotion, graph, ledger;
src/affinity/edge.rs      — EdgeId, EdgeAffinity, Hebbian learning
src/affinity/emotion.rs   — ModuleEmotion, homeostatic gain
src/affinity/graph.rs     — AffinityGraph, tick cycle, explore, prune, softmax
src/affinity/ledger.rs    — LedgerEvent, LedgerRingBuffer
src/reactor/mod.rs        — SeedReactor, 3 stub module integration tests
src/reactor/routing.rs    — Delivery, RoutingTable
```

## Files Modified

```
src/main.rs               — add `mod affinity; mod reactor;`
Cargo.toml                — add rand, rand_xoshiro
```

## Design Decisions

**Valence formula revised.** Original spec `-(error^2) - error_rate * 2.0`
was always ≤ 0. Changed to `1.0 - error^2 * 4.0 - error_rate * 2.0` so
positive valence drives Hebbian strengthening of good edges.

**Eligibility spikes on all deliveries.** Invalid deliveries spike
eligibility at 0.5× (vs 1.0× for valid). Without this, bad edges had
zero eligibility and couldn't be weakened by negative valence.

**Default target_activity = 1.0.** The original 5.0 made all modules
feel "starved" at startup when they only receive 0-2 signals/tick,
causing universal negative valence and weakening of all edges.

**Multi-cast routing.** Signals are delivered to all connected targets,
not probabilistically routed to one. Softmax weights affect learning
only. This matches continuous signal flow (audio, video) where all
consumers need every sample.

**Topology-gated routing table rebuild.** The routing table only
rebuilds when `topology_dirty` is true (edges added/removed/pruned).
Weight-only changes don't trigger a rebuild. This avoids redundant
work on ~99% of ticks where only Hebbian learning runs.

## Verification (35 tests passing)

Edge tests (8):
1. New edge starts at neutral defaults
2. Tick decay ages and decays eligibility
3. Good deliveries maintain goodput
4. Bad deliveries tank goodput
5. Positive reward strengthens weight
6. Negative reward weakens weight
7. Weight stays clamped to [0, 1]
8. Prune condition: old + weak = pruned

Emotion tests (7):
9. New emotion starts neutral
10. Steady activity at target → positive valence
11. Starved module → negative valence
12. High errors → very negative valence
13. Homeostatic gain > 1.0 when starved
14. Homeostatic gain < 1.0 when flooded
15. Arousal spikes on surprise (sudden activity change)

Graph tests (8):
16. Add edge + ledger recording
17. Tick decays all edges
18. Good deliveries + positive valence → weight strengthens past 0.5
19. Bad deliveries → weight weakens below 0.5
20. Pruning removes old weak edges + logs to ledger
21. Exploration creates type-compatible edges (probabilistic)
22. Softmax weights sum to 1.0 per output port
23. Unregister removes module + all its edges

Ledger tests (4):
24. Record and retrieve events
25. Capacity evicts oldest (ring buffer)
26. Filter events by edge ID
27. Recent(N) returns newest first

Reactor integration tests (7):
28. Register auto-discovers compatible edges between modules
29. Signals route end-to-end through 3-module chain
30. 1000-tick convergence: active edges strengthen, weights bounded [0,1], ledger populated
31. Unregister removes module + edges, reduces edge count
32. Emotions respond to activity (processor shows activity > 0)
33. Ledger traces full edge history (creation + Hebbian updates)
34. Softmax normalization: per-port weights sum to ~1.0

Bounded weight test (1):
35. 500-tick simulation: all weights stay in [0, 1]
