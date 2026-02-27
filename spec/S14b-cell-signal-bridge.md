# S14b — Cell-to-Module Signal Bridge

> The body's rhythm becomes the creature's voice. What happens inside is felt outside.

**Layer**: L4 (Cells) → L5 (Organisms) → L0 (Routing Backbone)
**Depends on**: S14 (cell-wiring), S13 (first organisms), S02 (routing backbone)
**Status**: Prospect

## Goal

Bridge cell-level events (beat triggers, envelope gates, spectral features) into the module signal layer so the AffinityGraph can learn cross-organism coordination. Currently the DSP cell layer and the module signal layer are disconnected — OrganismModule emits only `rms`, `peak`, and `is_active`. This spec makes the internal life of organisms visible to the learning system.

## Ancestry (MAKE A BABY)

The original Max/MSP patch had `send~` / `receive~` tunnels that let one subpatcher's rhythm drive another's envelope. Cell-to-module bridging is the Solido equivalent: what's born inside one organism can be felt by all.

## The Problem

Two disconnected worlds:

```
DSP Thread (44.1kHz)                    Control Thread (60Hz)
  PatternGen emits triggers ──╳──→     OrganismModule knows nothing
  ADSR envelope state        ──╳──→     AffinityGraph can't learn rhythm sync
  Spectral centroid          ──╳──→     No cross-organism timbre awareness
```

OrganismModule (module.rs:165-172) only sees aggregate `rms` and `peak` from the audio analysis channel. Cell-level events are invisible to the affinity graph. Organisms can't learn "TBLK's beat should drive MELO's arpeggiator."

## Architecture Decisions

### AD-1: Cells declare exportable events in their DspCell trait

```rust
pub trait DspCell: Send {
    // ... existing methods ...

    /// Events this cell can export to the module signal layer.
    /// Empty by default — only rhythm/envelope/spectral cells override.
    fn exportable_events(&self) -> &[CellEventDescriptor] { &[] }

    /// Drain pending events since last call. Audio thread calls this
    /// at block boundaries (~689Hz), not per-sample.
    fn drain_events(&mut self) -> &[CellEvent] { &[] }
}
```

### AD-2: Events are block-rate summaries, not per-sample

Cell events are NOT per-sample — they're summaries computed at block boundaries (every 64 samples). This keeps the ring buffer traffic manageable:

| Cell | Event | Rate | Content |
|------|-------|------|---------|
| PatternGen | BeatTrigger | Per-beat (~2-8 Hz) | velocity, accent |
| Arpeggiator | NoteTrigger | Per-note (~4-16 Hz) | freq, velocity |
| ADSR/Envelope | EnvelopeState | Per-block (~689 Hz) | stage (A/D/S/R), level |
| HarmonicBed | SpectralCentroid | Per-block | centroid_hz |

### AD-3: OrganismDsp collects cell events and sends via ring buffer

A new `CellEvent` ring buffer channel (audio → control thread) carries typed events:

```rust
pub enum CellEvent {
    Trigger { cell_idx: u8, velocity: f32 },
    EnvelopeStage { cell_idx: u8, stage: u8, level: f32 },
    SpectralFeature { cell_idx: u8, centroid_hz: f32 },
}
```

`OrganismDsp::tick()` calls `drain_events()` on each cell at block boundaries and forwards to the ring buffer.

### AD-4: OrganismModule creates dynamic output ports from cell events

At construction, OrganismModule inspects its cells' `exportable_events()` and creates output ports:

```
organism:tblk outputs:
  rms          (Float, Block)     — existing
  peak         (Float, Block)     — existing
  is_active    (Bool, Block)      — existing
  beat_trigger (Trigger, Event)   — NEW from PatternGen
  beat_velocity (Float, Event)    — NEW from PatternGen
```

These ports appear in the AffinityGraph. Hebbian learning discovers that TBLK's `beat_trigger` driving MELO's arpeggiator rate is productive.

### AD-5: Mixer state exposed as module output ports

OrganismModule also exposes its mixer state (from VoiceBus SharedHandles) as output signals:

```
organism:tblk outputs:
  mixer_rms    (Float, Block)     — pre-bus audio level
  mixer_gain   (Float, Block)     — current gain setting
```

This lets the affinity graph learn volume coordination: if DRON's `mixer_rms` is high and TBLK's `mixer_rms` is also high, the system can learn to reduce one.

## Implementation

### 1. CellEvent type and DspCell extension

New file: `src/dsp/cell/event.rs`

### 2. PatternGen exports beat triggers

`pattern_gen.rs`: Override `drain_events()` to emit `CellEvent::Trigger` when the euclidean pattern fires.

### 3. Arpeggiator exports note triggers

`arpeggiator.rs`: Emit `CellEvent::Trigger` with frequency and velocity on each note.

### 4. Ring buffer for cell events

New channel in `AudioSubstrate::new()`: `channel::<CellEvent>(128)` per organism.

### 5. OrganismModule dynamic port creation

In `OrganismModule::new()`, iterate cell DNA to determine which cells produce events, create corresponding output ports.

### 6. OrganismModule::tick drains cell events

Drain the cell event ring buffer and update internal state. Emit on next `emit_signals()` call.

## Files Created

| File | Description |
|------|-------------|
| `src/dsp/cell/event.rs` | CellEvent enum, CellEventDescriptor |

## Files Modified

| File | Changes |
|------|---------|
| `src/dsp/cell/mod.rs` | DspCell trait gains `exportable_events()`, `drain_events()` |
| `src/dsp/cell/pattern_gen.rs` | Export beat triggers |
| `src/dsp/cell/arpeggiator.rs` | Export note triggers |
| `src/dsp/organism_dsp.rs` | Block-boundary event collection, new ring buffer |
| `src/organism/module.rs` | Dynamic ports from cell events, drain events in tick |
| `src/substrate/audio.rs` | New ring buffer per organism for cell events |

## Verification

- [ ] TBLK OrganismModule emits `beat_trigger` signal into affinity graph
- [ ] MELO arpeggiator can receive TBLK beat_trigger via learned edge
- [ ] Hebbian learning strengthens TBLK→MELO beat connection when MELO valence is positive
- [ ] Cell events arrive at block rate (~689 Hz), not per-sample
- [ ] Ring buffer overflow is silent (drops oldest events, no panic)
- [ ] Existing rms/peak/is_active emissions unchanged
