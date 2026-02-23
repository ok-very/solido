# L5-S10 — UX Shell + Integration

> Press a key. The system breathes.

## Goal

Build the performance interface: per-module egui inspectors, global
controls, presets, ledger view, module palette, keyboard performance
mapping, and the Hosono test. This is the UX shell that wraps the
entire module system and the integration test that proves it all works.

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
egui panels for configuration. The performer's hands stay on the keyboard.

## Depends On

Everything. This is the integration session.

## Tasks

### 10.1 Module inspector framework

Create `src/ui/inspector.rs` — auto-generated inspector for any Module:

```rust
pub fn draw_module_inspector(
    ui: &mut egui::Ui,
    module: &dyn ModuleCore,
    emotion: &ModuleEmotion,
    edges: &[(EdgeId, f32)],  // connected edges + weights
    ledger: &[LedgerEvent],
) {
    // === Auto-generated section (always shown) ===
    // Port list: input/output names, types, current values
    // Emotion gauges: valence bar [-1,1], arousal bar [0,1]
    // Edge weight list: connected modules + weights
    // Last 10 ledger events for this module

    // === Custom section (ModuleUi::ui()) ===
    // If the module implements ModuleUi (feature-gated behind ui-egui),
    // its custom panel renders below the auto-generated header.
    // module_ui.ui(ui);
}
```

Click a blob → its inspector opens as an egui side panel.
The auto-generated header shows port connections and emotion state.
Below it, modules that implement `ModuleUi` (feature-gated behind `ui-egui`)
provide domain-specific controls via their custom `.ui()` panel.

### 10.2 Module palette

Create `src/ui/module_palette.rs`:

Shows all available Module types as a draggable palette:
- Keyboard Input
- Cursor Input
- Audio Analysis
- Quantizer
- Voice
- Tala
- Raga
- Camera
- Pixel Probe
- Video File
- LLaVA
- Tool Glyph
- Data Diagram
- ASCII Texture
- ISF Visual (+ list of available ISF shaders)

Drag a module type onto the canvas → `SeedReactor.register()` →
it appears as a new blob, starts forming affinity edges with
nearby modules. This is the "capture sketches in modules and see
how they interact" workflow.

### 10.3 Performance keyboard mapping

| Key | Action |
|-----|--------|
| Space | Toggle drone voice on/off |
| 1-7 | Trigger note at scale degree 1-7 |
| Up/Down | Gravity strength ±0.1 |
| Left/Right | Tempo ±5 bpm |
| R | Cycle to next raga |
| T | Cycle to next tala |
| E | Euclidean pattern density ±1 |
| D | Toggle drone mode |
| P | Panic: kill all voices, reset gravity to 0.5 |
| Esc | Stop all audio |
| F1 | Toggle inspector panel |
| F2 | Toggle module palette |
| F3 | Toggle ledger view |

### 10.4 Global control panel (egui)

Create `src/ui/controls.rs` — collapsible side panel:

**Pitch gravity**: slider 0.0–1.0 with live value
**Rhythm gravity**: slider 0.0–1.0
**Gamaka depth**: slider 0.0–1.0
**Morph speed**: slider 0.1–2.0
**Auto mode**: checkbox — when checked, gravity driven by emotion
(GravityState); when unchecked, manual sliders override

**Scale**: dropdown of all loaded TuningSystems
**Raga**: dropdown of loaded RagaModes (filtered to match current scale)
**Root Hz**: slider 200–520 (default 261.63)

**Tala**: dropdown of loaded TalaDefinitions
**Tempo**: slider 40–200 bpm
**Swing**: slider 0.0–0.5
**Euclidean hits**: slider 0–beats
**Beat position visualizer**: row of dots, current beat highlighted

### 10.5 Affinity inspector (read-only)

Part of the inspector framework:
- Module emotion gauges: valence bar, arousal bar
- Edge weight list: top 10 strongest edges with weights
- Last 10 ledger events
- Display only — no editing of affinity state yet

### 10.6 Ledger view

Create `src/ui/ledger_view.rs`:

Scrolling panel showing recent ledger events:
- Timestamp, edge ID, event type, weight change, reason
- Filter by module, event type, or edge
- Color-coded: green=strengthened, red=weakened, blue=explored, gray=pruned

### 10.7 Preset system

```rust
pub struct Preset {
    pub name: String,
    pub modules: Vec<ModulePreset>,  // which modules are instantiated
    pub edges: Vec<(EdgeId, f32)>,   // saved affinity weights
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

### 10.8 Status bar

Bottom of screen, always visible:
```
[Bhairav] [Teentaal 120bpm] [G:0.7] [♩=4] [voices:3/8] [modules:7] [edges:12]
```

Shows: current raga, tala+tempo, gravity, current beat, active voices,
module count, edge count.

### 10.9 Automation script — The Hosono Test

Create `src/automation.rs`:

```rust
pub struct AutomationStep {
    pub time_sec: f32,
    pub action: AutoAction,
}

pub enum AutoAction {
    SetRaga(String),
    SetGravity(f32),
    SetTempo(f64),
    SetEuclideanHits(u32),
    SpawnDrone,
    KillAll,
    MorphRaga(String, u32),  // target, morph_blocks
    RegisterModule(String),  // module type name
    SetModuleParam(ModuleId, String, f32),
}
```

The Hosono Test sequence:
```
 0s  SetRaga("bhairav"), SetGravity(0.8), SpawnDrone, SetTempo(72)
     SetEuclideanHits(5)
     // Locked Bhairav: clear scale, steady tala, defined groove

