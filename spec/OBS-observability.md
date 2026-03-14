# OBS — Observability Infrastructure

**Status**: Complete (Mar 2026)
**Goal**: Provide runtime introspection of affinity graph dynamics, per-organism musical state, and deterministic signal transforms — without impacting audio-thread performance.

**Depends on**: S01 (module contract), SAT (port satisfaction)
**Blocks**: L5 UX shell (inspectors need temporal data to display)

---

## Architecture

### 1. HandleId (RT-safe handle indexing)

`HandleId(u16)` replaces `HashMap<String, Shared>` for parameter lookup on the audio thread. HashMap hashing is ~80 cycles per lookup with cache-unfriendly pointer chasing; `HandleId` is a direct `Vec<Shared>` index at ~3 cycles.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleId(pub u16);
```

Assigned at `from_dna()` time when `ModWire` entries are built. The `u16` range (65536 handles) far exceeds any realistic organism's parameter count. The `Shared` wrapper itself stores `f32` as `Arc<AtomicU32>` with `Relaxed` ordering — lock-free, no allocation on read/write.

### 2. EdgeTrajectory (per-edge state history)

Ring buffer of `TrajectorySample` snapshots capturing affinity graph edge dynamics over time.

```rust
pub struct TrajectorySample {
    pub tick: u64,
    pub weight: f32,
    pub satisfaction: f32,
    pub impact: f32,
}
```

- **Capacity**: 256 entries, `Box<[TrajectorySample; 256]>` — heap-allocated fixed array, no Vec resizing or reallocation.
- **Sampling**: Configurable interval (default 4 ticks). At 60fps, 256 samples = ~17 seconds of history.
- **write_pos**: `u8` wraps naturally at 256 — no modulo needed.
- **Queries**: `iter()` for chronological traversal, `at_tick(u64)` for nearest-tick lookup (correlation with MusicalContext), `recent(n)` for trailing window, `latest()` for most recent.

### 3. TrajectoryStore + ExplorationEvent (graph-level observability)

`TrajectoryStore` owns all `EdgeTrajectory` instances keyed by `EdgeId` and a bounded log of exploration events.

```rust
pub struct ExplorationEvent {
    pub tick: u64,
    pub module_id: ModuleId,
    pub arousal: f32,
    pub candidate_count: usize,
    pub chosen: Option<EdgeId>,
    pub reason: &'static str,
}
```

- `track(edge_id)` / `untrack(edge_id)` — lifecycle follows edge creation/pruning.
- `sample_all(tick, edges)` — batch-samples every tracked edge per frame.
- `log_exploration(event)` — 100-entry `VecDeque`, FIFO eviction. Records why the graph explored (arousal-driven stochastic connection attempts), what candidates existed, and which edge was chosen.

### 4. MusicalContext (per-organism snapshot)

~72-byte `Copy` struct aggregating all musical state for one organism. Fields grouped by single writer to avoid contention:

| Group | Writer | Fields |
|-------|--------|--------|
| Pitch | `receive_signal(pitch_hz_port)` | `prompted_pitch_hz`, `actual_pitch_hz`, `scale_degree`, `pitch_deviation_cents` |
| Scale | `receive_gravity_weights()` | `scale_blend`, `scale_active` |
| Gamaka | `receive_gamaka_config()` | `gamaka_state`, `gamaka_depth` |
| Rhythm | `receive_beat_phase/trigger()` | `beat_phase`, `ticks_since_beat`, `rhythm_sync_mode`, `rhythm_affinity` |
| Direction | `tick()` (DirectionTracker) | `melodic_direction` |
| Audio | `tick()` (DspAnalysis) | `rms`, `seq_gate` |
| Identity | constructor (immutable) | `species_code`, `fidelity` |
| Timing | each update | `tick` |

Constructed once from DNA via `from_dna(species, fidelity, scale_affinity, rhythm_affinity, rhythm_sync)`. Updated at 60Hz on the control thread, never on the audio thread.

Species codes: 0=dron, 1=hoso, 2=spgl, 3=acid, 4=tblk, 5=kkit, 6=other, 7=isao.

Helper `nearest_degree_cents(hz, weights)` computes (degree, deviation_cents) from Hz + gravity weights — used to populate `scale_degree` and `pitch_deviation_cents`.

### 5. ContextHistory (temporal correlation)

256-entry ring buffer of `MusicalContext` snapshots, identical ring buffer design to `EdgeTrajectory`. ~18KB per organism (72 bytes x 256).

- Same interval-based sampling (`maybe_snapshot()`), same `at_tick()` nearest-lookup.
- **Correlation**: `EdgeTrajectory.at_tick(t)` + `ContextHistory.at_tick(t)` answers "what was the organism playing when this edge weight changed?" — the foundation for L5 causal inspectors.
- `iter()` for trend analysis (pitch drift, rhythmic alignment over time).

### 6. ProcessChain (deterministic transforms)

Ordered signal transform pipeline that runs outside the AffinityGraph — no Hebbian learning, no weights, deterministic order. Use cases: master EQ, velocity curves, sidechain compression, parameter automation.

```rust
pub trait ProcessStep: Send {
    fn process(&mut self, signal: Signal) -> Option<Signal>;
    fn accepts(&self) -> SignalType;
    fn name(&self) -> &str;
}
```

- **ChainPlacement**: `PreRoute` (normalize/gate/inject before AffinityGraph routing) or `PostRoute` (master EQ/limiter at delivery).
- **Port filtering**: `global: bool` + `target_ports: Vec<PortId>`. Global chains apply to all signals of their type; filtered chains apply only to specific target ports.
- **Suppression**: Any step returning `None` kills the signal entirely — useful for gates.
- **ProcessChainSet**: Collection with `apply_pre_route()` and `apply_post_route()` separation. Empty set = passthrough (zero overhead when no chains registered).

Infrastructure is complete. Not yet wired into app.rs tick cycle — ready for L5 UI integration.

---

## Critical Files

| File | Contents |
|------|----------|
| `src/dsp/shared.rs` | `HandleId(u16)`, `Shared` (AtomicU32 wrapper), `shared()` constructor |
| `src/affinity/trajectory.rs` | `EdgeTrajectory`, `TrajectorySample`, `TrajectoryStore`, `ExplorationEvent`, `TrajectoryIter` |
| `src/organism/module/context.rs` | `MusicalContext`, `ContextHistory`, `species_code_from_str()`, `nearest_degree_cents()` |
| `src/reactor/process_chain.rs` | `ProcessStep` trait, `ProcessChain`, `ProcessChainSet`, `ChainPlacement` |

---

## Design Constraints

- `MusicalContext` is `Copy` and fits in <=96 bytes (enforced by `context_size_assert` test).
- `EdgeTrajectory` and `ContextHistory` use `Box<[T; 256]>` — heap-allocated fixed arrays, no Vec resizing or reallocation on the hot path.
- `write_pos` is `u8` — wraps at 256 naturally without modulo arithmetic.
- All observability runs on the control thread at 60Hz, never on the audio thread.
- `HandleId` is the only OBS component that touches the audio thread — it eliminated HashMap from the RT path.
- `ProcessChain` is infrastructure-complete but not yet wired into the tick cycle (ready for L5 UI).

---

## Verification

**Unit tests** (20 cases across 4 files):

EdgeTrajectory:
1. `trajectory_samples_at_interval` — respects sample interval, skips intermediate ticks
2. `trajectory_ring_wraps` — 300 samples → 256 retained, oldest = tick 45
3. `trajectory_at_tick` — nearest-tick lookup returns correct sample
4. `trajectory_lifecycle` — track/untrack lifecycle in TrajectoryStore
5. `trajectory_recent` — trailing N samples in order
6. `exploration_log_capacity` — 150 events → 100 retained, FIFO eviction from tick 50
7. `store_sample_all` — batch sampling propagates edge weight
8. `empty_trajectory_queries` — all queries safe on empty buffer

MusicalContext:
9. `context_default_values` — sane defaults (C4, no scale, no beat)
10. `context_from_dna` — species code, fidelity, blend, rhythm sync mode
11. `context_size_assert` — struct <= 96 bytes
12. `context_history_snapshots` — interval-based snapshot, latest() correct
13. `context_history_ring_wraps` — 300 snapshots → 256 retained
14. `context_at_tick_correlation` — nearest-tick lookup with RMS verification
15. `nearest_degree_cents_basic` — 440Hz→A(9), near-zero deviation
16. `nearest_degree_cents_weighted` — sparse weights snap to nearest active degree
17. `empty_history_queries` — all queries safe on empty buffer

ProcessChain:
18. `chain_transforms_signal` — double+clamp pipeline applies in order
19. `chain_suppresses_on_none` — gate step kills signal above threshold
20. `chain_port_filter` — port-targeted chains respect filter
21. `chain_placement` — PreRoute/PostRoute enum correctness
22. `chain_set_pre_and_post` — set routes to correct placement
23. `chain_set_no_chains_passthrough` — empty set = identity

Shared:
24. `shared_roundtrip` — set/get preserves f32 precision
25. `shared_clone_shares_state` — Arc sharing verified
26. `shared_negative_values` — negative f32 roundtrips correctly
