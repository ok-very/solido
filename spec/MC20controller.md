# MC20 Controller — Full Interface Spec

**Status**: In Progress
**Depends on**: S31 (transport), S42 (RECH), MidiBus infrastructure
**Blocks**: Pre-union iteration (provides the interaction surface for evaluation)

## Goal

Replace the floating panel UI (organism_panel + synth_detail windows) with a single full-window **MC20 Controller** layout inspired by the Korg MS-20's horizontal architecture. The MC20 is THE Solido interface — not a plugin or panel, but the primary window.

## Layout (Classic Horizontal)

```
╔══════╦════════════════════════════════════════════════════════╦═══════════════╗
║  O   ║                     CELL PARAMETERS                   ║   XY PAD      ║
║  R   ║  ┌─ cell 0 ──┐  ┌─ cell 1 ──┐  ┌─ cell 2 ──┐       ║   ┌─────────┐ ║
║  G   ║  │ knob knob  │  │ knob knob │  │ knob knob │       ║   │    +    │ ║
║  A   ║  │ knob knob  │  │ knob mode │  │ knob togg │       ║   │ X:.45   │ ║
║  N   ║  │ [bypass]   │  │ [bypass]  │  │ [bypass]  │       ║   │ Y:.72   │ ║
║  I   ║  └────────────┘  └───────────┘  └───────────┘       ║   └─────────┘ ║
║  S   ║  ┌─ cell 3 ──┐  ┌─ cell 4 ──┐  ┌─ cell 5 ──┐       ║   ┌─ EG ────┐ ║
║  M   ║  │ knob knob  │  │ knob knob │  │ knob drop │       ║   │ A D S R │ ║
║  S   ║  └────────────┘  └───────────┘  └───────────┘       ║   └─────────┘ ║
╠══════╩════════════════════════════════════════════════════════╩═══════════════╣
║  P A T C H   B A Y                                                           ║
║ ┌──────────────────────── OUTPUTS ◎ ──────────────────────────────────────┐  ║
║ │ ◎ PITCH   ◎ GATE   ◎ VEL   ◎ VALENCE  ◎ AROUSAL  ◎ CONSON  ◎ CHAOS  │  ║
║ │ ◎ SEQ     ◎ LFO    ◎ EG    ◎ CONTOUR  ◎ DENSITY  ◎ BEAT.PH          │  ║
║ └────────────────────────────────────────────────────────────────────────┘  ║
║ ┌──────────────────────── INPUTS ● ───────────────────────────────────────┐  ║
║ │ ● FREQ.CV  ● CUTOFF  ● GAIN  ● PW   ● DRIVE   ● CALM   ● LINK.BIAS  │  ║
║ │ ● RATE.CV  ● DEPTH   ● TRIG  ● SWING ● CHAOS.IN ● FIELD ● AVOID.B   │  ║
║ └────────────────────────────────────────────────────────────────────────┘  ║
║  CC MAP: [mappings row]                                        [+ LEARN]    ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

### Sections

1. **Organism Selector (left strip)** — Vertical list of spawned organisms. Click = select + arm for MIDI NoteOn. ▶ = armed indicator. Compact: icon + 4-char name.

2. **Cell Parameters (center)** — Dynamic grid of cell modules from the armed organism's DNA. Each cell = bordered frame with:
   - Header: cell type name + bypass checkbox
   - Knobs: one per Shared param (rotary style, right-click = CC learn)
   - Dropdowns: for enum params (waveform, filter mode, curve type)
   - Values: numeric display below each knob

3. **XY Pad (right, top)** — 200×200 interactive square. Kaoscillator convention (Y-inverted: top=1.0). Crosshair + grid overlay. Outputs X (ch0), Y (ch1) as Shared.

4. **EG Compact (right, bottom)** — ADSR mini-display for the first env_cell found. Vertical sliders, compact.

5. **Patch Bay (bottom, full width)** — Two rows of jacks:
   - **Outputs ◎** (top row): signals the organism emits (pitch, gate, velocity, valence, arousal, consonance, chaos, seq, lfo, eg, contour, density, beat_phase)
   - **Inputs ●** (bottom row): parameters that accept external routing (freq_cv, cutoff, gain, pw, drive, calm, link_bias, avoid_bias, rate_cv, depth, trig, swing, chaos_in, field_in)
   - **Cables**: Colored Bezier curves between connected ◎→● pairs. Each cable gets a unique color from a 12-color palette. Slight sag for realism.
   - **Interaction**: Click ◎ → glows amber → compatible ● targets highlight green → click to connect. ESC cancels.

6. **CC Map Strip (bottom edge)** — Horizontal row showing active MIDI CC mappings: `CC# → target [×]`. [+ LEARN] button enters global learn mode.

