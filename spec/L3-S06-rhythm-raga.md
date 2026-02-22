# L3-S06 — Rhythm + Raga Modules

> Beats have gravity too. Scales have rules. Notes have ornaments.

## Goal

Build TalaGrid (rhythm gravity) and RagaMode (raga rules + gamaka
ornaments), each as separate Modules routed through the affinity graph.
TalaModule generates rhythmic triggers that route to VoiceModule.
RagaModule feeds gravity_weights to QuantizerModule. Both participate
in the texture↔music continuum: when arousal rises, triggers loosen
and gamaka deepens.

## Ancestry (MAKE A BABY)

The Max/MSP patch used `counter` + `metro` to step through scale
degrees — a fixed sequencer clock. The tala grid replaces this with
a gravity-based approach: triggers can arrive at any time, but the
tala's stressed beats attract them, creating groove from chaos.

The `ragaraga` and `origaraga` coll data files stored pitch sequences
with no ascending/descending distinction. We add the full raga
framework: aroha, avaroha, vadi, samvadi, and gamaka. The
`mods_simple_bow` abstraction was essentially a gamaka engine: LFO-based
modulation with feedback, creating bow-like articulation.

## Depends On

- L0-S01 (Module trait)
- L1-S02 (SeedReactor)
- L3-S04 (TuningSystem, PitchGravity — QuantizerModule receives raga weights)
- L4-S05 (VoiceModule — receives tala triggers)

## Tasks

### 6.1 Create `src/tuning/rhythm_gravity.rs`

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct TalaDefinition {
    pub name: String,
    pub beats: u32,
    pub divisions: Vec<u32>,     // vibhag structure
    pub stressed: Vec<u32>,       // beat indices with emphasis
    pub weights: Vec<f32>,        // per-beat gravity pull
}

pub struct TalaGrid {
    pub tala: TalaDefinition,
    pub tempo_bpm: f64,
    pub gravity: f32,            // 0.0=free, 1.0=strict grid snap
    pub swing: f32,              // micro-delay on even subdivisions [0.0–0.5]
    pub phase: f64,              // current position in cycle [0, beats)
}
```

### 6.2 Core quantize algorithm

```rust
pub fn quantize_trigger(
    &self,
    raw_time_beats: f64,  // position in beat-space
) -> f64                  // quantized position
```

1. Find nearest beat with gravity weights
2. Compute weighted distance: `dist / weight`
3. Apply gravity pull (same cubic curve as pitch)
4. Add swing offset for even subdivisions

### 6.3 Beat clock

```rust
pub fn advance(&mut self, delta_seconds: f64)
// Advances phase by (tempo_bpm / 60.0) * delta_seconds
```

- Wraps at `tala.beats`
- Emits `BeatEvent` at each beat crossing:
  ```rust
  pub struct BeatEvent {
      pub beat_index: u32,
      pub weight: f32,
      pub is_sam: bool,  // first beat of cycle (strongest)
  }
  ```

### 6.4 Euclidean rhythm generator

```rust
pub fn euclidean_rhythm(n_hits: usize, n_slots: usize) -> Vec<bool>
```

Bjorklund algorithm: distribute N hits across K slots as evenly
as possible. This is the mathematical basis of many world rhythms:
- E(3,8) = tresillo [x..x..x.] (Cuban)
- E(5,8) = cinquillo [x.xx.xx.] (West African)
- E(7,16) = [x.x.xx.x.x.xx.x.] (Brazilian)

### 6.5 Ship 5 tala YAML definitions

`assets/tala/`:

```yaml
# teentaal.yaml — 16 beats, standard Hindustani
name: Teentaal
beats: 16
divisions: [4, 4, 4, 4]
stressed: [0, 4, 8, 12]
weights: [2.0, 0.8, 1.5, 0.8, 0.7, 0.5, 0.7, 0.5,
          1.2, 0.6, 1.0, 0.6, 0.7, 0.5, 0.7, 0.5]
```

Ship: teentaal (16), rupak (7), jhaptal (10), dadra (6),
freeform (gravity=0 always).

### 6.6 `TalaRegistry`

Same pattern as TuningRegistry:
- `load_builtins()`
- `get(name)`, `list()`

### 6.7 Create `src/modules/tala_module.rs` — TalaModule

```rust
pub struct TalaModule {
    schema: ModuleSchema,
    grid: TalaGrid,
    tala_registry: TalaRegistry,
    euclidean_pattern: Vec<bool>,
    euclidean_hits: u32,
}
```

**Schema**:
- Inputs:
  - `tempo_delta` (Float, Event) — from KeyboardInputModule
  - `gravity_override` (Float, Block) — manual rhythm gravity
  - `euclidean_hits` (Float, Block) — pattern density
- Outputs:
  - `beat_trigger` (Trigger, Event) — fires on each active beat
  - `beat_phase` (Float, Block) — 0.0-1.0 within current beat
  - `beat_weight` (Float, Event) — stress weight of current beat
  - `is_sam` (Bool, Event) — first beat of cycle

**Custom UI panel**:
- Tala dropdown selector
- Tempo slider 40-200 bpm
- Swing slider 0.0-0.5
- Euclidean hits slider 0-beats
- Beat position visualizer: row of dots, current beat highlighted

### 6.8 Create `src/tuning/raga.rs` — RagaMode

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
    pub hue: f32,                 // characteristic color for blob tint
}
```

