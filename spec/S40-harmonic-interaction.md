# S40 — Harmonic Interaction

## Context

Organisms navigate gravity wells and develop pairwise affinities, but have zero awareness of each other's pitch. Consonance exists only organism-to-well (`consonance_weight(interval)` — a coarse 4-tier table). Two organisms playing a perfect fifth in the same well get no benefit from their harmonic relationship — they even compete via spectral niche penalty when their centroids overlap.

This spec adds organism-to-organism harmonic awareness using **Tenney height** as the consonance model: consonant pairs cooperate (valence boost, reduced niche penalty, stronger affinity), dissonant pairs create productive tension (arousal boost, exploration pressure). Static DNA roots create persistent family bonds; live pitch adds moment-to-moment musical variation.

**Depends on**: S38 (well ecology), S39 (navigation reward), SAT (port satisfaction)
**Blocks**: organism-union (harmonic compatibility is a union prerequisite)

---

## Architecture

### 1. Tenney Consonance Model

**Tenney height** for a just-intonation ratio p/q = log2(p × q). Lower = more consonant. We map each 12-TET interval to its nearest JI ratio and derive consonance via exponential decay:

```rust
/// consonance = exp(-TENNEY_DECAY * log2(p * q))
/// TENNEY_DECAY = 0.22
const TENNEY_CONSONANCE: [f32; 12] = [
    1.000,  //  0 semitones: unison   1/1   TH=0.0
    0.175,  //  1: minor 2nd         16/15  TH=7.9
    0.258,  //  2: major 2nd          9/8   TH=6.2
    0.339,  //  3: minor 3rd          6/5   TH=4.9
    0.386,  //  4: major 3rd          5/4   TH=4.3
    0.454,  //  5: perfect 4th        4/3   TH=3.6
    0.100,  //  6: tritone           45/32  TH=10.5
    0.566,  //  7: perfect 5th        3/2   TH=2.6
    0.310,  //  8: minor 6th          8/5   TH=5.3
    0.422,  //  9: major 6th          5/3   TH=3.9
    0.207,  // 10: minor 7th         16/9   TH=7.2
    0.219,  // 11: major 7th         15/8   TH=6.9
];
```

**Ranking**: unison(1.0) > 5th(0.57) > 4th(0.45) > 6th(0.42) > M3(0.39) > m3(0.34) > m6(0.31) > M2(0.26) > M7(0.22) > m7(0.21) > m2(0.18) > tritone(0.10)

Replaces the old 4-tier `consonance_weight()` for organism-to-organism use. Well-to-organism consonance continues to use the existing table (or can be migrated later).

**Live consonance with detuning penalty**: For actual Hz values, compute cents interval, find nearest JI ratio, apply Tenney height, then penalize detuning:

```rust
fn tenney_consonance_hz(hz_a: f32, hz_b: f32) -> f32 {
    let cents = (1200.0 * (hz_a / hz_b).log2()).abs() % 1200.0;
    let cents = cents.min(1200.0 - cents);  // shortest path around octave
    let (nearest_cents, consonance) = find_nearest_ji(cents);  // table lookup
    let detune = (cents - nearest_cents).abs();
    let detune_penalty = (1.0 - detune / DETUNE_TOLERANCE).max(0.0);
    consonance * detune_penalty
}
```

`DETUNE_TOLERANCE = 30.0` cents — within ~quarter-tone of JI, consonance is present; beyond, it fades to zero. This captures microtonal sensitivity.

### 2. HarmonicPair — Pairwise Snapshot

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct HarmonicPair {
    pub root_consonance: f32,   // static: TENNEY_CONSONANCE[interval]
    pub live_consonance: f32,   // dynamic: tenney_consonance_hz()
    pub consonance: f32,        // blended: root * 0.3 + live * 0.7
}
```

**Location**: `src/tuning/harmony.rs` (new file, ~120 lines)

- `root_consonance`: from `root_pitch_class` DNA → interval → `TENNEY_CONSONANCE[i]`
- `live_consonance`: from `seq_pitch_hz` → `tenney_consonance_hz()`
- Fallback: if either `seq_pitch_hz == 0.0`, `consonance = root_consonance`
- Exemption: if either `scale_affinity < 0.01`, `consonance = 0.0` (KKIT)

### 3. Data Pipeline

**Add to `WellDispatchEntry`** (app.rs:49):
```rust
seq_pitch_hz: f32,  // from OrganismModule::current_seq_pitch_hz()
```

Populated alongside existing `spectral_centroid` in the dispatch buffer fill loop (~line 1317).

**No new field on `OrganismState`** — `seq_pitch_hz` is transient per-frame dispatch data.

For emergent affinity (registry.rs), we pass `seq_pitch_hz` via a new transient lookup populated each frame from OrganismModule, alongside the existing `root_pitch_class` already on `OrganismState`.

### 4. Well Harmonic Bonus (Effect 1 + Effect 4)

In `apply_well_forces()`, after existing niche penalty pass (~line 618):

```
For each well W:
    For co-occupant pairs (i, j) with influence > 0:
        pair = compute_harmonic_pair(entry_i, entry_j)
        accumulate harmonic_bonus for both organisms

