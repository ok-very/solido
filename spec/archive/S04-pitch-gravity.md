# S04 — Pitch Gravity

> "Not a scale. A gravitational field."

## Goal

Build the core gravity quantization algorithm.
Raw pitch values (0.0–1.0) get pulled toward scale degrees
with tunable strength. At gravity=1.0 it's a hard quantizer.
At gravity=0.0 it's free chromatic drift. In between: music.

## Ancestry (MAKE A BABY)

The Max/MSP patch used `coll` lookup tables for scale-degree
quantization — hard snap, no interpolation. This is the upgrade:
continuous gravity pull with per-degree weighting, so some notes
attract harder than others (vadi/samvadi in raga theory).

## Depends On

- S02 (TuningSystem, nearest_degree)
- S03 (VoicePool, for audible testing)

## Tasks

### 4.1 Create `src/tuning/pitch_gravity.rs`

```rust
pub struct PitchGravity {
    pub tuning: TuningSystem,
    pub root_hz: f64,           // default: 261.63 (C4)
    pub gravity: f32,           // 0.0=free, 1.0=hard snap
    pub octave_range: (i32, i32), // e.g. (-1, 2)
    pub degree_weights: Vec<f32>, // per-degree pull multiplier
}
```

### 4.2 Core `quantize` algorithm

```rust
pub fn quantize(&self, raw_pitch: f32) -> f64 // returns Hz
```

1. Map `raw_pitch` [0,1] to cents across octave range
2. Find nearest scale degree (weighted by `degree_weights`)
3. Apply cubic pull curve: `pull = d * |d|^(1 + gravity)`
4. Subtract pull from raw position
5. Convert final cents to Hz

The cubic curve is the key insight from the addendum:
- Notes close to a degree snap cleanly
- Notes far from any degree slide gracefully
- `gravity` parameter controls the exponent

### 4.3 Weighted nearest-degree search

```rust
fn find_nearest_weighted(&self, cents: f64) -> NearestDegree
```

- Fold input cents into one period (rem_euclid)
- For each scale degree: `effective_distance = |cents - degree| / weight`
- High-weight degrees (vadi) appear closer, attract harder
- Return: degree cents (restored to original octave), weight

### 4.4 Continuous pitch output

`quantize` is called at block rate (~60Hz from the UI loop).
Output feeds into `VoicePool::set_param(id, Frequency, hz)`.

For smooth results, add a simple portamento:
```rust
pub struct PitchSmoother {
    current_hz: f64,
    target_hz: f64,
    slew_rate: f64,  // Hz per second
}
```

### 4.5 Test harness

Wire into `app.rs` temporarily:
- Mouse X position → `raw_pitch` [0,1]
- `PitchGravity::quantize(raw_pitch)` → Hz
- Feed Hz to a voice
- Mouse Y → `gravity` [0,1]
- Display current Hz and nearest degree name in egui

This lets you hear the gravity algorithm directly:
move mouse left-right to sweep pitch, up-down to change gravity.

### 4.6 Default degree weights

For scales without raga metadata, all weights = 1.0.
The raga session (S05) adds per-degree weights.

## Files Created

```
src/tuning/pitch_gravity.rs  — PitchGravity, quantize, PitchSmoother
```

## Files Modified

```
src/tuning/mod.rs            — pub mod pitch_gravity;
src/app.rs                   — mouse → pitch gravity → voice (test)
```

## Verification

1. `cargo run` — mouse-driven pitch test
2. Mouse at left edge → lowest pitch in range; right edge → highest
3. Gravity = 0 (mouse at top): pitch sweeps continuously, no steps
4. Gravity = 1 (mouse at bottom): pitch snaps to exact scale degrees
5. Gravity = 0.5: pitch bends toward degrees but glides between
6. Bhairav scale: audibly hear the komal Re and komal Dha pull
7. Switch to `jog.scl` (weak gravity weights): drift feels looser
8. Switch to `5limit_ji.scl`: pure intervals lock magnetically

## The Feel

At gravity ~0.3, moving the mouse slowly should feel like bowing
a fretless string instrument — the pitch slides but gets caught
momentarily at each scale degree. This is the core aesthetic of
the entire system.
