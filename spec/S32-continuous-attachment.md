# S32 — Continuous Attachment

**Status**: Complete (Mar 2026)
**Depends on**: S30 (interaction tuning), S31 (transport — needs pause for tuning)
**Blocks**: organism-union.md (attachment is prerequisite to fusion)

---

## Goal

Replace the binary glob state (attached/not-attached) with a continuous attachment strength [0.0, 1.0] per organism pair. Organisms pull toward each other with logarithmic acceleration — slow approach, then rapid lock. Both audio and visual behavior respond dynamically to attachment level. This is the most musically expressive interaction state: two organisms gradually synchronizing timbre, rhythm, and pitch as they merge.

---

## Problem Statement

### Current: binary glob

```
affinity > 0.3 → globbed (SDF merge, collective physics)
affinity < 0.15 → unglobbed (independent)
```

No in-between. No gradual pull-in. No audio response to proximity. Two organisms are either strangers or merged — the interesting middle ground is missing.

### Desired: continuous gradient

```
attachment = 0.0 → strangers (independent orbit)
attachment = 0.1 → acquaintances (slightly tighter orbit)
attachment = 0.3 → friends (visible pull, shared reverb)
attachment = 0.5 → partners (rhythmic sync, timbral bleed)
attachment = 0.7 → bonded (tight orbit, merged filter, shared pitch gravity)
attachment = 0.9 → fused (visually merged, shared DSP characteristics)
attachment = 1.0 → union candidate (triggers organism-union if enabled)
```

---

## Attachment Curve

### Logarithmic pull-in formula

```rust
// Raw affinity [0, 1] → attachment strength [0, 1]
// Logarithmic curve: slow start, rapid convergence at high affinity
fn attachment_from_affinity(affinity: f32) -> f32 {
    let threshold = 0.15;  // Below this, no attachment
    if affinity < threshold { return 0.0; }

    let normalized = (affinity - threshold) / (1.0 - threshold);  // [0, 1]
    // Log curve: steep near 1.0, gentle near 0.0
    let log_attachment = (1.0 + normalized * 9.0).log10();  // log10(1..10) = [0, 1]
    log_attachment.clamp(0.0, 1.0)
}
```

**Mapping at key points:**

| Affinity | Normalized | Attachment | Behavior |
|----------|-----------|------------|----------|
| 0.0 | 0.0 | 0.0 | Strangers |
| 0.15 | 0.0 | 0.0 | Threshold — just noticed each other |
| 0.25 | 0.12 | 0.04 | Barely attached — marginal orbit tightening |
| 0.35 | 0.24 | 0.13 | Mild pull — orbits visibly closer |
| 0.50 | 0.41 | 0.28 | Moderate — audio starts responding |
| 0.65 | 0.59 | 0.44 | Strong — rhythmic/timbral bleed noticeable |
| 0.80 | 0.76 | 0.62 | Very strong — shared gravity field |
| 0.90 | 0.88 | 0.77 | Near-fused — visual merge beginning |
| 1.00 | 1.00 | 1.0 | Full union candidate |

### Why logarithmic?

Linear attachment feels mechanical. Logarithmic matches musical dynamics — **slow approach, then snap into lock**. Like two musicians gradually finding a groove, then suddenly locking in. The transition from "orbiting" to "locked" should feel like a phase transition, not a linear ramp.

---

## Physics: Continuous Pull Force

### Replace binary glob physics with continuous

```rust
// Current (binary):
if in_same_glob_group {
    apply_glob_physics(a, b);  // Full strength
}

// New (continuous):
let attachment = attachment_from_affinity(pairwise_affinity);
if attachment > 0.01 {
    // Orbit tightening: reduce orbit range proportionally
    let orbit_range = base_orbit_range * (1.0 - attachment * 0.6);
    // Pull force: scales with attachment²  (quadratic feels snappier)
    let pull_strength = attachment * attachment * max_pull * desire_avg;
    apply_continuous_pull(a, b, orbit_range, pull_strength);
    // Viscous damping: increases with attachment (prevents oscillation)
    let damping = attachment * 0.3;
    apply_relative_damping(a, b, damping);
}
```

### Orbit range compression

| Attachment | Orbit Range (HOSO, base=300) | Visual |
|------------|------------------------------|--------|
| 0.0 | 300 px | Normal orbit |
| 0.3 | 246 px | Noticeably tighter |
| 0.5 | 210 px | Close orbit |
| 0.7 | 174 px | Very close |
| 0.9 | 138 px | Nearly touching |
| 1.0 | 120 px | Overlapping territories |

### Remove binary glob physics

`refresh_glob_groups()` and `prev_glob_pairs` are removed. The continuous system replaces them entirely. Visual merging (SDF smin blending) is now driven by attachment level directly.

---

## Audio Response to Attachment

This is where the magic happens. As two organisms approach, their sound changes.

### A1. Shared reverb boost (attachment > 0.1)

Already partially implemented — `reverb_send_base + prox * 0.3`. Extend:

```rust
let reverb_boost = attachment * 0.4;  // Up to +40% reverb send
reverb_send.set((base + reverb_boost).min(1.0));
```

Creates shared space — organisms sound like they're in the same room.

### A2. Filter convergence (attachment > 0.3)

When two organisms have high attachment, their filter cutoffs drift toward each other:

```rust
// In organism module tick:
if let Some((partner_id, attachment)) = strongest_attachment {
    if attachment > 0.3 {
        let partner_centroid = partner_spectral_centroid;
        let my_centroid = my_spectral_centroid;
        let blend = (attachment - 0.3) * 1.43;  // [0, 1] over [0.3, 1.0]
        let target_centroid = lerp(my_centroid, partner_centroid, blend * 0.3);
        // Apply as filter cutoff modulation via DspCommand
    }
}
```

