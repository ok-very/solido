# S26 — Six-Organism Integration + acidBros UI

**Layer**: L5 + L6
**Depends on**: S23 (ACID), S24 (TBLK), S25 (KKIT) — all six organisms complete
**Status**: Planned

## Implementation Plan

Four areas, in order. Tests after each area.

---

### Area 1 — Gain Staging

**File**: `src/audio/gain_staging.rs`

Add per-species constants:
```rust
pub const HOSO_GAIN: f32 = 0.65;
pub const SPGL_GAIN: f32 = 0.45;
pub const TBLK_GAIN: f32 = 0.65;
pub const KKIT_GAIN: f32 = 0.70;
pub const MASTER_GAIN: f32 = 0.65;  // was 0.5 — too quiet with 6 organisms
```

Update `species_gain()` to cover all six species (match on uppercase species prefix):
```rust
pub fn species_gain(name: &str) -> f32 {
    let upper = name.to_uppercase();
    if upper.contains("DRON") { DRON_GAIN }
    else if upper.contains("ACID") { ACID_GAIN }
    else if upper.contains("HOSO") { HOSO_GAIN }
    else if upper.contains("SPGL") { SPGL_GAIN }
    else if upper.contains("TBLK") { TBLK_GAIN }
    else if upper.contains("KKIT") { KKIT_GAIN }
    else { DEFAULT_ORG_GAIN }
}
```

**DNA mixer_cell gain updates** (per-organism internal level before VoiceBus):
- `dron-alpha.json`: mixer gain `0.7 → 0.5` (background pad)
- `hoso-malabar.json`: mixer gain keep at `0.8` (sequenced line needs presence)
- `spgl-kepler.json`: mixer gain `0.7 → 0.4` (texture, not foreground)
- `acid-kinoko.json`: mixer gain keep at `0.7` (lead bass needs presence)
- `tblk-dha.json`: mixer gain `0.75 → 0.6` (percussion, transient peaks)
- `kkit-909.json`: mixer gain `0.75 → 0.7` (drums need punch)

**Tests**:
- `species_gain_covers_all_six`: verify each of the 6 species names returns a non-DEFAULT gain
- `per_org_headroom`: each organism solo peak (mixer_gain × species_gain × MASTER_GAIN) < 0.85
- `six_org_sum_below_clipping`: worst-case sum of 6 peaks < 1.0

---

### Area 2 — Visual Identity Fix

**Files**: `assets/dna/*.json` (render section only)

Current hue collisions: ACID (0.0), KKIT (0.0), TBLK (0.05) all red — indistinguishable.

DNA render section changes:

| File | hue before | hue after | pulse_response before | pulse_response after |
|------|-----------|-----------|----------------------|---------------------|
| `acid-kinoko.json` | 0.0 | **0.35** (green) | 0.8 | 0.9 |
| `dron-alpha.json` | 0.6 | 0.6 ✓ | 0.3 | **0.2** |
| `hoso-malabar.json` | 0.15 | 0.1 | 0.5 | 0.5 ✓ |
| `kkit-909.json` | 0.0 | 0.0 ✓ | 0.9 | 0.9 ✓ |
| `spgl-kepler.json` | 0.65 | **0.75** (violet) | 0.3 | **0.1** |
| `tblk-dha.json` | 0.05 | 0.05 ✓ | 0.7 | 0.7 ✓ |

No Rust code changes — visual identity plumbing already wired (`app.rs:189,193` reads `dna.render.hue` and `dna.render.pulse_response` into `OrganismState`).

**Tests**:
- `visual_dna_loads`: parse each DNA file, assert expected hue values
- `six_blobs_distinct_hues`: verify no two organisms share hue within 0.1

---

### Area 3 — Tape Delay UI

**Files to modify**: `src/ui/panels/organism_panel.rs`, `src/app.rs`

#### organism_panel.rs

Add after `ReverbBusUiState`:
```rust
/// Tape delay bus UI state (global, not per-organism).
pub struct TapeDelayBusUiState {
    pub return_level: Shared,
    pub params: Vec<(String, Shared)>,  // time, feedback, hf_damp
}
```

Add `tape_delay_send: Option<Shared>` field to `OrganismUiState`.

Add `tape_delay_bus: Option<TapeDelayBusUiState>` field to `OrganismPanelState`.

In `show_organism()` UI function: add tape delay send slider after reverb send slider (same
pattern — only show when `Some`).

In the panel `show()` function: add collapsible "Tape Delay" section after the "Reverb" section
with return_level slider + time/feedback/hf_damp sliders.

#### app.rs