### 6.9 Ship 5 raga YAML definitions

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

Ship: bhairav, bhairavi, yaman, jog, kafi.

Jog is the secret weapon — deliberately weak gravity_weights:
```yaml
# jog.yaml — "always searching, never arriving"
gravity_weights: [1.0, 0.5, 0.6, 0.7, 0.5, 0.4, 0.6, 1.0]
hue: 160.0     # teal-green
```

### 6.10 `RagaRegistry`

Same pattern:
- `load_builtins()`, `get(name)`, `list()`

### 6.11 Create `src/tuning/gamaka.rs` — ornaments

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

- Applied as post-processing on PitchGravity::quantize output
- `slide_time_ms` driven by arousal (higher = faster slides)
- `vibrato_depth` driven by uncertainty (more uncertainty = more wobble)

### 6.12 Create `src/tuning/scale_morph.rs` — raga transitions

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
- Triggered when raga changes

### 6.13 Direction tracker

```rust
pub struct DirectionTracker {
    last_cents: f64,
    direction: Direction, // Ascending | Descending | Static
    hysteresis: f64,      // cents threshold before direction flips
}
```

Detects if pitch is ascending/descending and uses aroha/avaroha
to restrict available degrees.

### 6.14 Create `src/modules/raga_module.rs` — RagaModule

```rust
pub struct RagaModule {
    schema: ModuleSchema,
    raga_registry: RagaRegistry,
    current_raga: String,
    morph: Option<ScaleMorph>,
    gamaka: GamakaConfig,
    direction: DirectionTracker,
}
```

**Schema**:
- Inputs:
  - `raga_cycle` (Trigger, Event) — from KeyboardInputModule (R key)
  - `morph_target` (Text, Event) — raga name to morph to
  - `arousal` (Float, Block) — drives gamaka depth
- Outputs:
  - `gravity_weights` (Pattern, Block) — per-degree pull strengths
  - `gamaka_config` (Pattern, Block) — [slide_ms, vib_depth, vib_rate]
  - `raga_hue` (Float, Block) — color hint for blob renderer

**Custom UI panel**:
- Raga dropdown selector
- Gamaka depth slider
- Morph speed slider
- Current degree display with aroha/avaroha indicators

### 6.15 Add dependency

```toml
serde_yaml = "0.9"
```

## Files Created

```
src/tuning/rhythm_gravity.rs      — TalaGrid, TalaDefinition, euclidean_rhythm
src/tuning/raga.rs                — RagaMode, RagaRegistry, DirectionTracker
src/tuning/gamaka.rs              — GamakaConfig, GamakaState
src/tuning/scale_morph.rs         — ScaleMorph
src/modules/tala_module.rs        — TalaModule (Module impl)
src/modules/raga_module.rs        — RagaModule (Module impl)
assets/tala/*.yaml                — 5 tala definitions
assets/ragas/*.yaml               — 5 raga definitions
```

## Files Modified

```
src/tuning/mod.rs                 — pub mod rhythm_gravity, raga, gamaka, scale_morph;
src/modules/mod.rs                — add pub mod tala_module, raga_module;
src/app.rs                        — register TalaModule + RagaModule with SeedReactor
Cargo.toml                        — add serde_yaml
```

## Verification

1. Teentaal at 120bpm: hear 16-beat cycle with clear sam emphasis
2. Gravity=1.0: triggers snap perfectly to grid
3. Gravity=0.0: triggers fire at random times
4. Gravity=0.5: triggers pulled toward beats but with human feel
5. Swing=0.3: audible shuffle on even subdivisions
6. Euclidean E(5,16): hear the West African pattern
7. Rupak (7 beats): asymmetric cycle audible (3+2+2 grouping)
8. Freeform tala: no rhythmic pull at all (texture mode)
9. Load Bhairav raga → gravity weights applied → Ma (vadi) pulls hardest
10. Load Jog raga → much weaker pull, pitch drifts freely
11. Switch Bhairav → Yaman → hear gravity weights morph over ~2 seconds
12. Gamaka vibrato audible on sustained notes
13. Direction tracker: ascending run uses aroha degrees only
14. All 5 raga YAMLs and 5 tala YAMLs parse without error
