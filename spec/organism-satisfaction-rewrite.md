# Organism Satisfaction Rewrite — Processing Quality, Not Input Matching

**Status**: Spec (rewrite of SAT Receiver Satisfaction)
**Depends on**: S33-rewrite, S40-rewrite, S41-rewrite, nav-reward-rewrite
**Blocks**: video-substrate Phase 6 (VLM consciousness)

---

## What Changes

**Before (SAT)**: `port_satisfaction()` measures how well an incoming signal matches what the organism wanted. Satisfaction feeds Hebbian learning: `dw = LR × eligibility × valence × satisfaction`. Organisms that receive "good" signals strengthen those connections. Dissatisfaction from mismatched inputs drives exploration.

**After**: Satisfaction measures processing quality — how well the organism converts substrate energy into musical output. There's no "wanted" input. The substrate gives what it gives. Satisfaction comes from:
1. **Metabolic efficiency** — converting consumed energy into stable pitch/rhythm output
2. **Harmonic coherence** — producing consonant intervals with neighbors (from shared substrate)
3. **Rhythmic alignment** — temporal coordination with nearby organisms
4. **Nutrient balance** — all 3 channels above deficiency threshold

The organism doesn't evaluate its inputs. It evaluates its outputs.

---

## Satisfaction Sources

### 1. Metabolic Efficiency

How cleanly the organism converts consumed pitch energy into audio output:

```rust
fn metabolic_satisfaction(pitch_histogram: &[f32; 12], seq_pitch_hz: f32) -> f32 {
    // Convert current output pitch to pitch class
    let output_pc = hz_to_pitch_class(seq_pitch_hz);
    // How strongly was this pitch class consumed?
    let consumed_strength = pitch_histogram[output_pc as usize];
    let max_consumed = pitch_histogram.iter().cloned().fold(0.0f32, f32::max);
    if max_consumed < 0.01 { return 0.5; }  // No consumption = neutral
    // Satisfaction: high when output matches strongest consumed pitch
    (consumed_strength / max_consumed).clamp(0.0, 1.0)
}
```

Organism producing sound from what it ate abundantly = high satisfaction.
Organism producing sound from a pitch class it barely consumed = low satisfaction.

scale_affinity determines how strictly the quantizer snaps to consumed pitches, so high scale_affinity organisms naturally score higher here.

### 2. Harmonic Coherence (Neighbor Consonance)

From the S40 rewrite — histogram overlap with neighbors:

```rust
fn harmonic_satisfaction(
    my_histogram: &[f32; 12],
    neighbor_histograms: &[&[f32; 12]],
) -> f32 {
    if neighbor_histograms.is_empty() { return 0.5; }  // Solo = neutral
    let mean_consonance: f32 = neighbor_histograms.iter()
        .map(|h| histogram_consonance(my_histogram, h))
        .sum::<f32>() / neighbor_histograms.len() as f32;
    mean_consonance
}
```

Organisms eating the same substrate region produce similar histograms → high consonance → high satisfaction. This reward stabilizes herds — organisms that graze together stay together.

### 3. Rhythmic Alignment

How well the organism's sequencer phase aligns with nearby organisms:

```rust
fn rhythmic_satisfaction(my_beat_phase: f32, neighbor_phases: &[f32]) -> f32 {
    if neighbor_phases.is_empty() { return 0.5; }
    // Phase coherence: mean of cos(2π × (my_phase - their_phase))
    let coherence: f32 = neighbor_phases.iter()
        .map(|&p| (std::f32::consts::TAU * (my_beat_phase - p)).cos())
        .sum::<f32>() / neighbor_phases.len() as f32;
    // Map [-1, 1] to [0, 1]
    (coherence * 0.5 + 0.5).clamp(0.0, 1.0)
}
```

Organisms in phase → high rhythmic satisfaction. Anti-phase → low. This emerges naturally when organisms consume similarly-bright substrate (brightness → rhythm energy → similar tempos → phase drift toward alignment).

### 4. Nutrient Balance

The existing 3-channel nutrient system, now fed by substrate RGB:

```rust
fn nutrient_satisfaction(levels: &[f32; 3]) -> f32 {
    // Satisfaction = how far above deficiency threshold on all channels
    let min_level = levels.iter().cloned().fold(f32::MAX, f32::min);
    // 0.0 at threshold (0.3), 1.0 at fully fed (1.0)
    ((min_level - NUTRIENT_DEFICIENCY_THRESHOLD) / (1.0 - NUTRIENT_DEFICIENCY_THRESHOLD))
        .clamp(0.0, 1.0)
}
```