// Effect 4: Consonant organisms tolerate spectral overlap
adjusted_niche = niche_penalty * (1.0 - max_pair_consonance * CONSONANCE_NICHE_REDUCTION)

// Effect 1: Harmonic bonus boosts well quality
prox.harmonic_bonus = avg_consonance_across_cooccupants
```

**WellProximity gains one field**: `pub harmonic_bonus: f32`

**Modified net_score**:
```rust
net_score = best_quality * well_energy
    * (1.0 - adjusted_niche + harmonic_bonus * HARMONIC_BONUS_WEIGHT)
```

Flows automatically into `port_satisfaction()` → Hebbian learning via existing `net_score * WELL_SAT_WEIGHT`. No satisfaction pipeline changes needed.

### 5. Emergent Affinity — Harmonic Term (Effect 2)

Modify `compute_emergent_affinities()` in registry.rs:

```rust
// Current:  proximity * 0.35 + audio_corr * 0.35 + desire_avg * 0.30
// Proposed: proximity * 0.30 + audio_corr * 0.25 + desire_avg * 0.25 + harmonic * 0.20
```

Consonant organisms build affinity faster → stronger attachment → tighter orbits → Chladni field merging → eventually union eligibility.

### 6. Harmonic Emotion Modulation (Effect 3)

New per-frame pass in app.rs, after well dispatch:

```rust
fn apply_harmonic_emotions(dispatch_buf, emotions):
    for each pair (i, j) within HARMONIC_AWARENESS_RANGE:
        pair = compute_harmonic_pair(i, j)
        proximity_gate = (1 - dist / RANGE).max(0)

        // Consonance > 0.4 → positive valence (musical satisfaction)
        if pair.consonance > 0.4:
            valence_delta += (consonance - 0.4) * proximity * scale_affinity * VALENCE_RATE

        // Consonance < 0.25 → arousal (tension, creative friction)
        if pair.consonance < 0.25:
            arousal_delta += (0.25 - consonance) * proximity * scale_affinity * AROUSAL_RATE

    emotion.apply_harmonic_reward(valence_delta)
    emotion.apply_harmonic_tension(arousal_delta)
