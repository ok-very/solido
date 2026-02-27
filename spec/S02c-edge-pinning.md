# S02c — Edge Pinning + Exploration Efficiency

> Some connections are sacred. The rest are experiments.

**Layer**: L0 (Routing Backbone)
**Depends on**: S02 (routing backbone), S02b (routing refinement)
**Status**: Prospect

## Goal

Add the ability to pin edges in the AffinityGraph so they're immune to decay, pruning, and weight learning. Also cache exploration candidates per module to eliminate redundant compatibility scans. Together these give the user compositional control over the learned graph while making the autonomous system more efficient.

## Ancestry (MAKE A BABY)

The Max/MSP patch had hardwired connections (patch cables) alongside `send~/receive~` tunnels for flexible routing. Pinned edges are the patch cables — the user decides these connections exist. Learned edges are the tunnels — the system discovers what works.

## The Problem

### No compositional anchoring

All organism edges are subject to Hebbian learning. If you want "TBLK always routes pitch_hz to VoiceModule," you can't guarantee it. A brief period of negative valence on the voice module could weaken the edge below the pruning threshold (0.1 after 1000 ticks), destroying a connection the user intended to be permanent.

### Exploration is O(modules^2 * ports^2) per attempt

`discover_exploration_candidates()` (graph.rs:228-269) iterates all module pairs and all port pairs to find type/range/rate-compatible edges not yet in the graph. This runs every tick for every aroused module (10% probability). For 10 modules with 5 ports each: 250 compatibility checks per attempt.

### Ledger overflow

The ledger (1000 events) fills in ~10 ticks because every Hebbian weight update logs an event. History is lost immediately.

## Architecture Decisions

### AD-1: Pinned flag on EdgeAffinity

```rust
pub struct EdgeAffinity {
    pub weight: f32,
    pub eligibility: f32,
    pub goodput: f32,
    pub impact: f32,
    pub age: u32,
    pub pinned: bool,  // NEW — immune to decay, pruning, learning
}
```

Pinned edges:
- Weight fixed at 1.0 (or user-specified value)
- Skip decay step in `graph.tick()`
- Skip Hebbian reward update
- Never pruned regardless of age
- Appear in RoutingTable at full weight

### AD-2: Pin/unpin API on AffinityGraph

```rust
impl AffinityGraph {
    pub fn pin_edge(&mut self, edge_id: EdgeId) -> bool;
    pub fn unpin_edge(&mut self, edge_id: EdgeId) -> bool;
    pub fn is_pinned(&self, edge_id: &EdgeId) -> bool;
}
```

Pinning an edge that doesn't exist creates it (at weight 1.0). Unpinning returns the edge to learned mode (keeps current weight, resumes decay/learning). Pinning logs to ledger as `LedgerEvent::Pinned`.

### AD-3: Cached exploration candidates

Pre-compute potential edges per module on registration:

```rust
pub struct AffinityGraph {
    // ... existing fields
    /// Per-module list of candidate edges not yet in the graph.
    /// Rebuilt on register/unregister, not on every exploration attempt.
    exploration_cache: HashMap<ModuleId, Vec<EdgeId>>,
}
```

On `register_module()`: compute all compatible edges for the new module and add to cache. On `unregister_module()`: remove all entries involving that module. On `add_edge()`: remove the edge from the cache. Exploration then samples from the cached list — O(1) per attempt.

### AD-4: Ledger filters to significant events

Change Hebbian logging threshold from "any update" to `|dw| > 0.01`. This reduces ledger traffic ~10x, keeping meaningful history visible for longer. Exploration, pinning, and pruning events always log regardless of threshold.

### AD-5: Two-tier ledger

```rust
pub struct Ledger {
    hot: RingBuffer<LedgerEvent, 1000>,     // recent events, fast access
    archive: RingBuffer<LedgerEvent, 10000>, // longer history, UI scrollback
}
```

Events write to both tiers. Hot buffer gives low-latency access for real-time UI. Archive gives history for debugging and analysis.

## Implementation

### 1. Add pinned field to EdgeAffinity

`src/affinity/edge.rs`: New field, default `false`. Modify decay/reward/prune to skip pinned edges.

### 2. Pin/unpin API

`src/affinity/graph.rs`: New methods. Update `tick()` to skip pinned edges in decay and Hebbian steps. Update `prune()` to skip pinned edges.

### 3. Build exploration cache

`src/affinity/graph.rs`: New `exploration_cache` field. Populated on `register_module()`, invalidated on `unregister_module()` and `add_edge()`. `explore()` samples from cache.

### 4. Ledger filtering

`src/affinity/graph.rs`: Hebbian update only logs when `|dw| > 0.01`.

### 5. Two-tier ledger

`src/affinity/ledger.rs`: Rename existing buffer to `hot`, add `archive`. Both receive all events.

### 6. UI integration

Mixer panel or edge inspector gets a "pin" toggle per visible edge. Sends pin/unpin command to AffinityGraph via app.rs.

## Files Modified

| File | Changes |
|------|---------|
| `src/affinity/edge.rs` | `pinned` field, skip decay/reward when pinned |
| `src/affinity/graph.rs` | Pin/unpin API, exploration cache, log threshold |
| `src/affinity/ledger.rs` | Two-tier ring buffer |
| `src/reactor/mod.rs` | Exploration uses cached candidates |

## Verification

- [ ] Pinned edge stays at weight 1.0 after 10000 ticks
- [ ] Unpinned edge resumes decay from current weight
- [ ] Pinned edge survives pruning even at age > 1000
- [ ] Pin creates edge if it doesn't exist
- [ ] Exploration candidate cache reduces per-tick work (benchmark: <1us per exploration attempt)
- [ ] Ledger retains 10000 events in archive (was losing history after ~10 ticks)
- [ ] Hebbian updates with |dw| < 0.01 are not logged
- [ ] Pin/unpin appears in ledger
- [ ] All existing graph tests pass
