# SAT — Receiver Satisfaction

**Status**: Complete (Mar 2026)
**Goal**: Add per-port input health measurement to the Hebbian learning formula, so edges that deliver useful signals strengthen and edges that deliver noise or redundant signals weaken.

**Depends on**: OBS (observability — HandleId, EdgeTrajectory)
**Blocks**: S38 (well ecology satisfaction integration), S40 (harmonic interaction)

---

## Architecture

### 1. Port Satisfaction

`port_satisfaction(port: PortId) -> f32` is a default method on the `ModuleCore` trait, returning `[0, 1]`. Default: `1.0` (neutral — all signals equally useful). Modules override to report domain-specific quality.

**OrganismModule overrides** three ports:

| Port | Metric | Score |
|------|--------|-------|
| `pitch_hz_port` | Cents deviation from nearest scale degree, scaled by `scale_blend` tolerance. Ecological bonus from `well_proximity.net_score * WELL_SAT_WEIGHT`. | `(1 - cents/tolerance + eco_bonus).clamp(0, 1)` |
| `beat_trigger_port` | Phase error — how close `beat_phase` was to 0 or 1 when trigger arrived. | `(1 - phase_error * 4).clamp(0, 1)` |
| `gate_port` | Whether the organism is producing sound. Gating silence is mildly wasteful. | `1.0` if RMS > 0.01, else `0.5` |

The tolerance formula for pitch: `50.0 / scale_blend.max(0.01)`. This creates personality-driven strictness — HOSO (blend ~0.72, tolerance ~69 cents) is strict; DRON (blend ~0.09, tolerance ~555 cents) accepts nearly anything; KKIT (blend ~0.0) is always satisfied.

Called by `SeedReactor` immediately after each successful `receive_signal()` delivery. On error, satisfaction is forced to `0.0`.

### 2. Delta Impact

Each `EdgeAffinity` tracks **impact** — an EWMA of signal change magnitude between consecutive deliveries. This measures information novelty, not raw amplitude.

```
delta = |current_magnitude - prev_magnitude|
impact = impact * (1 - EWMA_ALPHA) + delta * EWMA_ALPHA
```

A constant signal (same value every tick) converges impact toward 0. A changing signal keeps impact elevated. Impact is tracked on the edge but does not currently enter the Hebbian formula directly — it is available for future use (e.g., pruning edges that carry no new information).

`EWMA_ALPHA = 0.05` — slow integration, ~20-tick effective window.

### 3. Hebbian Learning Formula

Weight update runs once per tick for every edge, using the receiving module's valence as the reward signal:

```
dw = LR * eligibility * valence * satisfaction
weight = (weight + dw).clamp(0.0, 1.0)
```

| Term | Source | Range | Role |
|------|--------|-------|------|
| `LR` | Constant `0.02` | — | Base learning rate |
| `eligibility` | Edge field, decays by `ELIG_DECAY = 0.85` per tick, spikes to 1.0 on delivery | [0, 1] | Recency trace — recently active edges learn faster |
| `valence` | `ModuleEmotion` on receiving module | [-1, 1] | Reward signal. Positive when module is on-target with low errors; negative when starved or error-prone |
| `satisfaction` | Edge field, EWMA of `port_satisfaction()` returns | [0, 1] | Quality gate — only edges delivering useful signals get the full learning rate |

The interaction: positive valence + high satisfaction = weight increases (good edge, happy receiver). Negative valence + high satisfaction = weight decreases (receiver unhappy despite good-quality signal — the connection is counterproductive). Low satisfaction attenuates both directions — the edge is delivering junk, so learning is dampened regardless of emotion.

### 4. Satisfaction EWMA on Edge

Satisfaction is stored per-edge as an EWMA, blending receiver quality with type validity:

```
effective = if type_valid { receiver_satisfaction } else { 0.0 }
satisfaction = satisfaction * (1 - EWMA_ALPHA) + effective * EWMA_ALPHA
```

Type mismatch forces effective to 0 regardless of receiver opinion — an edge delivering Float to a Trigger port gets no credit. Initial value: `1.0` (optimistic prior — new edges are assumed useful until proven otherwise).