7. **Header Bar** — Title, MIDI device dropdown, transport (▶/⏸/⏹ + BPM).

## Architecture

### Global ParamRegistry

```rust
pub struct ParamRegistry {
    /// Fully qualified keys: "DRON_0.cell2.cutoff"
    params: HashMap<String, Shared>,
}
```

- Built at organism spawn time from `OrganismDsp::shared_handles()`
- Keys: `"{species}_{index}.cell{N}.{param}"` (e.g., `"DRON_0.cell1.cutoff"`)
- Removed on organism kill
- Used by CC dispatch: `CcMapping.target` stores a registry key
- Thread-safe: lives on control thread only (UI + MIDI drain)

### CC→Shared Dispatch

In `drain_midi_events()`:
```
ControlChange { cc, value, .. } →
  if learning → create mapping (cc → focused param registry key)
  else → find_mapping(cc) → registry.get(target) → handle.set(mapped_value)
```

### MIDI Transport

```
MidiEvent::Start    → clock.playing.set(1.0)
MidiEvent::Stop     → clock.playing.set(0.0)
MidiEvent::Continue → clock.playing.set(1.0)
```

### Patch Bay Semantics

Patch bay connections are **visual aliases for affinity graph edges**. Connecting ◎PITCH → ●FREQ.CV on another organism creates an affinity edge between those ports. The patch bay is a human-friendly view of the AffinityGraph topology.

For v1: patch bay is intra-organism only (wiring between cells within one organism). Cross-organism patching comes with organism-union.

## Interaction Details

- **Right-click knob** → enters CC learn for that specific param. Knob border pulses amber until a CC arrives or ESC cancels.
- **Click organism** → selects it, updates center cell grid, arms for MIDI NoteOn.
- **Click ◎ output jack** → jack glows amber, all compatible ● inputs highlight green, incompatible dim. Click a green ● to create cable. Click amber ◎ again or ESC to cancel.
- **Click existing cable** → cable highlights, delete button appears.
- **Hover jack** → tooltip with signal name + current value.

## Critical Files

| File | Action |
|------|--------|
| `src/ui/panels/mc20.rs` | **NEW** — MC20 controller panel layout |
| `src/ui/panels/patch_bay.rs` | **NEW** — Patch bay widget (jacks + cables) |
| `src/param_registry.rs` | **NEW** — Global parameter registry |
| `src/app.rs` | Wire MC20 panel, param registry, CC dispatch, MIDI transport |
| `src/ui/panels/synth_detail.rs` | Absorb into mc20.rs (cell parameter section) |
| `src/ui/panels/organism_panel.rs` | Slim down to compact selector for MC20 left strip |
| `src/audio/midi_bus.rs` | No changes (already complete) |

## Implementation Order

1. `ParamRegistry` — global key→Shared lookup
2. CC→Shared dispatch wiring in `drain_midi_events()`
3. MIDI transport (Start/Stop/Continue)
4. `mc20.rs` — layout skeleton (left/center/right/bottom zones)
5. Center zone: cell parameter grid (migrate from synth_detail)
6. Left zone: organism selector strip
7. Right zone: XY pad + EG compact
8. Bottom zone: patch bay (jacks + click-to-connect)
9. Cable rendering (Bezier curves)
10. CC map strip + learn UI integration