- Rename `_tape_delay_bus_handles: Option<TapeDelayBusHandles>` → `tape_delay_bus_handles`
- In the organism panel build loop (around line 250): populate `tape_delay_send` from
  `endpoint.tape_delay_send.clone()` (same pattern as `reverb_send`)
- After building `reverb_bus_state`, build `tape_delay_bus_state`:
  ```rust
  let tape_delay_bus_state = tape_delay_bus_handles.as_ref().map(|h| TapeDelayBusUiState {
      return_level: h.return_level.clone(),
      params: h.params.clone(),
  });
  ```
- Store in `OrganismPanelState { ..., tape_delay_bus: tape_delay_bus_state }`

**Tests**:
- `tape_delay_bus_ui_state_builds`: construct a TapeDelayBusUiState from mock Shared handles

---

### Area 4 — drone_bed Deprecation

**Files**: `src/dsp/cell/drone_bed.rs`, `src/dsp/cell/mod.rs`

In `drone_bed.rs`, add above the struct definition:
```rust
#[deprecated(since = "0.6.0", note = "Use composable DRON (osc_cell + filter_cell + lfo_cell + mixer_cell) instead. See assets/dna/dron-alpha.json for the composable version.")]
```

In `mod.rs` `CellRegistry::new()`: remove the `drone_bed` registration block (both `reg.register(...)` and `reg.register_ranges(...)` calls). The module declaration `pub mod drone_bed` stays so its unit tests still compile.

**Tests**:
- Existing `drone_bed` tests continue to pass (module still compiles, just unregistered)
- `drone_bed_not_in_registry`: `CellRegistry::new().build(&drone_bed_dna, 44100.0)` returns `None`

---

### Deferred from S26

| Item | Reason | Future |
|------|---------|--------|
| Step grid UI editing | Complex egui widget work | S28 |
| Transport controls | Needs SequencerModule clock wiring | S28 |
| Oscilloscope | Optional visual polish | S29+ |
| Social dynamics matrix | Emergent from affinity graph, not hard-coded | Ongoing |
| Bus decay calibration | FunDSP reverb time scaling, feedback ceiling | **S27** |

---

## Goal

Integrate all six organisms into a single session. Gain staging audit. Visual identity DNA per species. acidBros UI elements (step grid, transport, oscilloscope). Deprecate `drone_bed`.

---

## Six Organisms Running Together

| Species | Session | Aesthetic | Fidelity | Role |
|---------|---------|-----------|----------|------|
| **DRON** | S20 | Ambient drone | 0.3 | Harmonic substrate |
| **HOSO** | S21 | Cochin Moon | 0.9 | Clinical sequenced bass/lead |
| **SPGL** | S22 | Expanding Universe | 0.1 | Slow evolving texture |
| **ACID** | S23 | Acid Mt. Fuji | 0.8 | Squelchy 303 bass |
| **TBLK** | S24 | Indian tabla | 0.5 | Organic polyrhythmic percussion |
| **KKIT** | S25 | TR-909 | 0.95 | Mechanical drum grid |

---

## Gain Staging Audit

With six organisms summing into the master bus, gain staging is critical.

### Target Levels

| Stage | Peak dBFS | Notes |
|-------|-----------|-------|
| Per-cell output | -12 | Individual cells stay well below 0 |
| Per-organism mix | -6 | mixer_cell gain keeps organism headroom |
| 6-organism sum | -3 | Combined peak before master bus |
| Master bus output | -1 | Limiter ceiling |

### Audit Steps

1. **Solo each organism** — measure peak and RMS over 30 seconds
2. **Pair combinations** — verify no pair clips without limiter
3. **All six together** — verify master bus limiter handles peaks
4. **Adjust mixer_cell gains** — per-organism levels in DNA presets

### Per-Organism Gain Targets

| Organism | mixer_cell gain | Rationale |
|----------|----------------|-----------|
| DRON | 0.5 | Background pad — should sit below everything |
| HOSO | 0.6 | Mid-level sequenced line |
| SPGL | 0.4 | Texture, not foreground |
| ACID | 0.7 | Lead bass — needs presence |
| TBLK | 0.6 | Percussion — transient peaks |
| KKIT | 0.7 | Drums — need punch through the mix |

These are starting points. The audit may adjust them.

---

## Visual Identity DNA

Each species gets visual parameters in their DNA that drive blob rendering.

### Visual Parameters