```

New methods on `ModuleEmotion`:
- `apply_harmonic_reward(delta: f32)` — additive valence, clamped [-1, 1]
- `apply_harmonic_tension(tension: f32)` — additive arousal, clamped [0, 1]

Thresholds adjusted for Tenney range: consonance > 0.4 triggers satisfaction (5th, 4th, 6th, M3); consonance < 0.25 triggers tension (m2, M2, m7, tritone).

### 7. Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `TENNEY_DECAY` | 0.22 | Exponential decay in Tenney mapping |
| `DETUNE_TOLERANCE` | 30.0 cents | Detuning penalty tolerance for live pitch |
| `HARMONIC_BONUS_WEIGHT` | 0.15 | Well quality boost from consonant co-occupants |
| `CONSONANCE_NICHE_REDUCTION` | 0.6 | Niche penalty reduction for consonant pairs |
| `HARMONIC_AWARENESS_RANGE` | 600.0 px | Spatial range for emotion effects |
| `HARMONIC_VALENCE_RATE` | 0.02 /frame | Consonance → valence gain |
| `HARMONIC_AROUSAL_RATE` | 0.015 /frame | Dissonance → arousal gain |
| `HARMONIC_AFFINITY_WEIGHT` | 0.20 | Weight in emergent affinity formula |

### 8. Natural Organism Families (Tenney values)

| Pair | Interval | Tenney consonance |
|------|----------|-------------------|
| DRON↔ACID, DRON↔ISAO, ACID↔ISAO | A-A unison | 1.000 |
| HOSO↔SPGL, HOSO↔TBLK, SPGL↔TBLK | C-C unison | 1.000 |
| A-root ↔ C-root (3 semitones) | minor 3rd | 0.339 |
| KKIT ↔ anyone | exempt | 0.000 |

Two families emerge: A-root (DRON, ACID, ISAO) and C-root (HOSO, SPGL, TBLK). Cross-family = minor 3rd (moderate). Dynamic `live_consonance` adds variation as organisms play different pitches within the active raga.

---

## Implementation Phases

**Phase A: Tenney consonance module**
1. Create `src/tuning/harmony.rs` — `TENNEY_CONSONANCE` table, `tenney_consonance_hz()`, `hz_to_pitch_class()`, `compute_harmonic_pair()`, `HarmonicPair`
2. JI interval table for live lookup (12 entries: cents + consonance value)
3. Add `pub mod harmony;` to `src/tuning/mod.rs`
4. Unit tests: static intervals, dynamic Hz pairs, detuning penalty, KKIT exemption, fallback

**Phase B: Well harmonic bonus**
1. Add `harmonic_bonus: f32` to `WellProximity`, `Default` impl
2. Add `seq_pitch_hz` to `WellDispatchEntry`, populate in fill loop
3. Extend existing `well_occupants` pass: compute pairwise consonance, accumulate bonus
4. Reduce niche penalty by consonance factor
5. Incorporate harmonic_bonus into `net_score` formula
6. Tests: unison co-occupants boost, tritone don't, niche reduction verified

**Phase C: Emergent affinity**
1. Pass `seq_pitch_hz` into `compute_emergent_affinities()` via per-frame transient data
2. Add harmonic term, rebalance formula weights
3. Tests: consonant pair → higher target than dissonant at same distance

**Phase D: Harmonic emotion**
1. Add `apply_harmonic_reward()` / `apply_harmonic_tension()` to `ModuleEmotion`
2. Add `apply_harmonic_emotions()` pass in app.rs main loop
3. Tests: valence for consonance, arousal for dissonance, zero outside range, scale_affinity gating

---

## Critical Files

| File | Changes |
|------|---------|
| `src/tuning/harmony.rs` | **New** — Tenney model, HarmonicPair, consonance functions |
| `src/tuning/mod.rs` | Add `pub mod harmony;` |
| `src/tuning/gravity_well.rs` | Add `harmonic_bonus` to WellProximity |
| `src/app.rs` | seq_pitch_hz in dispatch, harmonic bonus pass, harmonic emotion pass |
| `src/organism/registry.rs` | Harmonic term in compute_emergent_affinities() |
| `src/affinity/emotion.rs` | apply_harmonic_reward(), apply_harmonic_tension() |

**Reused existing code**:
- `well_occupants` Vec in app.rs — already collects co-occupant indices per well
- `WellDispatchEntry` pattern — already carries org_root, spectral_centroid
- `ModuleEmotion::apply_navigation_reward()` — pattern for additive valence

---

## Verification

**Unit tests** (11 cases):
1. `tenney_table_ordering` — 5th > 4th > M3 > m3 > M2 > tritone
2. `hz_to_pitch_class` — 440Hz→A(9), 261.6Hz→C(0), 330Hz→E(4)
3. `tenney_hz_perfect_fifth` — 440+660Hz → consonance ≈ 0.566
4. `tenney_hz_detuned_fifth` — 440+650Hz → consonance < pure fifth (detune penalty)
5. `tenney_hz_tritone` — 440+622Hz → consonance ≈ 0.100
6. `consonance_static_unison` — DRON(A)+ACID(A) → 1.000
7. `consonance_kkit_exempt` — KKIT pair → 0.0
8. `consonance_fallback_no_seq` — seq_pitch=0 → uses root_consonance only
9. `well_bonus_consonant_cooccupants` — unison pair → high harmonic_bonus
10. `niche_penalty_reduced_by_consonance` — same centroids + consonant → lower penalty
11. `harmonic_emotion_thresholds` — consonance > 0.4 → valence, < 0.25 → arousal

**Manual verification**:
- DRON(A) + ACID(A) near same well → strong mutual attraction, tight orbits
- DRON(A) + HOSO(C) → moderate attraction (m3, consonance=0.339)
- KKIT unaffected by any harmonic interaction
- Two organisms playing a fifth in real-time → visible affinity strengthening
