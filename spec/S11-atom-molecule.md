# S11 — Atom + Molecule Primitives

**Layer**: L2 (Atoms) + L3 (Molecules)
**Depends on**: S05c (two-tier architecture), S06 (rhythm/raga infrastructure)
**Status**: Complete (commit `71883b7`)
**FunDSP**: 0.23.0

## Goal

Build the primitive DSP building blocks that organisms are made of. Atoms are
single-function FunDSP wrappers. Molecules are small fixed-wired atom combinations.
Both run on the **audio thread** — organisms own their sound.

## Key Implementation Decision: Shared/Var (Lock-Free Atomics)

FunDSP 0.23 provides `Shared`/`var` — lock-free atomic floats that can be set from
the control thread and read by the audio graph. This eliminates the need for graph
reconstruction on parameter changes. Every controllable parameter becomes a `Shared`
handle.

```
Control thread (60Hz):                Audio thread (44.1kHz):
  OrganismModule (S13, future)          Atoms tick per-sample via FunDSP
    shared_freq.set(440.0)   ────→      var(&shared_freq) >> sine()
    shared_cutoff.set(2000.0) ───→      (pass() | var(&shared_cutoff) | var(&q)) >> lowpass()
    cmd_tx.send(NoteOn)      ────→      drain commands, trigger AdsrAtom gate
```

- **Float params**: `Shared` (lock-free atomic, no ring buffer needed)
- **Trigger events**: `DspCommand` via ring buffer (NoteOn/NoteOff/Reset only)
- **No graph reconstruction** — `Shared::set()` is O(1) and allocation-free

This deviates from the original spec's reconstruction approach. The original spec
proposed rebuilding FunDSP nodes on parameter change; Shared/var is strictly better.

## DspAtom Trait

```rust
pub trait DspAtom: Send {
    fn tick(&mut self, input: &[f32], output: &mut [f32]);
    fn set_param(&mut self, name: &str, value: f32) -> bool;
    fn get_param(&self, name: &str) -> Option<f32>;
    fn audio_inputs(&self) -> usize;
    fn audio_outputs(&self) -> usize;
    fn reset(&mut self);
    fn name(&self) -> &str;
}
```

Note: method names are `audio_inputs()`/`audio_outputs()` (not `inputs()`/`outputs()`
as originally specced). This avoids ambiguity with Shared parameter "inputs".

## Atom Inventory (17 implemented)

### Oscillators (5)

| Atom | FunDSP graph | Shared params | In/Out |
|------|-------------|---------------|--------|
| `NoiseAtom` | `noise()` | — | 0→1 |
| `SineAtom` | `var(&freq) >> sine()` | freq | 0→1 |
| `SawAtom` | `var(&freq) >> saw()` | freq | 0→1 |
| `SquareAtom` | `var(&freq) >> square()` | freq | 0→1 |
| `PulseAtom` | `(var(&freq) \| var(&width)) >> pulse()` | freq, width | 0→1 |

### Filters (5)

| Atom | FunDSP graph | Shared params | In/Out |
|------|-------------|---------------|--------|
| `LowpassAtom` | `(pass() \| var(&cutoff) \| var(&q)) >> lowpass()` | cutoff, q | 1→1 |
| `HighpassAtom` | `(pass() \| var(&cutoff) \| var(&q)) >> highpass()` | cutoff, q | 1→1 |
| `BandpassAtom` | `(pass() \| var(&center) \| var(&bw)) >> resonator()` | center, bandwidth | 1→1 |
| `AllpassAtom` | `(pass() \| var(&freq) \| var(&q)) >> allpass()` | freq, q | 1→1 |
| `LowpoleAtom` | `(pass() \| var(&freq)) >> lowpole()` | freq | 1→1 |

### Effects + Utilities (4)

| Atom | FunDSP graph | Shared params | In/Out |
|------|-------------|---------------|--------|
| `DelayAtom` | `delay(time)` | — (fixed at construction) | 1→1 |
| `PanAtom` | `pan(0.0)` + `Setting::pan(p)` | position (via Setting) | 1→2 |
| `GateAtom` | custom threshold comparator | threshold | 1→1 |
| `EnvFollowAtom` | `follow(time)` | — (fixed at construction) | 1→1 |

### Envelope + Timing (3)

| Atom | FunDSP graph | Shared params | In/Out |
|------|-------------|---------------|--------|
| `AdsrAtom` | custom (wraps `AdsrState`) | a, d, s, r, gate | 0→1 |
| `LfoAtom` | `var(&rate) >> sine()` × depth | rate, depth | 0→1 |
| `ClockAtom` | custom sample counter | bpm, division | 0→1 |

**Special cases**:
- DelayAtom/EnvFollowAtom: construction-time params only (FunDSP allocates fixed buffers)
- PanAtom: uses `AudioUnit::set(Setting::pan(p))` instead of Shared (no signal-input form)
- AdsrAtom: `set_param("gate", v)` where v > 0.5 = note_on, v <= 0.5 = note_off
- GateAtom, ClockAtom: custom implementations, not FunDSP wrappers

## Molecule (Enum, Two Variants)

The original spec had Molecule as a struct. Implementation uses an enum with two
variants, which better captures the performance difference between fused and wired
topologies.

### Fused — Single FunDSP AudioUnit

Best performance — single `tick()` call processes the whole chain. For molecules
composed entirely of FunDSP primitives with Shared param handles.

