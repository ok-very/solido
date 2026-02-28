# S26 — Six-Organism Integration + acidBros UI

**Layer**: L5 + L6
**Depends on**: S23 (ACID), S24 (TBLK), S25 (KKIT) — all six organisms complete
**Status**: Spec

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
| `src/ui/organism_panel.rs` | Modify — rotary knobs (optional), per-organism response display |
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