| Species | Hue | Body Shape | Movement | Blob Size |
|---------|-----|------------|----------|-----------|
| DRON | 0.6 (blue) | Large, diffuse, soft edges | Slow, heavy, breathing | 1.5x |
| HOSO | 0.1 (amber) | Medium, angular, defined edges | Precise, mechanical | 1.0x |
| SPGL | 0.75 (violet) | Large, nebulous, cloudy | Drifting, glacial | 1.8x |
| ACID | 0.35 (green) | Small, spiky, sharp edges | Darting, reactive | 0.7x |
| TBLK | 0.05 (red) | Medium, sharp, faceted | Impulsive bursts | 0.9x |
| KKIT | 0.0 (red-orange) | Compact, dense, hard | Mechanical, snapping | 0.6x |

### DNA Visual Section

```json
{
  "visual": {
    "hue": 0.35,
    "body": "spiky",
    "size_scale": 0.7,
    "edge_sharpness": 0.8,
    "movement_speed": 1.5,
    "pulse_response": 0.9
  }
}
```

`pulse_response` controls how much the blob reacts to audio transients:
- KKIT (0.9): snaps sharply on every hit
- DRON (0.2): barely pulses, slow breathing
- SPGL (0.1): almost no transient response

---

## acidBros UI Elements

### Step Grid (egui)

**File**: `src/ui/sequencer_grid.rs` (deferred from S19, created here)

**Note**: The step grid UI was deferred from S19 (dialogue architecture) to S26, when all six organisms exist for visualization. S19 completed the SequencerModule backend; S26 builds the UI.

The step grid is the primary pattern interface with **organism response overlay**:

```
┌──────────────────────────────────────────────────┐
│  ▶ 130 BPM  [16]  Swing: [====]                  │
├──────────────────────────────────────────────────┤
│  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16│
│  ■  □  □  ■  ■  □  ■  □  ■  □  □  ■  ■  □  □  ■│  ← gates (click to toggle)
│  ▲        ▲     ▲        ▲        ▲  ▲        ▲│  ← accents
│  ~     ~        ~     ~        ~     ~          │  ← slides
├──────────────────────────────────────────────────┤
│  ACID ████░░████████░░████░░░░████████████░░░░██│  ← green, high fidelity
│  DRON ████████████████████████████████████████████│  ← blue, continuous
│  KKIT ██░░██░░██░░██░░██░░██░░██░░██░░██░░██░░██│  ← red, mechanical grid
│  TBLK █░░░██░░░░█░░██░░█░░░██░░░░█░░██░░░░█░░░░│  ← amber, polyrhythmic
│  HOSO ████░░████████░░████░░░░████████████░░░░██│  ← yellow, follows closely
│  SPGL ████████████████████████████████████████████│  ← violet, ignores pattern
├──────────────────────────────────────────────────┤
│  ▲ step 5                                        │
└──────────────────────────────────────────────────┘
```

**Implementation**:
- Top section: editable pattern (click gates, drag pitch, toggle accent/slide)
- Organism rows: read-only RMS envelopes color-coded by species hue
- Playhead indicator: current step highlighted
- Per-step pitch display: note name or Hz

### Transport Controls

**File**: `src/ui/transport.rs` (new)

Integrated into status bar:

```
[▶] [■] [●REC]  130 BPM  [◄ ═══════●══ ►]  |  Step 5/16
```

- Play/Stop/Record buttons
- BPM display + slider (20–300)
- Drives SequencerModule clock via Shared handle
- Record: captures live keyboard input into step grid

### Bus Mix Controls

**File**: `src/ui/organism_panel.rs` (per-organism sends) + inline in the existing bus panel

Two send buses need UI:

