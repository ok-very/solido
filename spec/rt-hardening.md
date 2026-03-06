# RT Hardening — Deferred Items from 2026-03-04 Audit

**Status**: Specced. L-01/M-02/M-03 fixed. Remaining items are latency optimizations and maintainability improvements.

## Completed (this audit)

- **L-01 FIXED**: Spawn capacity guard — `organisms.len() >= MAX_CHANNELS` check prevents heap allocation on RT thread during spawn integration (`substrate/audio.rs:256`). Same guard added to `VoiceBus::add_strip`, `ReverbBus::add_organism_send`, `TapeDelayBus::add_organism_send`.
- **M-02 FIXED**: `trigger_commands` capacity — now `trigger_wire_count.max(cell_count)` instead of just `cell_count` (`organism_dsp.rs:190`). Covers fan-out organisms.
- **M-03 FIXED**: `channel::drain()` doc comment — warns `Allocates. Forbidden on the audio thread.`

## Deferred: L-02 — Precomputed per-cell adjacency lists

**Priority**: Low (acceptable at ≤16 cells per organism)

**Problem**: `tick()` inner loop is O(cells × wires) per sample — scans ALL wires for each cell to find matches.

**Fix**: At `from_dna()` time, build per-cell incoming wire lists:
```rust
// In OrganismDsp struct:
audio_inputs: Vec<Vec<(usize, f32, WireMode)>>,  // [dst_cell] -> [(src_cell, gain, mode)]

// In from_dna():
let mut audio_inputs = vec![Vec::new(); cell_count];
for (src, dst, tag) in &wiring {
    if let WireTag::Audio { gain, mode } = tag {
        audio_inputs[*dst].push((*src, *gain, mode.clone()));
    }
}

// In tick(): replace inner wire scan with direct lookup
for &i in &self.tick_order {
    let mut cell_input = [0.0f32; 2];
    for &(src, gain, ref mode) in &self.audio_inputs[i] {
        // accumulate from scratch[src]
    }
}
```

**Trigger**: When any organism DNA exceeds 16 cells or 20 wires.

## Deferred: L-03 — Cache sample_cell playback rate

**Priority**: Low (~1ms/s CPU at 44.1kHz)

**Problem**: `sample_cell.rs:232` calls `(2.0_f32).powf(tune/12.0)` every tick.

**Fix**: Cache playback rate, recompute only when `tune` Shared changes (same pattern as `osc_cell` detune caching):
```rust
// In SampleCell struct:
cached_tune: f32,
playback_rate: f32,

// In tick():
let tune = self.tune.value();
if (tune - self.cached_tune).abs() > 0.01 {
    self.cached_tune = tune;
    self.playback_rate = 2.0_f32.powf(tune / 12.0);
}
```

## Deferred: M-01 — aarch64 denormal protection

**Priority**: Very low (only relevant for ARM builds)

**Problem**: `substrate/audio.rs:245-250` sets FTZ+DAZ only under `#[cfg(target_arch = "x86_64")]`. ARM builds get no explicit denormal flush.

**Note**: ARM NEON flushes denormals by default on most platforms. Only needed if targeting Raspberry Pi or Apple Silicon with non-NEON code paths.

**Fix** (when needed):
```rust
#[cfg(target_arch = "aarch64")]
unsafe {
    // Set FPCR.FZ bit (bit 24) to flush denormals to zero
    let mut fpcr: u64;
    std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
    std::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
}
```

## Deferred: M-04 — osc_cell string comparison to bool flag

**Priority**: Very low (no allocation, just pedantic)

**Problem**: `osc_cell.rs:182` does `self.wtype == "pulse"` in per-sample tick. This is a byte comparison (no allocation) but could be a trivial bool check.

**Fix**: Replace `wtype: String` with `is_pulse: bool` set at construction. Saves 24 bytes struct size and eliminates string comparison from hot path.

## Optional: assert_no_alloc integration

For runtime proof that the audio callback is allocation-free:

```toml
[dev-dependencies]
assert_no_alloc = "1"
```

Wrap the callback body in debug builds:
```rust
#[cfg(debug_assertions)]
assert_no_alloc::assert_no_alloc(|| {
    // ... full callback body ...
});
```

This panics with a stack trace on the first allocation, catching violations that static analysis misses.
