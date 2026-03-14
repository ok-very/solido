# S41 — Raga Activation

**Status**: Shipped
**Depends on**: S40 (harmonic interaction), S38 (well ecology)
**Blocks**: organism-union (musical genetics needs raga awareness)

## Goal

Activate the raga system so organisms respond to microtonal raga tunings instead of just 12-TET gravity weights. The 12-TET grid remains available (all chromatic pitches reachable), but raga degrees exert stronger gravitational pull at their exact cents positions.

## Architecture

### Two-Layer Tuning on the Audio Thread

- **Base layer**: `SetScaleWeights([f32; 12], f32)` — unchanged 12-TET chromatic weights
- **Overlay**: `SetMicroTuning { cents, weights, count, blend }` — raga-specific microtonal positions
- **CombinedTuning**: Merges both layers (up to 24 degrees). Degrees within 20 cents merge (micro wins position, max weight). Rebuilt lazily when either layer changes.
- **quantize_to_tuning()**: Cents-space quantizer replaces MIDI-space `quantize_to_scale_fast()`. Gravity-weighted distance in cents domain.

### Control Thread Pipeline

1. `RagaModule` computes micro tuning via `raga_to_micro_tuning()` using TuningRegistry .scl cents
2. Vadi/samvadi boosts applied (arousal-driven: `VADI_BOOST_BASE + arousal * VADI_AROUSAL_SCALE`)
3. `app.rs` per-organism dispatch: transposes cents by `root_pitch_class * 100`, applies aroha/avaroha direction preference, sends `SetMicroTuning`

### Direction Tracking

- `DirectionTracker` (50-cent hysteresis) on each `OrganismModule`, fed by `seq_pitch_hz` from DspAnalysis
- `melodic_direction` added to `MusicalContext` and `WellDispatchEntry`
- Non-path degrees get weight × 0.2 (still reachable, softly reduced)

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_MICRO_DEGREES` | 12 | Max degrees in micro overlay |
| `MICRO_MERGE_TOLERANCE` | 20.0 cents | Merge threshold for 12-TET + micro |
| `VADI_BOOST_BASE` | 1.5 | Base vadi weight multiplier |
| `SAMVADI_BOOST_BASE` | 1.3 | Base samvadi weight multiplier |
| `VADI_AROUSAL_SCALE` | 0.5 | Additional vadi boost per unit arousal |
| `NON_PATH_REDUCTION` | 0.2 | Weight multiplier for non-aroha/avaroha degrees |

## Critical Files

| File | Changes |
|------|---------|
| `src/dsp/command.rs` | `SetMicroTuning` variant, `MAX_MICRO_DEGREES` constant |
| `src/dsp/organism_dsp.rs` | `CombinedTuning`, `rebuild_combined()`, `quantize_to_tuning()`, micro fields |
| `src/tuning/raga.rs` | `raga_to_micro_tuning()`, `apply_direction_preference()`, constants |
| `src/modules/raga_module.rs` | TuningRegistry, cached micro tuning, `micro_tuning()`, `recompute_micro_tuning()` |
| `src/app.rs` | `SetMicroTuning` dispatch per organism, direction preference, bypass handling |
| `src/organism/module/mod.rs` | `DirectionTracker`, `melodic_direction` in MusicalContext |
| `src/organism/module/context.rs` | `melodic_direction` field |

## Tests (17 new, 649 total)

- `cents_distance_basic`, `cents_distance_wraps`
- `combined_tuning_merge`, `combined_tuning_independent`, `combined_tuning_near_merge`
- `quantize_to_tuning_exact_raga`, `quantize_to_tuning_non_raga_chromatic`, `quantize_bypass_when_blend_zero`
- `set_micro_tuning_marks_dirty`
- `raga_to_micro_bhairav`, `raga_to_micro_yaman`, `raga_to_micro_jog`
- `vadi_boost_applied`, `aroha_soft_preference`
- `direction_tracker_ascending`, `direction_tracker_descending`, `direction_tracker_hysteresis`
