# S40 Rewrite — Harmonic Interaction from Consumed Substrate

**Status**: Spec (rewrite of S40 Harmonic Interaction)
**Depends on**: substrate-encoding.md, S33-rewrite
**Blocks**: organism-satisfaction rewrite

---

## What Changes

**Before (S40)**: Organism-to-organism consonance computed from Tenney height of their root pitch classes. Static root (30%) + live pitch (70%) blended. Consonance modulates well quality, niche penalty, emergent affinity, valence/arousal.

**After**: Consonance emerges from what organisms consumed, not from what they are. Two organisms eating similar substrate produce similar pitches → consonance is high. Two organisms eating different substrate → dissonance. Harmonic relationships are emergent, not computed from DNA root_pitch_class.

---

## Consonance Model

### Old: Static Root Comparison
```
consonance(A, B) = 0.3 × root_consonance(A.root_pc, B.root_pc)
                 + 0.7 × live_consonance(A.seq_pitch_hz, B.seq_pitch_hz)
```

### New: Consumption Overlap
```
consonance(A, B) = histogram_overlap(A.pitch_histogram, B.pitch_histogram)
```

Histogram overlap (cosine similarity):
```rust
fn histogram_consonance(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a < 0.001 || mag_b < 0.001 { return 0.0; }
    (dot / (mag_a * mag_b)).clamp(0.0, 1.0)
}
```

Two organisms eating the same substrate colors → histograms align → consonance ≈ 1.0.
Two organisms with different appetites eating the same region → histograms diverge by appetite weighting → partial consonance.
Two organisms in different substrate regions → unrelated histograms → low consonance.

### Why This Is Better

- Consonance is **emergent**, not prescribed by DNA root_pitch_class
- Organisms that happen to graze the same area develop harmonic affinity naturally
- Moving apart reduces consonance, moving together increases it
- Key change shifts ALL histograms simultaneously (substrate recolors) — relationships preserved
- No static 30% root contribution — everything is live

---

## Effects (Same Hooks, Different Source)

The four S40 effects stay, they just read from consumption consonance instead of Tenney height:

### 1. Well Quality Bonus → REMOVED
Wells no longer grant harmonic quality. They focus substrate energy (lens model). Organisms near wells eat concentrated substrate → their histograms become more focused → consonance with similarly-focused organisms increases naturally.

### 2. Niche Penalty → Substrate Competition
When two organisms of different species consume the same pitch class heavily, they compete:
```
competition(A, B) = histogram_overlap(A, B) × appetite_overlap(A.profile, B.profile)
```
High competition → slight valence penalty (crowded niche). Different species eating different channels at the same location → no competition (complementary foraging).

### 3. Emergent Affinity Term → Consumption Affinity
The affinity graph edge weight between two organisms gets a bonus proportional to consumption consonance:
```
affinity_bonus = consonance(A, B) × 0.1
```
Organisms that produce similar music (because they ate similar substrate) strengthen their connection. This creates musical "herds" — organisms that graze together, play together.

### 4. Valence/Arousal Modulation → From Consonance
Same as S40 but sourced from consumption consonance:
```
// High consonance with neighbors → positive valence (harmony satisfaction)
valence_delta += consonance × 0.05

// Low consonance when crowded → arousal spike (discomfort, explore)
if neighbors > 2 && mean_consonance < 0.3:
    arousal_delta += 0.02
```

---

## root_pitch_class Reinterpretation

`root_pitch_class` in DNA no longer determines what the organism "wants to hear." Instead it becomes the organism's **tonal center for output** — which octave register it prefers to produce sound in:

```
output_hz = quantized_hz × 2^(root_pitch_class / 12 - 0.5)
```

This is a subtle shift: root_pitch_class affects output transposition, not input preference. An organism with root_pitch_class=9 (A) produces sound an A-offset from whatever pitch class it consumed, not specifically seeking A in the substrate.

---

## Critical Files

| File | Change |
|------|--------|
| `src/tuning/harmony.rs` | Replace Tenney height with histogram_consonance() |
| `src/app.rs` | Pairwise consonance from pitch_histograms, not root PCs |
| `src/organism/sim.rs` | pitch_histogram already added (S33 rewrite) |
| `src/organism/interaction.rs` | Competition from histogram overlap × appetite overlap |
| `src/affinity/emotion.rs` | Valence/arousal from consumption consonance |

---

## Verification

1. Two organisms at same position eating same substrate → consonance high → affinity strengthens
2. Move one organism away → consonance drops as histograms diverge
3. Key change → all histograms shift together → inter-organism consonance preserved
4. Different species at same location → partial consonance (different appetite channels)
5. No more "snapping" to preferred pitches — organisms produce what they eat
