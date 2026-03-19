# UI: Western Scales + Controls Panel Redesign

**Status**: In Progress
**Depends on**: S41 (raga activation), existing scale.rs + controls.rs
**Blocks**: pre-union-iteration (all environments need key/scale selection)
**Priority**: Immediate

---

## Goal

1. Expand the scale system from 6 abstract modes to 17 western scales + 5 ragas.
2. Replace the controls panel internals with a **Circle of Fifths** widget driven by a reusable **RotaryDial** component.
3. Build the RotaryDial as a standalone egui widget — first entry in the Solido UI component library.

---

## Design Decisions (Locked)

- **Scale selection**: Circle of Fifths, not a dropdown.
- **Circle behavior**: Two rings — outer = 12 key names (clickable), inner = active scale degrees (visualizer, glows on active degrees).
- **Center dial**: A draggable rotary dial that turns to select key position. The alien dial (`refs/ui/alien/dial2.png`) is the visual reference — organic bioluminescent knob aesthetic.
- **Mode selection**: Chip buttons below the circle for scale presets + raga presets.
- **Panel**: Keep as floating window, redesign internals.
- **Size**: Responsive — circle fills available panel width.
- **Interaction**: Drag center dial to rotate key selection around the circle. Click ring positions directly as shortcut. Both work.

---

## 1. RotaryDial Widget (`src/ui/widgets/rotary_dial.rs`)

Reusable egui widget. First entry in `src/ui/widgets/`.

### API

```rust
pub struct RotaryDial {
    /// Number of discrete positions (12 for CoF, continuous for knobs).
    positions: usize,
    /// Current position index (0-based). For continuous: f32 angle.
    value: usize,
    /// Visual radius of the dial.
    radius: f32,
}

pub struct RotaryDialResponse {
    pub changed: bool,
    pub value: usize,
}
```

### Behavior

- **Drag**: Horizontal or angular drag rotates the dial. Snaps to nearest position.
- **Click**: Click on the dial body, drag direction determines rotation.
- **Scroll**: Mouse wheel steps through positions (optional).
- **Visual**: Circle with indicator mark at current position. Glow on active position.

### Rendering

Phase 1: Procedural rendering (arc + indicator line + glow).
Phase 2 (future): Alien dial texture as background glyph via the texture engine.

---

## 2. Circle of Fifths Widget (`src/ui/widgets/circle_of_fifths.rs`)

Composite widget: RotaryDial (center) + key ring (outer) + degree visualizer (inner).

### Layout

```
        F
    Bb      C       ← outer ring: 12 key labels, clickable
  Eb    ╭───╮   G
      ╭─┤ ◉ ├─╮     ← center: RotaryDial (draggable)
  Ab    ╰───╯   D
    Db      A
        E
                     ← inner ring: 12 arcs, brightness = gravity_weight
```

### Fifths Order

Positions (clockwise from top): C, G, D, A, E, B, F#/Gb, Db, Ab, Eb, Bb, F

### Inner Ring (Degree Visualizer)

12 arc segments, one per chromatic degree. Brightness proportional to `gravity_weight[i]`. Root degree has accent color (amber). Non-scale degrees are dim/dark. Rotates with key selection so the root is always at the dial's indicator position.

### Interaction

- Drag center dial → key rotates through circle of fifths
- Click outer ring label → jump to that key
- Both update `base_key` and trigger gravity_weights broadcast

---

## 3. Scale Definitions (`tuning/scale.rs`)

### New Struct

```rust
pub struct ScaleDefinition {
    pub name: &'static str,
    pub short: &'static str,      // 3-char chip label
    pub group: ScaleGroup,
    pub weights: [f32; 12],
    pub hue: f32,
}

pub enum ScaleGroup {
    MajorMinor,
    Modes,
    Symmetric,
    Raga,
}
```

### 17 Western Scales

**Major/Minor**: Major, Natural Minor, Harmonic Minor, Melodic Minor
**Modes**: Dorian, Phrygian, Lydian, Mixolydian, Locrian
**Symmetric/Other**: Blues, Whole Tone, Diminished (HW), Hungarian Minor, Pentatonic Major, Pentatonic Minor, Chromatic

### Gravity Weight Convention

- Root (1): 2.0
- Fifth (5): 1.8
- Third (3/b3): 1.5
- Other scale degrees: 1.0–1.2
- Non-scale: 0.0

### Migration

Keep old 6 scales as aliases mapping to new definitions. No DNA breakage.

---

## 4. Chip Buttons (Scale Presets)

Below the circle, organized by group:

```
── Major/Minor ──
[Maj] [min] [Hmin] [Mmin]
── Modes ──
[Dor] [Phr] [Lyd] [Mix] [Loc]
── Other ──
[Blu] [PnM] [Pnm] [WTn] [Dim] [Hun] [Chr]
── Raga ──
[Bha] [Bhi] [Yam] [Jog] [Kaf]
```

Active chip is highlighted. Selecting a raga activates microtonal overlay; selecting a western scale disables it.

---

## 5. Controls Panel Layout (Redesigned)

```
╔══════════════════════════╗
║ ⚙ Controls               ║
╠══════════════════════════╣
║  ▶ ⏸ ■  │  120 BPM [━●━] ║
╠══════════════════════════╣
║                          ║
║      Circle of Fifths    ║
║    (responsive, fills    ║
║     panel width)         ║
║                          ║
╠══════════════════════════╣
║ [Maj] [min] [Dor] [Phr] ║
║ [Lyd] [Mix] [Blu] [Pen] ║
║ ── Raga ──               ║
║ [Bha] [Bhi] [Yam] [Jog] ║
╠══════════════════════════╣
║ Tala: [Tintal ▼]         ║
╚══════════════════════════╝
```

Transport stays at top. Circle of Fifths replaces key+scale dropdowns. Chip rows replace scale/raga dropdowns. Tala stays as a simple dropdown (only 3-5 options).

---

## Implementation Order

1. `src/ui/widgets/mod.rs` — widget module scaffold
2. `src/ui/widgets/rotary_dial.rs` — RotaryDial widget (drag, snap, render)
3. `src/ui/widgets/circle_of_fifths.rs` — CoF composite (dial + ring + visualizer)
4. `src/tuning/scale.rs` — ScaleDefinition struct + 17 western scales + weight convention
5. `src/ui/panels/controls.rs` — Redesign: CoF + chip rows + transport
6. `src/modules/scale_module.rs` — Accept new scale names, emit gravity_weights
7. Status bar: `[C Major]` combined format

---

## Critical Files

| File | Changes |
|------|---------|
| `src/ui/widgets/mod.rs` | **NEW** — widget module |
| `src/ui/widgets/rotary_dial.rs` | **NEW** — reusable RotaryDial widget |
| `src/ui/widgets/circle_of_fifths.rs` | **NEW** — Circle of Fifths composite |
| `src/tuning/scale.rs` | ScaleDefinition struct, 17 scales, replace 6-mode enum |
| `src/modules/scale_module.rs` | Accept expanded scale names |
| `src/ui/panels/controls.rs` | Redesign: CoF + chips + transport |
| `src/ui/panels/status_bar.rs` | Combined `[C Major]` display |
