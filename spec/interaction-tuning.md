# Interaction Tuning — Glob State & Affinity Polish

**Status**: In progress
**Depends on**: Mood-driven repel↔merge (committed f6b9e65)
**Blocks**: organism-union.md Phase 1

---

## Goal

Perfect the existing mood-driven interaction system so organisms visibly demonstrate affinity-based behavior: repel strangers, orbit acquaintances, glob with allies. No fusion/union — just the physics and visual merging working convincingly.

---

## Current State

| Parameter | Value | Notes |
|-----------|-------|-------|
| Affinity detection range | 800px | Dedicated, independent of sonar 400px |
| Affinity smoothing | ~3s tau (EMA) | Rises and decays |
| Glob threshold | 0.25 | Visual SDF merging |
| Attraction threshold | 0.2 | Emergent pull begins |
| Attraction strength | `(aff - 0.2) × desire_avg × 10.0` | Ramps up with affinity |
| Dwell threshold | 0.5 affinity + consent | For fusion (disabled for now) |
| Orbit DNA range | 400px (all species) | Keeps organisms at ~400px center-to-center |
| Repel DNA range | 100px (all species) | Surface-to-surface after visual_radius |
| desire_to_connect initial | 0.3 (all species) | Adapts from valence |

---

## Known Issues / Critiques

### 1. Affinity formula is proximity-dominated
`affinity = proximity × (0.3 + audio_corr × 0.4 + desire_avg × 0.3)`

At orbit distance (400px in 800px range), proximity ≈ 0.5. Max target ≈ 0.5 × 0.7 = 0.35. This means orbiting organisms can glob (threshold 0.25) but attraction is weak and fusion is unreachable. Audio correlation and desire are multiplied by proximity rather than being independent signals.

**Consider**: Additive components that can build affinity even at moderate distance, or a sigmoid proximity curve instead of linear.

### 2. All species have identical interaction DNA
Every organism: Repel 100px/8.0, Slow 120px/3.0, Orbit 400px/8.0, Slow 60px/0.5. No species-specific personality in how they interact. A drone should interact differently from a drum kit.

**TODO**: Species-specific interaction profiles in DNA.

### 3. desire_to_connect starts identical for all
All default to 0.3. No DNA differentiation. Some species should be more social (HOSO — ensemble instrument) and others more solitary (DRON — ambient drone).

**TODO**: Set per-species `desire_to_connect` in DNA files.

### 4. No visual feedback of affinity state
Users can't see affinity values, desire levels, or glob group membership. Need debug overlay or subtle visual cues.

### 5. Repel modulation may be too aggressive
`repel_factor = (1.0 - affinity × desire_avg).max(0.0)` — at affinity 0.35 and desire 0.5, repel is at 82.5% of normal. The effect is subtle. May need steeper curve.

### 6. Glob groups clear every frame
`refresh_glob_groups()` clears all groups and rebuilds from scratch. This causes flicker if affinity oscillates near threshold. Need hysteresis: glob-on at 0.25, glob-off at 0.15.

---

## Tuning Targets

| Behavior | When | Expected |
|----------|------|----------|
| Full repel | Strangers (affinity < 0.1) | Normal orbit distance, no attraction |
| Weakened repel | Acquaintances (0.1-0.25) | Slightly tighter orbit |
| Visual glob | Allies (affinity > 0.25) | SDF territories merge |
| Attraction pull | Close allies (affinity > 0.2, desire > 0.3) | Organisms drift toward each other |
| Tight orbit | High affinity (> 0.4) | Noticeably closer than default 400px |

---

## Action Items

- [ ] Species-specific interaction rules in DNA (orbit ranges, repel strengths)
- [ ] Species-specific desire_to_connect defaults in DNA
- [ ] Glob hysteresis (on/off thresholds differ by ~0.1)
- [ ] Affinity debug overlay (optional, togglable)
- [ ] Tune affinity formula — consider sigmoid proximity or additive audio term
- [ ] Tune attraction strength for visible but not violent behavior
- [ ] Disable `check_integrations()` call until organism-union spec is implemented
