# Audit Report: L3-S04 Tuning & Gravity

**Date:** 2026-02-22
**Status:** Pre-Implementation Review
**Component:** Tuning System, Gravity Algorithm, Quantizer Module

## Executive Summary

The L3-S04 specification introduces the core musical intelligence of the Solido system. The architecture cleanly separates the static tuning data (`Scala/TuningSystem`), the algorithmic logic (`PitchGravity`), and the reactor integration (`QuantizerModule`). 

However, there is a critical mathematical flaw in the proposed gravity formula that will cause massive pitch instability, and a few minor edge cases in parsing and signal smoothing that should be addressed during implementation.

---

## 1. Algorithmic Integrity

### 1.1 The Gravity Formula (CRITICAL)
- **Finding**: The spec defines the cubic pull curve as: `pull = d * |d|^(1 + gravity)` where `d` is presumably the distance in cents to the nearest degree.
- **Risk**: If `d` is in cents (e.g., 50 cents away from a note), and `gravity = 1.0`, the pull becomes $50 	imes 50^{2} = 125,000$ cents. Subtracting this from the raw pitch will violently throw the output thousands of octaves away, resulting in `NaN` or audio driver crashes downstream.
- **Recommendation**: The distance `d` **must be normalized** to a range of `[-1.0, 1.0]` before applying the polynomial curve. 
  1. Find the nearest degree.
  2. Find the boundaries (midpoints) between the nearest degree and its neighbors.
  3. Calculate `normalized_d` as the fraction of the distance from the degree to the boundary (range `[-1.0, 1.0]`).
  4. Apply the curve: `pull_norm = normalized_d * |normalized_d|^(gravity)`. (Notice it's `^gravity`, not `1+gravity`, so `gravity=0` yields linear $x$, `gravity=1` yields $x|x|$ or $x^2$ shaping, etc).
  5. Scale the `pull_norm` back to cents and subtract it from the raw position.

### 1.2 Pitch Smoothing Domain
- **Finding**: `PitchSmoother` is described as smoothing discrete `quantize()` jumps. It tracks `current_hz` and `target_hz`.
- **Risk**: Linear slew on Hz values sounds unnatural. A glide from 100Hz to 200Hz (1 octave) takes the same time as a glide from 1000Hz to 1100Hz (~1.5 semitones) if slew is constant Hz/sec. 
- **Recommendation**: `PitchSmoother` should smooth in the logarithmic domain (either tracking `cents` or a normalized `[0, 1]` pitch value) and only convert to `Hz` at the very end of the `tick()` right before emission.

## 2. Parsing & Data Structures

### 2.1 Scala `.scl` Parser Edge Cases
- **Finding**: The spec notes that Scala lines are either "a decimal number → cents" or "a fraction → ratio". 
- **Risk**: The official Scala specification allows a single integer (e.g., `2`) to represent a ratio (e.g., `2/1`). If the parser strictly looks for a `/` for ratios and `.` for cents, it will fail to parse standard files like `12tet.scl` if they use `2` for the octave.
- **Recommendation**: Implement the parsing logic as:
  - If it contains a `.`, it is Cents.
  - Otherwise (whether it contains a `/` or is just an integer), it is a Ratio. If no `/` is present, assume the denominator is `1`.

## 3. Module Integration

### 3.1 Rate Conversion (`Event` to `Block`)
- **Finding**: `KeyboardInputModule` emits `raw_pitch` at `Event` rate (only when a key is pressed). `QuantizerModule` emits `pitch_hz` at `Block` rate (~60Hz).
- **Note for Implementation**: The `QuantizerModule` must store the last received `raw_pitch` internally. On every `tick(dt)`, it runs the smoother toward the target pitch and updates the internal `current_hz`. On every `emit_signals()`, it pushes the current smoothed `pitch_hz`. This correctly bridges the sporadic input to a continuous control signal for downstream audio modules.

---

## Conclusion

The structural design is excellent. The primary intervention required is the correction of the gravity math (normalizing distance before exponentiation) to prevent catastrophic numerical explosion. Once the math and the Scala parser integer rule are accounted for, implementation can proceed safely.