30s  SetGravity(0.5)
     // Loosening: pitches start to bend between degrees

40s  SetGravity(0.2), SetEuclideanHits(2)
     // Dissolving: mostly free pitch, sparse triggers

50s  SetGravity(0.05)
     // Pure texture: microtonal drift, no recognizable scale

60s  MorphRaga("yaman", 120), SetGravity(0.3)
     // Beginning to coalesce: new gravity weights fading in

70s  SetGravity(0.6), SetEuclideanHits(4)
     // Reforming: Yaman intervals emerging, rhythm returning

80s  SetGravity(0.8), SetTempo(90)
     // Locked Yaman: bright evening raga, faster tempo

90s  — end of test —
```

Trigger with a hotkey (F5) or from the UI.

### 10.10 Recording

Use the existing `Recorder` from 0.5 to capture:
- Visual frames (blob renderer output)
- State log: gravity, raga, tempo, voice count per frame
- Audio capture (via cpal; WAV file writer or system capture)

### 10.11 Evaluation criteria

**Pass/Fail (automated checks):**
- [ ] No audio underruns during 90s test
- [ ] No crashes or panics
- [ ] Voice count stays <= max_voices
- [ ] Gravity values change at scheduled times
- [ ] Raga morph completes without error
- [ ] Frame rate stays above 30fps throughout

**Subjective (human evaluation):**
- [ ] 0-30s: Can you identify it as Bhairav? (komal Re/Dha audible)
- [ ] 40-50s: Does the dissolution sound intentional, not broken?
- [ ] 50-60s: Does it feel like ambient texture, not noise?
- [ ] 60-80s: Can you hear Yaman emerging? (teevra Ma audible)
- [ ] 80-90s: Is the groove re-established?
- [ ] Overall: Does the system sound like it *meant* to do that?

**Visual checks:**
- [ ] 0-30s: Sharp-edged blobs, pulsing with beat, cool-warm palette
- [ ] 40-50s: Softening edges, glow increasing
- [ ] 50-60s: Diffuse glowing blobs, hot palette
- [ ] 60-80s: Edges reforming, new color temperature (Yaman hue)
- [ ] 80-90s: Sharp blobs again, new rhythm pulse, new palette

### 10.12 Edge cases to verify

- Rapid raga switching (< 1 second): morph handles gracefully
- Gravity 0→1 snap: no audio discontinuities
- All voices killed then respawned: clean recovery
- Tempo change during active pattern: no timing glitches
- Running for 10+ minutes: stable, no memory growth
- Adding module mid-performance: no audio glitches
- Removing module mid-performance: edges prune cleanly

### 10.13 Performance profiling

- Audio thread CPU usage
- Control thread (gravity/affinity) CPU usage
- GPU frame time
- Memory allocation rate (should be ~0 in steady state)
- Affinity graph tick time (should be < 1ms for < 20 modules)

## Files Created

```
src/ui/mod.rs              — pub mod inspector, controls, ledger_view, presets, module_palette;
src/ui/inspector.rs        — auto-gen + custom Module inspector
src/ui/controls.rs         — global gravity/tuning/rhythm panels
src/ui/ledger_view.rs      — scrolling ledger panel
src/ui/presets.rs          — Preset struct, save/load
src/ui/module_palette.rs   — draggable module palette
src/automation.rs          — AutomationStep, AutoAction, Hosono test sequence
assets/presets/*.json      — 3 default presets
```

## Files Modified

```
src/main.rs                — add `mod ui; mod automation;`
src/app.rs                 — keyboard dispatch, panel rendering, preset logic,
                             automation runner (triggered by hotkey)
```

## Verification

1. All keyboard shortcuts work as documented
2. Module palette: drag module → new blob appears → edges form
3. Click blob → inspector opens with ports, emotions, edges, ledger
4. Raga dropdown: switching ragas changes audible pitch quantization
5. Tala dropdown: switching talas changes rhythm pattern
6. Gravity slider: moving it audibly changes pitch snap strength AND blob edges
7. Tempo slider: beat rate changes smoothly
8. Beat visualizer: dots animate in time with audio
9. Save preset → close app → reopen → load preset → same state
10. Panic key (P): all voices stop, gravity resets, blobs calm
11. Status bar updates in real time
12. Ledger view: see weight changes scrolling in real time
13. No UI lag from egui panels during active audio
14. **The Hosono Test passes all automated and subjective criteria**

## What Success Looks Like

You press F5. The system starts playing Bhairav: you can hear the
komal Re, the strong Ma, the steady 16-beat pulse. The blobs are
sharp-edged and pulsing, cool-toned. Over 30 seconds, the pitch
starts to wander. The rhythm thins. The blobs soften, edges blurring,
colors warming. By a minute in, it's pure microtonal shimmer —
ambient, floating, no scale. The blobs glow diffusely, hot palette.

Then new intervals appear: brighter, more open. The teevra Ma of
Yaman. The rhythm firms up. The blobs reform, edges sharpening,
a new color temperature. By 90 seconds you're in a different raga,
a different mood, and the transition felt like the system exhaled
and inhaled.

Camera motion drives arousal which drives gravity which drives
both sound and vision. LLaVA descriptions scroll through the
inspector panel. Affinity edges pulse with activity. The ledger
records everything.

That's the Hosono Test.
