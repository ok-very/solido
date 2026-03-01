# S27 — Bus Effects Decay + Calibration

**Layer**: L4
**Depends on**: S26 (bus UI exposed)
**Status**: Spec

## Problem

After S23 (tape delay bus) and with all six organisms running, the reverb and tape delay buses sustain too long — they do not decay properly when the source signal stops. The effect sounds like a wash that never cleans up.

## Root Causes

### Reverb long tail

`reverb_bus.rs` maps `dcy` DNA param as:
```rust
let time = dcy * 8.0 + 1.0;  // [1, 9] seconds
```

At `dcy=0.5` (mid-range), `time = 5.0s` — a very long tail. Most DNA presets use `dcy=0.2–0.4`, giving `1.6–4.2s`. These values feel acceptable in isolation but wash out when six organisms are sending simultaneously.

**Fix**: Rescale the mapping so mid-range gives 2–3s:
```rust
let time = dcy * 4.0 + 0.5;  // [0.5, 4.5] seconds
```

Or expose `time` directly in DNA instead of through the `dcy` indirection.

### Tape delay feedback saturation

`tape_delay_bus.rs` feedback is read directly from the Shared handle. If feedback approaches 1.0 (or exceeds it due to imprecise DNA authoring), the delay line saturates and never decays. The bus has no hard feedback ceiling enforced at tick time.

**Fix**: Clamp feedback to 0.0–0.92 in `TapeDelayBus::tick()`, regardless of the handle value.

### No noise gate on bus returns

Both buses continue to output signal below the noise floor indefinitely. A simple noise gate (threshold ~-60dBFS) on the return would clean up tail behaviour and prevent DC drift.

---

## Scope

1. **Rescale reverb time mapping** in `reverb_bus.rs::new()` — change `dcy * 8.0 + 1.0` to a shorter range
2. **Clamp tape delay feedback** in `tape_delay_bus.rs::tick()` — hard ceiling at 0.92
3. **Noise gate on bus returns** — threshold in both `reverb_bus.rs` and `tape_delay_bus.rs`
4. **Re-tune DNA reverb params** — revisit `size`/`dcy`/`damp` per organism after rescaling
5. **Tests**:
   - `reverb_decays_to_silence_after_source_stops`: feed 0.5s impulse, verify output < -60dBFS after 5s
   - `tape_delay_feedback_clamped`: feedback=1.2 in handle, verify bus never saturates

---

## Known Behaviour (until fixed)

- Reverb washes out when multiple organisms send simultaneously
- Tape delay can sustain indefinitely if feedback ≥ 1.0
- Both buses add perceived volume to the mix (contributing to the "too quiet dry, too loud wet" balance)
