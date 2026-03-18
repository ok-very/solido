# UI: Western Scales + Transport Revision

**Status**: Spec (blocks pre-union-iteration Phase A)
**Depends on**: S41 (raga activation), existing scale.rs + controls.rs
**Blocks**: pre-union-iteration (all environments need key/scale selection)
**Priority**: Immediate

---

## Goal

Expand the scale system from 6 abstract modes to a full western scale vocabulary (major, minor, modes, blues, etc.) and consolidate transport + key + scale + raga into a coherent top-level panel. Users should be able to set the musical key and mode of the ecology with one dropdown, see the active scale degrees visualized, and have the gravity system respond immediately.

---

## Current State

**6 scales** in `tuning/scale.rs`: Chromatic, Diatonic, Pentatonic, Octatonic, Middle Eastern, Quartal. These are abstract groupings — "Diatonic" doesn't distinguish between major and dorian. The gravity_weights system already accepts any 12-element array, so adding scales is purely definitional.

**Key selection**: 12 pitch classes (C through B) in controls.rs dropdown. Stored as `base_key: u8`.

**Raga selection**: 5 ragas with microtonal overlays, separate from western scales.

**Transport**: BPM slider (20-300), Play/Pause/Stop buttons, all in the small Controls panel.

---

## 1. Western Scale Definitions

Add to `tuning/scale.rs`. Each scale is a named `[f32; 12]` gravity_weights array. Weight 0.0 = excluded degree, higher values = stronger gravitational pull.

### 1a. Major/Minor Family

| Name | Degrees | Notes (from C) |
|------|---------|----------------|
| Major (Ionian) | 1 2 3 4 5 6 7 | C D E F G A B |
| Natural Minor (Aeolian) | 1 2 b3 4 5 b6 b7 | C D Eb F G Ab Bb |
| Harmonic Minor | 1 2 b3 4 5 b6 7 | C D Eb F G Ab B |
| Melodic Minor (asc) | 1 2 b3 4 5 6 7 | C D Eb F G A B |

### 1b. Modal Family

| Name | Degrees | Character |
|------|---------|-----------|
| Dorian | 1 2 b3 4 5 6 b7 | Jazz minor, bittersweet |
| Phrygian | 1 b2 b3 4 5 b6 b7 | Spanish, dark |
| Lydian | 1 2 3 #4 5 6 7 | Bright, ethereal |
| Mixolydian | 1 2 3 4 5 6 b7 | Dominant, blues-rock |
| Locrian | 1 b2 b3 4 b5 b6 b7 | Diminished, unstable |

### 1c. Symmetric & Other

| Name | Degrees | Character |
|------|---------|-----------|
| Blues | 1 b3 4 b5 5 b7 | 6-note blues |
| Whole Tone | 1 2 3 #4 #5 b7 | Dreamlike, no resolution |
| Diminished (HW) | 1 b2 b3 3 #4 5 6 b7 | 8-note alternating |
| Hungarian Minor | 1 2 b3 #4 5 b6 7 | Exotic, dramatic |
| Pentatonic Major | 1 2 3 5 6 | Open, folk |
| Pentatonic Minor | 1 b3 4 5 b7 | Universal, grounded |
| Chromatic | all 12 | Free, no gravity |

### 1d. Gravity Weight Convention

- **Root (1)**: weight = 2.0 (strongest pull)
- **Fifth (5)**: weight = 1.8
- **Third (3/b3)**: weight = 1.5
- **Other scale degrees**: weight = 1.0-1.2
- **Non-scale degrees**: weight = 0.0

These weights determine how strongly organisms' pitch is pulled toward each degree. Higher = more magnetic. Zero = pitch avoids that degree.

---

## 2. Scale Selection UI

### 2a. Grouped Dropdown

Replace the current flat dropdown with a grouped selector:

```
Scale: [Major ▼]
  ── Major/Minor ──
  Major
  Natural Minor
  Harmonic Minor
  Melodic Minor
  ── Modes ──
  Dorian
  Phrygian
  Lydian
  Mixolydian
  Locrian
  ── Other ──
  Blues
  Pentatonic Major
  Pentatonic Minor
  Whole Tone
  Diminished
  Hungarian Minor
  Chromatic
  ── Raga ──
  Bhairav
  Bhairavi
  Yaman
  Jog
  Kafi
```

Ragas are in the same dropdown — selecting a raga activates microtonal overlay. Selecting a western scale disables microtonal overlay.

### 2b. Scale Degree Visualizer

Below the dropdown, a 12-segment horizontal bar showing active degrees:

```
C  C# D  D# E  F  F# G  G# A  A# B
██       ██    ██ ██    ██    ██    ██   ← Major (bright segments)
```

Each segment's brightness/height proportional to gravity_weight. Zero-weight segments are dim/empty. Root degree (current key) is highlighted with accent color.

### 2c. Key + Scale Combined Display

Status bar format changes from `[Key:C] [Diatonic]` to `[C Major]` or `[A Dorian]` — combined, concise.

---

## 3. Transport Panel Consolidation

Move transport controls from the small Controls panel into a dedicated top-bar or always-visible region:

### 3a. Layout

```
┌─────────────────────────────────────────────────────┐
│ ▶ ■  │ 120 BPM [━━━━━━━━━●━━━━]  │ C Major ▼  │ ⚙ │
│       │ [/] nudge                  │ scale viz   │   │
└─────────────────────────────────────────────────────┘
```

- **Left**: Play/Stop buttons (compact icons)
- **Center-left**: BPM with slider, keyboard shortcut hint
- **Center-right**: Key + Scale combined dropdown + degree visualizer
- **Right**: Settings gear (opens full controls panel)

### 3b. Raga indicator

When a raga is selected instead of a western scale, the degree visualizer shows the raga's gravity weights with a "Raga" badge and the raga's hue color.

---

## 4. Implementation

### 4a. `tuning/scale.rs` Changes

```rust
pub struct ScaleDefinition {
    pub name: &'static str,
    pub group: &'static str,       // "Major/Minor", "Modes", "Other", "Raga"
    pub weights: [f32; 12],
    pub hue: f32,                   // UI color identity
}

pub const WESTERN_SCALES: &[ScaleDefinition] = &[
    ScaleDefinition { name: "Major", group: "Major/Minor", weights: [...], hue: 0.15 },
    // ...
];
```

Replace the current 6-mode enum with the expanded `WESTERN_SCALES` array. The ScaleModule reads from this array instead of the current match block.

### 4b. `controls.rs` Changes

- Replace flat ComboBox with grouped selector
- Add scale degree visualizer widget (12 colored rects)
- Consolidate key+scale into single line

### 4c. `app.rs` Changes

- Status bar: `[C Major]` combined format
- Transport: Optionally render in a top-bar region instead of a floating panel

---

## Critical Files

| File | Changes |
|------|---------|
| `src/tuning/scale.rs` | ScaleDefinition struct, WESTERN_SCALES array (17 scales), replace 6-mode enum |
| `src/modules/scale_module.rs` | Read from WESTERN_SCALES, emit gravity_weights for selected scale |
| `src/ui/panels/controls.rs` | Grouped dropdown, scale degree visualizer, transport consolidation |
| `src/ui/panels/status_bar.rs` | Combined `[C Major]` display |
| `src/ui/mod.rs` | Optional top-bar transport region |
