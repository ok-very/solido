# S02 — Tuning Core

> Parse .scl files. Convert scale degrees to Hz. Ship 9 built-in scales.

## Goal

Build the `TuningSystem` that everything else quantizes against.
This is pure math — no audio, no rendering, no threads.
Unit-testable in isolation.

## Ancestry (MAKE A BABY)

The Max/MSP patch stored raga scale data in `coll` objects
(`ragaraga`, `origaraga`, `origaraga_flute2`) — lookup tables
mapping step indices to pitch values. We replace that with the
universal `.scl` format so any microtonal scale is loadable.

## Tasks

### 2.1 Create `src/tuning/mod.rs` + `src/tuning/scala.rs`

Core types:

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

### 2.2 Scala parser

`TuningSystem::from_scl(source: &str) -> Result<Self, ScalaParseError>`

The `.scl` format:
- Lines starting with `!` are comments
- First non-comment line: description
- Second: count of scale degrees (integer)
- Remaining lines: one degree each, either:
  - A decimal number → cents (e.g., `386.31`)
  - A fraction → ratio (e.g., `5/4` or `2/1`)
- Inline comments after `!` on degree lines

### 2.3 `degree_to_hz`

```rust
pub fn degree_to_hz(&self, degree: usize, octave: i32, root_hz: f64) -> f64
```

- `degree` indexes into `self.cents` (0 = root)
- `octave` shifts by the scale's period (usually 1200 cents, but Bohlen-Pierce uses ~1902)
- `root_hz` is the tuning reference (default: 261.63 = C4)

### 2.4 `nearest_degree`

```rust
pub fn nearest_degree(&self, cents: f64) -> (usize, f64)
// Returns (degree_index, distance_in_cents)
```

Core utility for the gravity engine (S04).

### 2.5 Ship built-in scales as `assets/scales/`

Embed as `include_str!` constants. Each `.scl` file:

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

### 2.6 `TuningRegistry`

```rust
pub struct TuningRegistry {
    systems: HashMap<String, TuningSystem>,
}
```

- `load_builtins()` — parse all embedded `.scl` strings
- `get(name: &str) -> Option<&TuningSystem>`
- `list() -> Vec<&str>` — for UI dropdown (S09)

## Files Created

```
src/tuning/mod.rs       — pub mod scala; TuningRegistry
src/tuning/scala.rs     — ScalaDegree, TuningSystem, from_scl, degree_to_hz
assets/scales/*.scl     — 9 scale files
```

## Files Modified

```
src/main.rs             — add `mod tuning;`
```

## Verification

1. `cargo test` — unit tests pass:
   - Parse `bhairav.scl` → 7 degrees, correct cents values
   - Parse ratio (`5/4`) → 386.31 cents (±0.01)
   - `degree_to_hz(4, 0, 261.63)` for Bhairav = Pa = ~392Hz (just 3/2)
   - `nearest_degree` finds correct degree for arbitrary cent values
   - Parse all 9 built-in scales without error
   - Bohlen-Pierce period = ~1901.96 cents (not 1200)
2. No audio needed — this is pure math

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