Well-fed organism = high nutrient satisfaction. Any deficient channel drags the score down.

---

## Combined Satisfaction Score

```rust
fn organism_satisfaction(
    metabolic: f32,
    harmonic: f32,
    rhythmic: f32,
    nutrient: f32,
) -> f32 {
    // Weighted blend — nutrient is prerequisite, others are musical quality
    let base = nutrient * 0.3 + metabolic * 0.3 + harmonic * 0.25 + rhythmic * 0.15;
    base.clamp(0.0, 1.0)
}
```

Weights reflect priority:
- **Nutrient 0.3**: Can't make music if you're starving
- **Metabolic 0.3**: Core function — convert food to sound
- **Harmonic 0.25**: Social — playing well with others
- **Rhythmic 0.15**: Temporal — being in sync (less critical than pitch)

---

## Hebbian Learning Integration

The existing Hebbian formula stays:
```
dw = LR × eligibility × valence × satisfaction
```

But satisfaction now measures OUTPUT quality, not INPUT matching:

**Before**: "Did I receive the signal I wanted?" → satisfaction
**After**: "Am I producing good music from what I ate?" → satisfaction

This changes what the affinity graph learns:
- **Before**: Strengthen connections that deliver preferred inputs
- **After**: Strengthen connections between organisms that produce good music together

An organism eating "wrong" substrate (low metabolic efficiency) but producing consonant output with neighbors (high harmonic coherence) still scores well — the system rewards musical outcome, not dietary preference.

---

## port_satisfaction() Replacement

The old `port_satisfaction()` method on ModuleCore becomes:

```rust
// Old: per-port input evaluation
fn port_satisfaction(&self, _port: PortId) -> f32 { 1.0 }

// New: organism-level output evaluation (computed in app.rs, stored on OrganismModule)
fn organism_satisfaction(&self) -> f32 {
    self.cached_satisfaction  // Updated each frame from the 4 sources
}
```

The per-port granularity is no longer needed — satisfaction is a whole-organism measure. The cached score is computed in `app.rs` during the organism update loop and stored on `OrganismModule` for the Hebbian learning pass to read.

---

## Valence Coupling

Satisfaction feeds valence directly (existing mechanism, different source):

```rust
// Per frame:
let sat = organism_satisfaction(metabolic, harmonic, rhythmic, nutrient);
let valence_target = sat * 2.0 - 1.0;  // Map [0,1] → [-1,1]
org.valence = lerp(org.valence, valence_target, 0.05);  // Smooth convergence
```

High satisfaction → positive valence → Hebbian learning strengthens current connections.
Low satisfaction → negative valence → connections weaken → arousal rises → exploration.

The cycle: eat → produce → evaluate → reinforce or explore. The organism doesn't judge its food. It judges its cooking.

---

## What Gets Removed

- `port_satisfaction()` per-port input evaluation
- Signal type matching satisfaction (Float, Trigger, etc.)
- "Delta impact" satisfaction term (input change → satisfaction)
- Input-matching dissatisfaction as exploration driver

## What Stays

- `dw = LR × eligibility × valence × satisfaction` Hebbian formula
- Satisfaction → valence → arousal feedback loop
- Ledger ring buffer recording weight changes (explainability)
- Eligibility trace decay
- The principle: satisfaction drives learning, dissatisfaction drives exploration

---

## Critical Files

| File | Change |
|------|--------|
| `src/affinity/graph.rs` | Read organism_satisfaction() instead of port_satisfaction() for Hebbian |
| `src/organism/module/mod.rs` | Add cached_satisfaction, computed from 4 sources each frame |
| `src/app.rs` | Compute metabolic/harmonic/rhythmic/nutrient satisfaction per organism |
| `src/module/mod.rs` | Deprecate port_satisfaction() default impl |
| `src/tuning/harmony.rs` | histogram_consonance() already specified in S40-rewrite |

---

## Verification

1. Well-fed organism producing consumed pitch class → satisfaction near 1.0 → positive valence
2. Starving organism → nutrient satisfaction drops → valence goes negative → arousal spikes → wanders
3. Two organisms eating same substrate → harmonic satisfaction high → affinity strengthens → they stay together
4. Organism forced onto depleted substrate → metabolic satisfaction drops → explores for richer ground
5. Phase-aligned neighbors → rhythmic satisfaction boost → temporal coordination emerges
6. Hebbian learning: organism finds productive substrate + good neighbors → connections strengthen → stable herd
