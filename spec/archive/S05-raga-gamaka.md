# S05 — Raga Modes + Gamaka

> Scales have rules. Notes have ornaments. Ragas have souls.

## Goal

Layer raga-specific musical rules on top of the gravity engine:
ascending/descending note sequences, stressed notes (vadi/samvadi),
per-degree gravity weights, and gamaka (microtonal ornaments).

## Ancestry (MAKE A BABY)

The Max/MSP patch had `ragaraga` and `origaraga` coll data files —
just pitch sequences, no ascending/descending distinction. We add
the full raga framework: aroha (ascending), avaroha (descending),
vadi (primary stressed note), samvadi (secondary), and gamaka
(the slides and oscillations that make raga music breathe).

The `mods_simple_bow` abstraction in MAKE A BABY was essentially
a gamaka engine: LFO-based modulation with feedback, creating
bow-like articulation. We formalize that as `GamakaState`.

## Depends On

- S02 (TuningSystem)
- S04 (PitchGravity, degree_weights)

## Tasks

### 5.1 Create `src/tuning/raga.rs`

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct RagaMode {
    pub name: String,
    pub tuning: String,           // references TuningSystem by name
    pub aroha: Vec<u8>,           // ascending degree indices
    pub avaroha: Vec<u8>,         // descending degree indices
    pub vadi: u8,                 // primary stressed degree
    pub samvadi: u8,              // secondary stressed degree
    pub gravity_weights: Vec<f32>, // per-degree pull strength [0.5–2.0]
    pub hue: f32,                 // characteristic color (for blob tint, S08)
}
```

### 5.2 Ship raga YAML definitions

`assets/ragas/`:

```yaml
# bhairav.yaml
name: Bhairav
tuning: bhairav
aroha: [0, 1, 2, 3, 4, 5, 6, 7]
avaroha: [7, 6, 5, 4, 3, 2, 1, 0]
vadi: 3        # Ma
samvadi: 6     # Ni
gravity_weights: [1.0, 1.5, 0.8, 2.0, 1.2, 1.5, 0.8, 1.0]
hue: 230.0     # deep blue
```

Ship 5 ragas: bhairav, bhairavi, yaman, jog, kafi.

Jog is the secret weapon — deliberately weak gravity_weights:
```yaml
# jog.yaml — "always searching, never arriving"
gravity_weights: [1.0, 0.5, 0.6, 0.7, 0.5, 0.4, 0.6, 1.0]
hue: 160.0     # teal-green
```

### 5.3 `RagaRegistry`

```rust
pub struct RagaRegistry {
    modes: HashMap<String, RagaMode>,
}
```

- `load_builtins()` — parse embedded YAML
- `get(name) -> Option<&RagaMode>`
- `list() -> Vec<&str>`

### 5.4 Wire raga weights into PitchGravity

When a raga is active:
- `PitchGravity.degree_weights` = `raga.gravity_weights`
- Vadi and samvadi get extra gravity (already encoded in weights)
- Direction tracking: detect if pitch is ascending/descending
  and use aroha/avaroha to restrict available degrees

Direction detection:
```rust
pub struct DirectionTracker {
    last_cents: f64,
    direction: Direction, // Ascending | Descending | Static
    hysteresis: f64,      // cents threshold before direction flips
}
```

### 5.5 Create `src/tuning/gamaka.rs`

Gamakas are post-gravity ornaments: slides, vibrato, bends.

```rust
pub struct GamakaConfig {
    pub slide_time_ms: f32,        // portamento duration
    pub vibrato_depth_cents: f32,  // vibrato amplitude
    pub vibrato_rate_hz: f32,      // vibrato speed
}

pub enum GamakaState {
    Idle,
    Sliding { from_hz: f64, to_hz: f64, progress: f32 },
    Vibrating { center_hz: f64, phase: f32 },
}
```

- Applied as post-processing on `PitchGravity::quantize` output
- `slide_time_ms` driven by arousal (higher = faster slides)
- `vibrato_depth` driven by uncertainty (more uncertainty = more wobble)
- For now: hardcoded config. S08 wires to emotion/gravity state.

### 5.6 Create `src/tuning/scale_morph.rs`

When switching ragas, don't hard-cut — interpolate gravity weights:

```rust
pub struct ScaleMorph {
    from_weights: Vec<f32>,
    to_weights: Vec<f32>,
    t: f32,           // 0.0 → 1.0
    morph_blocks: u32, // how many update ticks to complete
}
```

- `advance(speed: f32)` — increment t
- `current_weights() -> Vec<f32>` — lerp between from and to
- Triggered when raga changes (via UI or automation)

## Files Created

```
src/tuning/raga.rs         — RagaMode, RagaRegistry, DirectionTracker
src/tuning/gamaka.rs       — GamakaConfig, GamakaState
src/tuning/scale_morph.rs  — ScaleMorph
assets/ragas/*.yaml        — 5 raga definitions
```

## Files Modified

```
src/tuning/mod.rs          — pub mod raga, gamaka, scale_morph;
src/tuning/pitch_gravity.rs — accept RagaMode, use degree_weights
Cargo.toml                 — add serde_yaml
```

## Verification

1. Load Bhairav raga → gravity weights applied → Ma (vadi) pulls hardest
2. Load Jog raga → much weaker pull, pitch drifts freely
3. Switch Bhairav → Yaman → hear gravity weights morph over ~2 seconds
4. Gamaka vibrato audible on sustained notes
5. Gamaka slide audible when jumping between distant degrees
6. Direction tracker: ascending run uses aroha degrees only
7. All 5 raga YAMLs parse without error