Timbral bleed — organisms start to sound alike.

### A3. Pitch gravity sharing (attachment > 0.5)

When strongly attached, organisms share pitch gravity — they tend toward the same notes:

```rust
if attachment > 0.5 {
    let blend = (attachment - 0.5) * 2.0;  // [0, 1] over [0.5, 1.0]
    // Partner's seq_pitch influences my pitch gravity
    // Applied via personality_transform_pitch blend factor
}
```

Musical convergence — organisms start playing related notes.

### A4. Tempo nudge (attachment > 0.5, requires S31 GlobalClock)

Organisms with different `tempo_ratio` values drift toward each other's tempo:

```rust
if attachment > 0.5 {
    let nudge = (attachment - 0.5) * 0.1;  // Up to 10% tempo shift
    let partner_ratio = partner.tempo_ratio;
    let my_ratio = self.tempo_ratio;
    let target_ratio = lerp(my_ratio, partner_ratio, nudge);
    // Gradual — takes 5-10 seconds to fully sync
}
```

Rhythmic convergence — polyrhythms slowly resolve into unison.

---

## Visual Response to Attachment

### V1. SDF blend radius (continuous)

Currently `smin_k` is a constant. Make it respond to attachment:

```rust
// In biofield.wgsl or CPU-side CellData:
let smin_k = base_smin_k * (1.0 + attachment * 2.0);
// attachment=0: normal territory boundaries
// attachment=1: territories merge smoothly
```

### V2. Hue interpolation (attachment > 0.3)

Organisms shift hue toward each other:

```rust
let hue_blend = (attachment - 0.3).max(0.0) * 0.3;  // Up to 30% hue shift
let blended_hue = lerp(my_hue, partner_hue, hue_blend);
```

Visual indication of growing affinity — colors drift.

### V3. Connection line / thread (attachment > 0.1)

Render a thin line between attached organisms, opacity proportional to attachment:

```rust
// In biofield.wgsl or overlay pass:
let line_alpha = attachment * 0.6;
// Line thickness: 1px at attachment=0.1, 3px at attachment=1.0
// Color: average of both organism hues
```

Visible thread of connection — the user can SEE the relationship.

### V4. Pulse sync (attachment > 0.5)

Both organisms' `pulse_response` modulates in sync:

```rust
let pulse_sync = (attachment - 0.5).max(0.0) * 2.0;
// Shared beat pulse from the stronger rhythmic organism
```

Visual rhythmic locking — organisms pulsate together.

---

## Data Model Changes

### OrganismState additions

```rust
pub struct OrganismState {
    // ... existing fields ...

    // NEW: per-pair attachment strengths
    // Not stored here — computed from pairwise_affinities in registry
}
```

### OrganismRegistry changes

```rust
pub struct OrganismRegistry {
    // REMOVE:
    // glob_on_threshold: f32,
    // glob_off_threshold: f32,
    // prev_glob_pairs: HashSet<(OrganismId, OrganismId)>,

    // KEEP:
    pub pairwise_affinities: HashMap<(OrganismId, OrganismId), f32>,

    // NEW:
    pub pairwise_attachments: HashMap<(OrganismId, OrganismId), f32>,
    // Computed from pairwise_affinities via logarithmic curve
    // Updated every tick, used by physics + audio + visual
}
```

### refresh_glob_groups() → compute_attachments()

```rust
fn compute_attachments(&mut self) {
    self.pairwise_attachments.clear();
    for (&(a, b), &affinity) in &self.pairwise_affinities {
        let attachment = attachment_from_affinity(affinity);
        if attachment > 0.01 {
            self.pairwise_attachments.insert((a, b), attachment);
        }
    }
}
```

### GPU data: attachment pairs

For visual rendering, pass attachment data to shader:

```rust
#[repr(C)]
pub struct AttachmentPair {
    pub pos_a: [f32; 2],
    pub pos_b: [f32; 2],
    pub attachment: f32,
    pub hue_a: f32,
    pub hue_b: f32,
    pub _pad: f32,
}
```

Storage buffer alongside CellData. Shader draws connection lines and blends territories.

---

## Interaction with organism-union.md

Continuous attachment **replaces** the binary glob prerequisite for union:

```
// Old (binary):
glob group + dwell > 5s + mutual consent → fusion

// New (continuous):
attachment > 0.9 + dwell > 5s + mutual consent → fusion candidate
```

Attachment naturally accumulates over time. The 0.9 threshold is high enough that organisms must genuinely interact musically (audio correlation, shared pitch gravity) before fusion is possible. This makes fusion feel earned, not accidental.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/organism/registry.rs` | Remove glob groups, add `pairwise_attachments`, `compute_attachments()`, continuous physics |
| `src/organism/interaction.rs` | `continuous_pull()` replacing `glob()`, orbit range compression |
| `src/organism/sim.rs` | Remove `glob_group: Option<u32>` field |
| `src/renderer/biofield_renderer.rs` | `AttachmentPair` buffer, connection lines |
| `src/renderer/biofield.wgsl` | SDF blend from attachment, connection line rendering |
| `src/app.rs` | Wire attachment data to audio modulation (reverb boost, filter convergence) |
| `src/organism/module.rs` | Receive attachment data, apply pitch/filter/tempo modulation |

## Verification

1. Two organisms slowly approaching: orbit tightens visibly
2. At attachment ~0.3: shared reverb noticeably increases
3. At attachment ~0.5: timbral similarity emerges (filter convergence)
4. At attachment ~0.7: visual territories start blending
5. At attachment ~0.9: nearly fused — visual and audio almost unified
6. Separating organisms: attachment decays over ~3 seconds, all effects reverse
7. Connection lines visible between all attached pairs, opacity tracks strength
