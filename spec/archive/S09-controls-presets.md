# S09 — Controls + Presets

> The performance interface. Keyboard-driven, minimal, immediate.

## Goal

Build the egui control panels for live performance: raga selector,
tala selector, gravity override, tempo control, keyboard triggers.
Add preset save/load so interesting states can be captured.

## Ancestry (MAKE A BABY)

The Max/MSP patch had keyboard-driven performance:
- Arrow keys: preset navigation
- Enter: looper start/stop
- Spacebar: test trigger
- "p": roll mode (repeated notes)
- "b": slow + kill
- `umenu` dropdowns for audio driver, I/O settings
- `preset`/`vpreset` for state storage

We adapt the same philosophy: keyboard for performance actions,
egui panels for configuration. The performer's hands stay on
the keyboard.

## Depends On

- S02 (TuningRegistry for dropdown)
- S04 (PitchGravity for gravity slider)
- S05 (RagaRegistry for dropdown)
- S06 (TalaGrid for tempo/tala selection)
- S07 (AffinityGraph for emotion display)

## Tasks

### 9.1 Performance keyboard mapping

| Key | Action |
|-----|--------|
| Space | Toggle voice on/off (drone) |
| 1-7 | Trigger note at scale degree 1-7 |
| Up/Down | Gravity strength ±0.1 |
| Left/Right | Tempo ±5 bpm |
| R | Cycle to next raga |
| T | Cycle to next tala |
| E | Euclidean pattern density ±1 |
| D | Toggle drone mode |
| P | Panic: kill all voices, reset gravity to 0.5 |
| Esc | Stop all audio |

### 9.2 Gravity control panel (egui)

Collapsible side panel:
- **Pitch gravity**: slider 0.0–1.0 with live value
- **Rhythm gravity**: slider 0.0–1.0
- **Gamaka depth**: slider 0.0–1.0
- **Morph speed**: slider 0.1–2.0
- **Auto mode**: checkbox — when checked, gravity driven by emotion
  (S08 GravityState); when unchecked, manual sliders override

### 9.3 Tuning/raga panel

- **Scale**: dropdown of all loaded TuningSystems
- **Raga**: dropdown of loaded RagaModes (filtered to match current scale)
- **Root Hz**: slider 200–520 (default 261.63)
- Current note display: show nearest degree name + Hz

### 9.4 Rhythm panel

- **Tala**: dropdown of loaded TalaDefinitions
- **Tempo**: slider 40–200 bpm
- **Swing**: slider 0.0–0.5
- **Euclidean hits**: slider 0–beats
- Beat position visualizer: row of dots, current beat highlighted

### 9.5 Affinity inspector (read-only for now)

- Module emotion gauges: valence bar, arousal bar
- Edge weight list: top 10 strongest edges with weights
- Last 10 ledger events
- This is display only — no editing of affinity state yet

### 9.6 Preset system

```rust
pub struct Preset {
    pub name: String,
    pub raga: String,
    pub tala: String,
    pub root_hz: f64,
    pub tempo_bpm: f64,
    pub pitch_gravity: f32,
    pub rhythm_gravity: f32,
    pub gamaka_depth: f32,
    pub swing: f32,
    pub euclidean_hits: u32,
}
```

- Save: Ctrl+S → prompt for name, serialize to JSON
- Load: Ctrl+1 through Ctrl+9 → load preset by index
- Preset list: egui side panel showing saved presets
- Presets stored in `assets/presets/` as JSON files
- Ship 3 default presets:
  - "Dawn Bhairav": bhairav, teentaal, 60bpm, gravity=0.7
  - "Drifting Jog": jog, rupak, 90bpm, gravity=0.3
  - "Texture Mode": jog, freeform, gravity=0.0

### 9.7 Status bar

Bottom of screen, always visible:
```
[Bhairav] [Teentaal 120bpm] [G:0.7] [♩=4] [voices:3/8]
```

Shows: current raga, tala+tempo, gravity, current beat, active voices.

## Files Created

```
src/ui/mod.rs              — pub mod controls, presets, status;
src/ui/controls.rs         — gravity/tuning/rhythm panels
src/ui/presets.rs          — Preset struct, save/load
src/ui/status.rs           — bottom status bar
assets/presets/*.json      — 3 default presets
```

## Files Modified

```
src/main.rs                — add `mod ui;`
src/app.rs                 — keyboard dispatch, panel rendering, preset logic
```

## Verification

1. All keyboard shortcuts work as documented
2. Raga dropdown: switching ragas changes audible pitch quantization
3. Tala dropdown: switching talas changes rhythm pattern
4. Gravity slider: moving it audibly changes pitch snap strength
5. Tempo slider: beat rate changes smoothly
6. Beat visualizer: dots animate in time with audio
7. Save preset → close app → reopen → load preset → same state
8. Panic key: all voices stop, gravity resets, blob visuals calm
9. Status bar updates in real time
10. No UI lag from egui panels during active audio
