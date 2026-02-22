# S06 — Rhythm Gravity

> Beats have gravity too. The tala grid pulls triggers into groove.

## Goal

Build a rhythmic quantization layer that works the same way as
pitch gravity: raw trigger times get pulled toward beat positions
with tunable strength. Strong gravity = locked groove. Zero gravity
= free rhythm. In between: human-feeling swing.

## Ancestry (MAKE A BABY)

The Max/MSP patch used `counter` + `metro` to step through scale
degrees — a fixed sequencer clock. The tala grid replaces this with
a gravity-based approach: triggers can arrive at any time, but the
tala's stressed beats attract them, creating groove from chaos.

## Depends On

- S01 (AudioEngine, for trigger delivery)
- S03 (VoicePool, for triggering notes)

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
      pub weight: f32,  // stress weight of this beat
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

- `n_hits` can be seeded from external control (0.0–1.0 → 0–beats)
- Pattern rotates by an offset to create variation

### 6.5 Ship tala YAML definitions

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

Ship 5 talas: teentaal (16), rupak (7), jhaptal (10),
dadra (6), freeform (gravity=0 always).

### 6.6 `TalaRegistry`

Same pattern as TuningRegistry/RagaRegistry:
- `load_builtins()`
- `get(name)`, `list()`

### 6.7 Wire into audio for testing

Temporary test mode in `app.rs`:
- TalaGrid runs on UI thread, advances each frame
- On each beat: spawn a voice at a random Bhairav degree
- Gravity slider controls how tightly triggers lock to beats
- Swing slider adds shuffle feel
- Euclidean pattern generator: slider controls density (n_hits)

## Files Created

```
src/tuning/rhythm_gravity.rs  — TalaGrid, TalaDefinition, euclidean_rhythm
assets/tala/*.yaml             — 5 tala definitions
```

## Files Modified

```
src/tuning/mod.rs             — pub mod rhythm_gravity; TalaRegistry
src/app.rs                    — beat clock test, euclidean pattern
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
9. Change tempo via egui slider: beat clock adjusts smoothly

## Design Notes

The rhythm gravity engine runs on the control thread, not the
audio thread. It produces quantized trigger times that get sent
to the audio thread via the ring buffer. The audio thread never
does rhythm math — it just receives "play note at sample X"
commands.