**Per-organism (in each organism's strip in organism_panel):**
- Reverb send level slider — already has `OrganismEndpoint.reverb_send: Option<Shared>`
- Tape delay send level slider — `OrganismEndpoint.tape_delay_send: Option<Shared>`

Both sliders appear only when the corresponding bus is active (i.e., `Some`). Range [0.0, 1.0].

**Global bus params (collapsible section in the panel):**

Reverb bus:
- Return level — `ReverbBusHandles.return_level: Shared` [0.0, 1.0]
- Size, decay, damp — `ReverbBusHandles.params: HashMap<String, Shared>`

Tape delay bus:
- Return level — `TapeDelayBusHandles.return_level: Shared` [0.0, 1.0]
- Time — [0.05, 1.0] seconds
- Feedback — [0.0, 0.95]
- HF damp — [0.0, 1.0]

These handles are already wired through `OrganismEndpoint` and `AudioSubstrate::new()` return values — they just need UI bindings in the panel.

**Note**: `TapeDelayBusHandles` is already returned from `AudioSubstrate::new()` and stored in `SolidoApp._tape_delay_bus_handles` from S23. In S26, promote it from `_tape_delay_bus_handles` (ignored) to an active UI state field.

---

### Oscilloscope (deferred if time-tight)

**File**: `src/ui/oscilloscope.rs` (new, optional)

CRT-style waveform display from organism analysis data:
- Circular buffer of last 2048 samples per organism
- Rendered as egui polyline with CRT phosphor color
- Trigger mode: rising zero-crossing for stable display
- Optional: FFT spectrum view toggle

### Rotary Knobs (deferred if time-tight)

Optional upgrade from sliders in organism_panel. Classic synth-style rotary encoders:
- Drag up/down to change value
- Double-click to reset to default
- Right-click for exact value entry

---

## drone_bed Deprecation

`drone_bed` is replaced by the composable DRON (osc_cell + filter_cell + lfo_cell + mixer_cell from S20).

### Migration

1. Keep `drone_bed.rs` but mark as `#[deprecated]`
2. `dron-alpha.json` remains loadable (backwards compatibility)
3. `dron-composable.json` becomes the default DRON preset
4. Remove `drone_bed` from CellRegistry's default factory list
5. Delete `drone_bed.rs` in a future cleanup session

---

## Social Dynamics: Six-Organism Matrix

```
         DRON    HOSO    SPGL    ACID    TBLK    KKIT
DRON    [+0.3]   weak    med     weak    weak    none
HOSO    strong  [+0.1]   none    med     weak    weak
SPGL    med      none   [-0.2]   none    none    none
ACID    weak     med     none   [+0.0]   strong  strong
TBLK    weak     weak    none    strong  [-0.3]  strong
KKIT    none     weak    none    strong  strong  [+0.1]
```

### Emergent Behaviors (six organisms)

1. **ACID↔KKIT lock-in**: Both high-fidelity to sequencer, ACID's bass locks to KKIT's kick pattern. Accent correlation strengthens edge.

2. **TBLK↔KKIT polyrhythm**: TBLK's organic patterns play against KKIT's grid. When combined rhythm satisfies both, sync edge strengthens.

3. **DRON harmonic field**: DRON's slow drift sets the harmonic context. HOSO's filter tracks DRON's frequency. ACID's root note gravitates toward DRON.

4. **SPGL independence**: SPGL barely participates in the social graph. Its func_gen_cells dominate. Occasionally its pitch drift pulls others toward it via weak learned edges.

5. **HOSO clinical follower**: HOSO mirrors the sequencer precisely but through its own PWM filter character. Provides the "human intent faithfully rendered" contrast to ACID's interpretive following.

6. **TBLK isolation cycles**: TBLK burns energy, goes quiet, regenerates near other organisms. Creates macro-level rhythmic tension.

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `src/ui/sequencer_grid.rs` | Modify — add organism response overlay |
| `src/ui/transport.rs` | Create — play/stop/BPM transport controls |
| `src/ui/oscilloscope.rs` | Create (optional) — CRT waveform display |
| `src/ui/organism_panel.rs` | Modify — per-organism reverb/tape-delay send sliders, rotary knobs (optional), response display |
| `src/dsp/cell/drone_bed.rs` | Modify — add `#[deprecated]` attribute |
| `src/dsp/cell_registry.rs` | Modify — remove drone_bed from default factory |
| `assets/dna/*.json` | Modify — add visual identity section, adjust mixer gains |
| `src/organism/module.rs` | Modify — visual DNA → blob renderer parameters |
| `src/renderer/blob_renderer.rs` | Modify — species-specific body shapes + movement |

---

## Test Plan (~10 tests)

### Gain staging
- `six_org_sum_below_zero`: combined output peak < 0 dBFS before limiter
- `per_org_headroom`: each organism solo peaks below -6 dBFS
- `limiter_catches_peaks`: master bus output never exceeds -1 dBFS

### Integration
- `all_six_load`: all 6 DNA presets load without error
- `all_six_produce_audio`: all 6 organisms simultaneously produce non-zero output
- `sequencer_drives_all`: SequencerModule signals reach all connected organisms

### Visual
- `visual_dna_loads`: visual section of DNA presets deserializes correctly
- `six_blobs_distinct`: blob renderer assigns different hue/size to each species

### UI
- `transport_updates_bpm`: transport BPM slider updates SequencerModule clock
- `step_grid_shows_response`: organism response overlay receives actual_pitch data

---

## Verification Criteria

- [ ] All 6 organisms running simultaneously without clipping
- [ ] Gain staging: per-organism < -6 dBFS, sum < -3 dBFS, master output < -1 dBFS
- [ ] Each species has distinct visual identity (hue, body, movement)
- [ ] Step grid shows human pattern + per-organism response overlay
- [ ] Transport controls drive SequencerModule
- [ ] drone_bed deprecated, composable DRON is default
- [ ] Social dynamics produce emergent musical behavior
- [ ] `cargo test` — all tests pass
