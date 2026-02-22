# L3-S04 — Tuning + Gravity Core

> "Not a scale. A gravitational field."

## Goal

Build the Scala parser, TuningSystem, and PitchGravity algorithm, all
wrapped as a QuantizerModule that receives `raw_pitch` (Float) through
the affinity graph and emits `pitch_hz` (Float). This is the first
processing module — it proves input→processing→output routing.

## Ancestry (MAKE A BABY)

The Max/MSP patch stored raga scale data in `coll` objects (`ragaraga`,
`origaraga`, `origaraga_flute2`) — lookup tables mapping step indices
to pitch values. Hard snap, no interpolation. This is the upgrade:
continuous gravity pull with per-degree weighting, so some notes attract
harder than others (vadi/samvadi in raga theory).

## Depends On

- L0-S01 (Module trait, Signal types)
- L1-S02 (SeedReactor — QuantizerModule registers with it)
- L2-S03 (KeyboardInputModule — source of raw_pitch signals)

## Tasks

### 4.1 Create `src/tuning/scala.rs` — .scl parser

```rust
pub enum ScalaDegree {
    Cents(f64),
    Ratio(u64, u64),
}

pub struct TuningSystem {
    pub name: String,
    pub description: String,
    pub degrees: Vec<ScalaDegree>,  // excludes root, includes octave/period
    pub cents: Vec<f64>,            // cached, sorted, includes 0.0 at root
}
```

**Scala parser**: `TuningSystem::from_scl(source: &str) -> Result<Self, ScalaParseError>`

The `.scl` format:
- Lines starting with `!` are comments
- First non-comment line: description
- Second: count of scale degrees (integer)
- Remaining lines: one degree each, either:
  - A decimal number → cents (e.g., `386.31`)
  - A fraction → ratio (e.g., `5/4` or `2/1`)
- Inline comments after `!` on degree lines

### 4.2 `degree_to_hz`

```rust
pub fn degree_to_hz(&self, degree: usize, octave: i32, root_hz: f64) -> f64
```

- `degree` indexes into `self.cents` (0 = root)
- `octave` shifts by the scale's period (usually 1200 cents, but Bohlen-Pierce uses ~1902)
- `root_hz` is the tuning reference (default: 261.63 = C4)

### 4.3 `nearest_degree`

```rust
pub fn nearest_degree(&self, cents: f64) -> (usize, f64)
// Returns (degree_index, distance_in_cents)
```

Core utility for the gravity engine.

### 4.4 Ship 9 built-in scales as `assets/scales/`

Embed as `include_str!` constants:

| File | Scale | Notes |
|------|-------|-------|
| `12tet.scl` | 12-tone equal temperament | Western standard, baseline |
| `5limit_ji.scl` | 5-limit just intonation | Pure ratios: 9/8, 5/4, 4/3, 3/2, 5/3, 15/8, 2/1 |
| `bhairav.scl` | Bhairav raga | Dark morning: komal Re + Dha |
| `bhairavi.scl` | Bhairavi raga | All komal svaras, melancholic |
| `yaman.scl` | Yaman raga | Evening, teevra Ma, golden |
| `jog.scl` | Jog raga | Ambiguous, weak gravity — best for texture |
| `slendro.scl` | Javanese Slendro | 5-note, ~240 cents spacing |
| `pelog.scl` | Javanese Pelog | 7-note asymmetric, haunting |
| `bohlen_pierce.scl` | Bohlen-Pierce | Tritave (3/1), non-octave, alien |

### 4.5 `TuningRegistry`

```rust
pub struct TuningRegistry {
    systems: HashMap<String, TuningSystem>,
}
```

- `load_builtins()` — parse all embedded `.scl` strings
- `get(name: &str) -> Option<&TuningSystem>`
- `list() -> Vec<&str>` — for UI dropdown

### 4.6 Create `src/tuning/pitch_gravity.rs` — gravity quantization

```rust
pub struct PitchGravity {
    pub tuning: TuningSystem,
    pub root_hz: f64,             // default: 261.63 (C4)
    pub gravity: f32,             // 0.0=free, 1.0=hard snap
    pub octave_range: (i32, i32), // e.g. (-1, 2)
    pub degree_weights: Vec<f32>, // per-degree pull multiplier
}
```

### 4.7 Core `quantize` algorithm

```rust
pub fn quantize(&self, raw_pitch: f32) -> f64  // returns Hz
```

