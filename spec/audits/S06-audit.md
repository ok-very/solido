# Audit Report: L3-S06 Rhythm + Raga Modules

**Date:** 2026-02-22
**Status:** Pre-Implementation Review
**Component:** TalaGrid, RagaMode, Gamaka, ScaleMorph, TalaModule, RagaModule

## Executive Summary

L3-S06 is the largest spec in the session map so far — 15 tasks, 6 new files, 2 new modules, 10+ new types, and a new crate dependency. The musical design is excellent but there are structural issues that will cause integration failures if not addressed: a missing receiver port on QuantizerModule, a hard dependency on an unbuilt session (L4-S05), and a YAML dependency that's been archived upstream.

---

## 1. Dependency Chain (BLOCKING)

### 1.1 L4-S05 VoiceModule doesn't exist yet
- **Finding**: The spec lists `L4-S05 (VoiceModule — receives tala triggers)` as a dependency. Verification items 1-8 require "hearing" rhythmic output. But L4-S05 is a separate session that hasn't been built.
- **Risk**: TalaModule can be built and unit-tested, but end-to-end verification is impossible. The spec's verification criteria are untestable.
- **Recommendation**: Build TalaModule and RagaModule without the L4-S05 dependency. Verify through debug logging (`[tala] beat=4 weight=0.8 sam=false`) and unit tests rather than audio output. Mark verification items 1-8 as "deferred to L4-S05 integration." The modules will still route through the affinity graph — VoiceModule just won't be there to receive yet.

### 1.2 QuantizerModule has no `gravity_weights` input port
- **Finding**: RagaModule outputs `gravity_weights` (Pattern, Block) which is meant to feed into QuantizerModule's `degree_weights`. But QuantizerModule currently only has `raw_pitch` (Float) and `gravity_override` (Float) inputs. There's no Pattern input to receive a weights vector.
- **Risk**: The affinity graph auto-discovers edges by matching `SignalType`. Pattern→Pattern edges would be created, but QuantizerModule has no Pattern input to receive them. The gravity_weights signal goes nowhere.
- **Recommendation**: Add a `degree_weights` input port (Pattern, Block) to QuantizerModule in this session. When received, overwrite `self.gravity.degree_weights` with the incoming vector. This is a small change to an existing module that completes the RagaModule→QuantizerModule pipeline.

## 2. Algorithmic Issues

### 2.1 Rhythm gravity should use corrected formula
- **Finding**: Task 6.2 says "Apply gravity pull (same cubic curve as pitch)." The L3-S04 audit corrected the pitch formula from `d * |d|^(1+gravity)` to midpoint-normalized `norm_d * |norm_d|^gravity`.
- **Recommendation**: Use the corrected formula from the start. Normalize beat distance to [-1, 1] using midpoints to adjacent beats, apply `pull = norm_d * |norm_d|^gravity`.

### 2.2 Beat crossing detection in advance()
- **Finding**: `advance()` increments phase by `(tempo/60) * dt`. If dt is large (e.g., lag spike, first frame), the phase could skip multiple beats.
- **Recommendation**: Loop over beat crossings: detect all beats between `old_phase` and `new_phase`, emit a `BeatEvent` for each. Don't just check the current beat.

### 2.3 GamakaState should work in cents, not Hz
- **Finding**: `GamakaState::Sliding { from_hz, to_hz, progress }` uses Hz. The L3-S04 audit just established that pitch smoothing must happen in the log/cents domain.
- **Recommendation**: `Sliding { from_cents, to_cents, progress }` and `Vibrating { center_cents, phase }`. Convert to Hz only at emission.

### 2.4 ScaleMorph across different degree counts
- **Finding**: Bhairav has 8 entries in gravity_weights (root + 7 degrees). Jog's YAML shows 8 entries too, but the Jog *scale* has 6 degrees (+ root = 7 in cents). The weights vector length doesn't match the scale's degree count.
- **Risk**: If switching from Bhairav (8-degree cents vec) to Slendro (6-degree cents vec), lerping between weight vectors of different lengths will panic or produce garbage.
- **Recommendation**: ScaleMorph should only morph weights when the tuning system stays the same (same degree count). When the tuning *also* changes (Bhairav→Slendro), snap the weights immediately rather than morphing. Document this constraint.

## 3. Dependency & Build Issues

### 3.1 `serde_yaml` is archived
- **Finding**: Task 6.15 adds `serde_yaml = "0.9"`. The `serde_yaml` crate was archived by its maintainer (dtolnay) in 2023. The community fork is `serde_yml`.
- **Recommendation**: Either use `serde_yml` instead, or avoid the YAML dependency entirely by embedding tala/raga definitions as Rust constants (like the .scl scales use `include_str!`). Given that there are only 5 talas and 5 ragas, Rust constants are simpler and avoid the external dependency. YAML parsing can be added later when user-defined ragas/talas are needed.

## 4. Scope Concerns

### 4.1 This spec is 2-3x larger than L3-S04
- **Finding**: 15 tasks vs L3-S04's 10. Two new modules, four new algorithm files, YAML parsing, a dependency addition, and modifications to an existing module. Estimated at 1200-1500 lines of new code.
- **Recommendation**: Split into 4-5 branches instead of 3:
  1. `rhythm-gravity` — TalaGrid, TalaDefinition, euclidean_rhythm, beat clock, 5 tala definitions, TalaRegistry
  2. `tala-module` — TalaModule ModuleCore impl + app wiring
  3. `raga-data` — RagaMode, RagaRegistry, 5 raga definitions, DirectionTracker
  4. `gamaka-morph` — GamakaConfig, GamakaState, ScaleMorph
  5. `raga-module` — RagaModule ModuleCore impl + QuantizerModule weights port + app wiring

### 4.2 Gamaka + ScaleMorph may be premature
- **Finding**: GamakaState and ScaleMorph are post-processing features that depend on having audio output (L4-S05) to hear. Without audio, they can only be tested numerically. The DirectionTracker also needs continuous pitch input to be meaningful.
- **Recommendation**: Consider deferring gamaka and scale_morph to a follow-up session (or a later branch that lands after L4-S05). The core value of L3-S06 is TalaModule + RagaModule with gravity_weights routing. Ornaments are polish.

## 5. Minor Issues

### 5.1 Jog YAML gravity_weights has 8 entries but Jog scale has 7 degrees
- **Finding**: `jog.yaml` shows `gravity_weights: [1.0, 0.5, 0.6, 0.7, 0.5, 0.4, 0.6, 1.0]` (8 entries). But the Jog .scl file has 6 degrees + root = 7 entries in the cents vector.
- **Recommendation**: Jog's gravity_weights should have 7 entries to match the scale, not 8. Fix the YAML.

### 5.2 `gamaka_config` as Pattern is awkward
- **Finding**: Encoding [slide_ms, vib_depth, vib_rate] as a 3-element `Pattern(Arc<Vec<f32>>)` is semantically vague — consumers need to know the field ordering by convention.
- **Note**: This is acceptable for now since there's no struct-typed Signal variant. Just document the field order clearly. Could be refactored into a dedicated Signal variant later if gamaka becomes more complex.

---

## Conclusion

The musical architecture is sound. The primary interventions are:
1. **Add a `degree_weights` Pattern input to QuantizerModule** (without this, RagaModule's output goes nowhere)
2. **Avoid the archived serde_yaml dependency** (use Rust constants or serde_yml)
3. **Use the corrected gravity formula** from the L3-S04 audit
4. **Split the scope** into more branches and consider deferring gamaka/morph to post-L4-S05
5. **Fix Jog weights count** to match the actual scale degree count
