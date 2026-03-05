# Trigger-Level Cross-Organism Routing

**Status**: Planned (future work)
**Depends on**: interaction_params DNA extension (S28)

## Goal

Enable rhythm triggers to cross organism boundaries through the AffinityGraph.
KKIT's kick pattern can reset ACID's sequencer. TBLK's dayan strike can fire
KKIT's snare. Organisms synchronize rhythmically through learned connections,
not hardwired clock sync.

## Current State

Triggers exist at two levels but do not bridge between them:

- **Intra-organism (audio thread, 44.1kHz)**: `seq_cell` and `logic_seq_cell`
  emit triggers through `WireType::Trigger` wires to `drum_voice_cell`,
  `strike_voice_cell`, etc. Sample-accurate within `OrganismDsp::tick()`.
- **Inter-organism (control thread, 60Hz)**: `OrganismModule` accepts a `gate`
  input port (`SignalType::Trigger`) that sends `DspCommand::NoteOn` to the
  audio thread via ringbuf. The AffinityGraph can already route Trigger signals
  -- but no organism currently **emits** triggers on output ports.

The gap: organisms have no trigger output ports. Internal seq_cell fires stay
inside `OrganismDsp`. The affinity graph never sees them.

## Architecture

### DNA Schema

```json
"trigger_exports": [
  { "name": "kick", "source_cell": 0, "description": "Four-on-the-floor kick" },
  { "name": "snare", "source_cell": 1, "description": "Backbeat snare" }
],
"trigger_inputs": [
  { "name": "reset", "action": "seq_reset", "target_cell": 0 },
  { "name": "ext_strike", "action": "note_on", "target_cell": 2 }
]
```

- `source_cell` indexes into `cells[]`. When that cell fires a trigger (edge
  detection on gate output in `OrganismDsp`), the event is reported upstream.
- `action` maps to `DspCommand` variants: `seq_reset` resets a seq_cell's
  step counter, `note_on` fires `DspCommand::NoteOn` on the target cell.

### Signal Flow

```
KKIT seq_cell[0] fires (audio thread, sample-accurate)
  -> OrganismDsp detects trigger, pushes TriggerEvent to analysis channel
  -> OrganismModule::tick() drains channel, latches "kick fired this frame"
  -> OrganismModule::emit_signals() emits Signal::Trigger on kick_port
  -> SeedReactor routes through AffinityGraph to ACID's "reset" input
  -> ACID OrganismModule::receive_signal() sends DspCommand::SeqReset
  -> ACID seq_cell resets step counter on next sample tick
```

## Timing

Three options in increasing fidelity:

**(A) Accept 60Hz jitter (recommended for v1).** Control-thread triggers arrive
within one frame (~16ms). At 130 BPM, a 16th note is ~115ms. 16ms jitter is
~14% -- noticeable as loose feel but musically usable for polyrhythmic
cross-triggering where tight lock is not the aesthetic goal.

**(B) Timestamped trigger buffer.** `OrganismDsp` reports the exact sample
offset. `DspCommand` gains a `sample_offset: u32` field. Receiving organisms
delay response to the matching sample position. Sample-accurate sync.

**(C) Audio-rate trigger bus.** Shared ringbuf carries triggers directly between
`OrganismDsp` instances on the audio thread. Lowest latency, breaks the
"everything routes through AffinityGraph" invariant. Not recommended.

## Hebbian Learning

Trigger edges need a different reward signal than continuous Float edges.
Proposed metric: **rhythmic coherence**.

- After trigger delivery, measure receiving organism's next audio onset
  relative to its expected pattern. Onset within 10ms of grid → reward.
- Trigger disrupted ongoing pattern (valence dropped) → penalize.
- Trigger ignored (organism mid-note) → zero reward, natural decay.

Reuses existing `ModuleEmotion` valence as Hebbian reward. No new learning
infrastructure required.

## Implementation Order

1. Extend `DspAnalysis` with `TriggerEvent` variant (or add second channel)
2. Add `TriggerExportDna` / `TriggerInputDna` to `dna.rs`
3. Add `DspCommand::SeqReset` variant
4. Wire trigger export detection in `OrganismDsp::tick()`
5. Create dynamic trigger ports in `OrganismModule::new()` from DNA
6. Handle trigger inputs in `OrganismModule::receive_signal()`
7. Add trigger exports to KKIT and TBLK DNA
8. Add trigger inputs (reset, ext_strike) to ACID and TBLK DNA
9. Test: KKIT kick -> ACID reset end-to-end

## Critical Files

| File | Change |
|------|--------|
| `src/organism/module.rs` | Dynamic trigger ports, emit/receive |
| `src/organism/dna.rs` | TriggerExportDna, TriggerInputDna |
| `src/dsp/organism_dsp.rs` | Detect internal triggers, report upstream |
| `src/dsp/command.rs` | SeqReset, TriggerEvent variants |
| `assets/dna/kkit-909.json` | First trigger_exports |
| `assets/dna/acid-kinoko.json` | First trigger_inputs |