### 5. Eligibility Trace

Eligibility marks "this edge was recently active" for credit assignment. Spikes on delivery (1.0 for valid type, 0.5 for mismatched), decays by `ELIG_DECAY = 0.85` each tick. An edge that hasn't fired in ~30 ticks has eligibility < 0.01 and effectively stops learning.

### 6. Decay and Pruning

All edges drift toward neutral weight (`0.5`) at rate `DECAY = 0.001` per tick. This prevents stale edges from locking in historical weights.

Pruning: edges older than `PRUNE_MIN_AGE = 1000` ticks with weight below `PRUNE_WEIGHT_THRESHOLD = 0.1` are removed. Pruning triggers a topology rebuild of the `RoutingTable`.

### 7. Feedback into Emotion

Satisfaction does not directly feed back into `ModuleEmotion`. The feedback loop is indirect:

1. Satisfaction modulates Hebbian learning rate on edges.
2. Edge weights determine signal routing (softmax-weighted multicast).
3. Routing quality determines how many useful signals a module receives.
4. Signal throughput and error rate drive homeostatic valence/arousal via `ModuleEmotion::update()`.

Additionally, downstream systems (S38 well ecology, S40 harmonic interaction) inject satisfaction-derived bonuses into `ModuleEmotion` via dedicated methods (`apply_navigation_reward`, `apply_harmonic_reward`, etc.).

### 8. Tick Cycle

The full cycle per `AffinityGraph::tick()`:

1. **Emotion update** — module stats (signals_received, errors) → `ModuleEmotion::update()`
2. **Edge decay** — all edges: weight drift + eligibility decay + age increment
3. **Record deliveries** — `on_delivery()` updates satisfaction EWMA, impact EWMA, eligibility spike
4. **Hebbian update** — `apply_reward(valence)` on each edge using receiver's emotion
5. **Trajectory sampling** — periodic edge state snapshots for observability
6. **Pruning** — remove old weak edges, mark topology dirty

---

## Critical Files

| File | Contents |
|------|----------|
| `src/module/mod.rs` | `ModuleCore::port_satisfaction()` default method (returns 1.0) |
| `src/affinity/edge.rs` | `EdgeAffinity`: weight, eligibility, satisfaction, impact fields; `on_delivery()`, `apply_reward()`, `tick_decay()`, `should_prune()` |
| `src/affinity/graph.rs` | `AffinityGraph::tick()` — full Hebbian cycle; `DeliveryRecord` with satisfaction field |
| `src/affinity/emotion.rs` | `ModuleEmotion` — valence/arousal from homeostatic activity tracking |
| `src/organism/module/mod.rs` | `OrganismModule::port_satisfaction()` — pitch, beat, gate overrides |
| `src/reactor/mod.rs` | Delivery loop — calls `port_satisfaction()` after each `receive_signal()` |

---

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `LR` | 0.02 | Hebbian learning rate |
| `DECAY` | 0.001 | Weight drift toward 0.5 per tick |
| `ELIG_DECAY` | 0.85 | Eligibility trace decay per tick |
| `EWMA_ALPHA` | 0.05 | Smoothing factor for satisfaction and impact |
| `PRUNE_MIN_AGE` | 1000 ticks | Minimum age before pruning eligible |
| `PRUNE_WEIGHT_THRESHOLD` | 0.1 | Weight below which old edges are pruned |
| `WELL_SAT_WEIGHT` | 0.2 | Ecological bonus weight in pitch satisfaction |

---

## Design Constraints

- Satisfaction is per-port, not per-edge — multiple edges targeting the same port get the same receiver score, but each edge maintains its own satisfaction EWMA (blended with type validity).
- Learning runs at control-thread rate (60Hz), never on the audio thread.
- Edge weights are clamped to `[0.0, 1.0]`. Pruning removes edges below 0.1 after 1000 ticks, but edges can reach 0.0 transiently.
- New edges start with `satisfaction = 1.0` (optimistic prior) and `weight = 0.5` (neutral). This lets new connections prove themselves before being penalized.
- The formula is multiplicative: any zero term (zero eligibility, zero valence, zero satisfaction) halts learning entirely for that edge that tick.