```rust
Molecule::Fused {
    name: String,
    unit: Box<dyn AudioUnit>,
    params: Vec<(String, Shared)>,
    audio_inputs: usize,
    audio_outputs: usize,
}
```

### Wired — Individual DspAtoms with Explicit Routing

For molecules needing custom atoms (AdsrAtom, ClockAtom, GateAtom) that can't
be expressed as FunDSP graphs. Uses scratch buffers and topological processing order.

```rust
Molecule::Wired {
    name: String,
    atoms: Vec<(String, Box<dyn DspAtom>)>,
    wiring: Vec<(usize, usize, usize, usize)>,  // src_atom, src_ch, dst_atom, dst_ch
    process_order: Vec<usize>,                    // topological sort
    scratch: Vec<Vec<f32>>,
    external_inputs: Vec<(usize, usize)>,         // (atom_idx, ch)
    external_outputs: Vec<(usize, usize)>,        // (atom_idx, ch)
}
```

### Molecule Methods

```rust
impl Molecule {
    fn tick(&mut self, input: &[f32], output: &mut [f32]);
    fn set_param(&mut self, name: &str, value: f32) -> bool;  // supports "atom.param" dotted names
    fn get_param(&self, name: &str) -> Option<f32>;
    fn audio_inputs(&self) -> usize;
    fn audio_outputs(&self) -> usize;
    fn reset(&mut self);
    fn name(&self) -> &str;
}
```

Some Wired molecules have custom tick functions that handle modulation routing
(e.g., ADSR output → filter cutoff), since these modulate parameters rather than
routing audio:
- `tick_stereo_spread()` — LFO → PanAtom position
- `tick_filter_envelope()` — ADSR → LowpassAtom cutoff
- `tick_amp_envelope()` — ADSR × audio

## Molecule Inventory (9 implemented)

### TBLK (all Fused)

| Factory | Signature | I/O | Params |
|---------|-----------|-----|--------|
| `membrane_sim` | `(freq, bw, sr)` | 0→1 | center, bandwidth |
| `snap_transient` | `(sr)` | 0→1 | — |
| `body_resonance` | `(delay_time, sr)` | 1→1 | — |

### DRON (Fused + 1 Wired)

| Factory | Signature | I/O | Params |
|---------|-----------|-----|--------|
| `detuned_stack` | `(root_hz, sr)` | 0→1 | f1, f2, f3, f_sub |
| `slow_filter` | `(cutoff, q, sr)` | 1→1 | cutoff, q |
| `stereo_spread` | `(sr)` | 1→2 | lfo.rate, lfo.depth (Wired) |

### MELO (Fused + 2 Wired)

| Factory | Signature | I/O | Params |
|---------|-----------|-----|--------|
| `osc_pair` | `(freq, sr)` | 0→1 | freq, freq_sub |
| `filter_envelope` | `(base_cutoff, depth, sr)` | 1→1 | adsr.{gate,a,d,s,r} (Wired) |
| `amp_envelope` | `(sr)` | 1→1 | adsr.{gate,a,d,s,r} (Wired) |

## DspCommand Protocol

```rust
#[derive(Debug, Clone, Copy)]
pub enum DspCommand {
    NoteOn { freq: f32, velocity: f32 },
    NoteOff,
    Reset,
    Panic,
}

#[derive(Debug, Clone, Copy)]
pub struct DspAnalysis {
    pub rms: f32,
    pub peak: f32,
}
```

DspCommand is `Copy` and ≤ 16 bytes. Only for discrete events. All continuous
params use Shared directly — no SetParam variant needed.

## Extracted AdsrState

`AdsrState` + `AdsrStage` moved from `src/audio/voice.rs` to `src/dsp/adsr.rs`.
`src/audio/voice.rs` re-imports: `pub use crate::dsp::adsr::{AdsrStage, AdsrState};`
All 28 existing voice tests still pass.

## File Structure

```
src/dsp/
  mod.rs              — pub mod atom, molecule, command, adsr + integration tests
  adsr.rs             — AdsrState + AdsrStage (extracted from audio/voice.rs)
  command.rs          — DspCommand, DspAnalysis
  atom/
    mod.rs            — DspAtom trait, re-exports, test helpers (render_atom, rms, zero_crossings)
    oscillators.rs    — NoiseAtom, SineAtom, SawAtom, SquareAtom, PulseAtom
    filters.rs        — LowpassAtom, HighpassAtom, BandpassAtom, AllpassAtom, LowpoleAtom
    effects.rs        — DelayAtom, PanAtom, GateAtom, EnvFollowAtom
    envelopes.rs      — AdsrAtom, LfoAtom, ClockAtom
  molecule/
    mod.rs            — Molecule enum + impl + build_scratch()
    tblk.rs           — membrane_sim, snap_transient, body_resonance
    dron.rs           — detuned_stack, slow_filter, stereo_spread + tick_stereo_spread
    melo.rs           — osc_pair, filter_envelope, amp_envelope + tick helpers
```

## Test Summary

69 new tests, all passing. Categories:
- Per-atom: produces audio, param change works, reset works
- Per-molecule: produces audio, params modulate output
- Integration: TBLK chain (percussive transient), DRON chain (stereo drone with beating),
  MELO chain (synth pluck with ADSR envelope)
- DspCommand: is Copy, ≤ 16 bytes