1. Map `raw_pitch` [0,1] to cents across octave range
2. Find nearest scale degree (weighted by `degree_weights`)
3. Apply cubic pull curve: **`pull = d * |d|^(1 + gravity)`**
4. Subtract pull from raw position
5. Convert final cents to Hz

The cubic curve is the key insight:
- Notes close to a degree snap cleanly
- Notes far from any degree slide gracefully
- `gravity` parameter controls the exponent

### 4.8 Weighted nearest-degree search

```rust
fn find_nearest_weighted(&self, cents: f64) -> NearestDegree
```

- Fold input cents into one period (rem_euclid)
- For each scale degree: `effective_distance = |cents - degree| / weight`
- High-weight degrees (vadi) appear closer, attract harder
- Return: degree cents (restored to original octave), weight

### 4.9 PitchSmoother

```rust
pub struct PitchSmoother {
    current_hz: f64,
    target_hz: f64,
    slew_rate: f64,  // Hz per second
}
```

Continuous pitch output — called at block rate (~60Hz).
Smooths discrete quantize() jumps into portamento glides.

### 4.10 Create `src/modules/quantizer.rs` — QuantizerModule

```rust
pub struct QuantizerModule {
    schema: ModuleSchema,
    gravity: PitchGravity,
    smoother: PitchSmoother,
    tuning_registry: TuningRegistry,
    current_tuning: String,
    override_gravity: Option<f32>,
}
```

**Schema**:
- Inputs:
  - `raw_pitch` (Float, Event) — from KeyboardInputModule
  - `gravity_override` (Float, Block) — manual gravity control
- Outputs:
  - `pitch_hz` (Float, Block) — quantized pitch in Hz
  - `nearest_degree` (Float, Block) — index of nearest scale degree

**Custom UI panel** (Tiered UI):
- Gravity slider 0.0-1.0
- Current tuning dropdown
- Root Hz slider 200-520
- Display: nearest degree name + cents distance + Hz

The module registers with SeedReactor. When KeyboardInputModule emits
`raw_pitch`, the affinity edge routes it to QuantizerModule, which
emits `pitch_hz`. Gravity is driven by the module's own emotion state
(via GravityState from S09) OR by the `gravity_override` input port.

## Reference Values

For `bhairav.scl` with root = C4 (261.63 Hz):

| Degree | Svara | Cents | Hz |
|--------|-------|-------|----|
| 0 | Sa | 0.00 | 261.63 |
| 1 | komal Re | 112.00 | 279.07 |
| 2 | Ga | 386.31 | 327.03 |
| 3 | Ma | 498.04 | 348.83 |
| 4 | Pa | 701.96 | 392.44 |
| 5 | komal Dha | 813.69 | 418.60 |
| 6 | Ni | 1088.27 | 490.55 |
| 7 | Sa' | 1200.00 | 523.25 |

## Files Created

```
src/tuning/mod.rs              — pub mod scala, pitch_gravity; TuningRegistry
src/tuning/scala.rs            — ScalaDegree, TuningSystem, from_scl, degree_to_hz
src/tuning/pitch_gravity.rs    — PitchGravity, quantize, PitchSmoother
src/modules/quantizer.rs       — QuantizerModule (Module impl)
assets/scales/*.scl            — 9 scale files
```

## Files Modified

```
src/main.rs                    — add `mod tuning;`
src/modules/mod.rs             — add pub mod quantizer;
src/app.rs                     — register QuantizerModule with SeedReactor
```

## Verification

1. `cargo test` — Parse `bhairav.scl` → 7 degrees, correct cents values
2. `cargo test` — Parse ratio (`5/4`) → 386.31 cents (±0.01)
3. `cargo test` — `degree_to_hz(4, 0, 261.63)` for Bhairav = Pa = ~392Hz
4. `cargo test` — `nearest_degree` finds correct degree for arbitrary cents
5. `cargo test` — Parse all 9 built-in scales without error
6. `cargo test` — Bohlen-Pierce period = ~1901.96 cents (not 1200)
7. `cargo run` — keyboard input → quantizer → debug log shows Hz values
   pulled toward Bhairav scale degrees
8. Gravity=0: debug log shows continuous pitch values (no snapping)
9. Gravity=1: debug log shows exact scale degree Hz values
10. Gravity=0.5: debug log shows values pulled toward but not snapped to degrees

## The Feel

At gravity ~0.3, moving the mouse slowly should feel like bowing
a fretless string instrument — the pitch slides but gets caught
momentarily at each scale degree. This is the core aesthetic of
the entire system.
