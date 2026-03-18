# UI: Tempo/Groove Panel — Quantization Grid & Organism Rhythm Matrix

**Status**: Spec (blocks pre-union-iteration Phase A)
**Depends on**: S31 (transport/GlobalClock), S33 (scale/rhythm bridge), existing seq_cell swing
**Blocks**: pre-union-iteration (ENV-3 rhythm ecology depends on groove controls)
**Priority**: Immediate

---

## Goal

A dedicated panel for rhythmic control — the grid/groove counterpart to the key/scale system. Users can set quantization strength, swing amount, groove templates, and see per-organism rhythm behavior in a matrix view. This panel is the primary way to shape HOW organisms interact with the temporal substrate: locked, loose, swung, or free.

---

## Current State

**BPM**: Global, slider 20-300 in controls panel. Propagated via `SetGlobalBpm`.

**Swing**: Per-cell DNA param on seq_cell and melodic_cell. Even-step delay, range [0,1]. Not adjustable at runtime from UI (only via synth_detail slider if the cell exposes it).

**Rhythm sync**: Per-organism DNA string: `"none"`, `"soft"`, `"hard"`. Determines how organism locks to beat grid. Not runtime-adjustable.

**Beat phase**: Tala module emits `beat_phase` [0,1] and `beat_trigger` (true on downbeat). Organisms receive via bridge.

**Tempo ratio**: Per-organism DNA float (default 1.0). Allows polyrhythmic subdivision (e.g., RECH uses 1.0/1.003/0.997 for phasing).

**No groove templates, no global swing, no quantization grid UI.**

---

## 1. Panel Layout

```
┌─── Groove ──────────────────────────────────────────┐
│                                                     │
│  Grid: [1/16 ▼]   Swing: [━━━●━━━] 62%            │
│                                                     │
│  ┌─ Organism Rhythm Matrix ───────────────────────┐ │
│  │            Sync    Swing  Tempo×  Quant  Chaos  │ │
│  │  DRON      none    0%     1.0×    free   0.08  │ │
│  │  HOSO      soft    15%    1.0×    1/8    0.12  │ │
│  │  ISAO      soft    0%     1.0×    1/16   0.08  │ │
│  │  SPGL      soft    25%    1.0×    1/8    0.15  │ │
│  │  ACID      hard    40%    1.0×    1/16   0.20  │ │
│  │  TBLK      soft    0%     1.0×    free   0.10  │ │
│  │  KKIT      hard    0%     1.0×    1/16   0.05  │ │
│  │  RECH      soft    0%     1.003×  1/8    0.06  │ │
│  └────────────────────────────────────────────────┘ │
│                                                     │
│  ┌─ Groove Templates ────────────────────────────┐  │
│  │  [Straight] [Swing 60] [Swing 67] [Triplet]  │  │
│  │  [Shuffle]  [Laid Back] [Pushed]  [Phasing]   │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
│  ┌─ Beat Visualizer ────────────────────────────┐   │
│  │  ● ○ ○ ○ │ ○ ○ ○ ○ │ ○ ○ ○ ○ │ ○ ○ ○ ○    │   │
│  │  1         2         3         4              │   │
│  └───────────────────────────────────────────────┘  │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## 2. Global Groove Controls

### 2a. Grid Division

Dropdown: `1/4`, `1/8`, `1/8T` (triplet), `1/16`, `1/16T`, `1/32`, `Free`

Sets the default quantization grid for organisms with `rhythm_sync != "none"`. Organisms with "hard" sync lock to this grid exactly; "soft" sync nudges toward it.

**Implementation**: New `DspCommand::SetGridDivision(u8)` — maps to a step subdivision multiplier applied in seq_cell's phase accumulation. Division 0 = free (no grid snap).

### 2b. Global Swing

Slider: 0-100% (50% = straight, >50% = delayed even beats, <50% = rushed even beats).

Applied as a global override to all organisms' swing params unless per-organism override is set.

**Implementation**: New `DspCommand::SetGlobalSwing(f32)` dispatched to all organisms. seq_cell uses `max(global_swing, local_swing)` or a blend.

### 2c. Beat Visualizer

16-segment beat indicator showing current beat position. Active beat is filled circle, others are empty. Groups of 4 separated by bar lines. Driven by `beat_phase` from tala module.

---

## 3. Organism Rhythm Matrix

An interactive table where each row is an organism and columns are rhythm properties. This is the main control surface for per-organism groove behavior.

### 3a. Columns

| Column | Type | Range | Source | Editable? |
|--------|------|-------|--------|-----------|
| **Name** | Label | — | OrganismUiState.name | No |
| **Sync** | Dropdown | none/soft/hard | DNA rhythm_sync | Yes (sends DspCommand) |
| **Swing** | Slider | 0-100% | seq_cell swing param | Yes (via Shared) |
| **Tempo x** | Slider | 0.5-2.0 | DNA tempo_ratio | Yes (sends DspCommand) |
| **Quant** | Dropdown | free/1/4/.../1/32 | Per-organism grid override | Yes (sends DspCommand) |
| **Chaos** | Slider | 0.0-1.0 | DNA base_chaos + current | Read (from DspAnalysis) |

### 3b. Sync Mode Runtime Adjustment

Currently `rhythm_sync` is a DNA string only. Need a new `DspCommand::SetRhythmSync(u8)` (0=none, 1=soft, 2=hard) that the bridge receiver can handle at runtime. This allows the user to switch an organism from free-running to locked without restarting.

### 3c. Tempo Ratio Runtime Adjustment

Currently per-cell DNA only. Need `DspCommand::SetTempoRatio(f32)` so the groove panel can adjust per-organism tempo multiplier live. This enables polyrhythmic experiments: set one organism to 1.5x for 3:2 polyrhythm, or 1.003x for gradual phasing.

---

## 4. Groove Templates

Preset buttons that set global swing + per-organism sync patterns:

| Template | Swing | Sync Pattern | Notes |
|----------|-------|-------------|-------|
| **Straight** | 50% | All soft | Clean, no swing |
| **Swing 60** | 60% | All soft | Light swing (jazz) |
| **Swing 67** | 67% | All soft | Hard swing (triplet feel) |
| **Triplet** | 50% | All soft, grid=1/8T | Triplet subdivision |
| **Shuffle** | 67% | Percussion=hard, rest=soft | Boom-chick feel |
| **Laid Back** | 55% | All soft, tempo_ratio 0.98 | Behind the beat |
| **Pushed** | 45% | All hard | Ahead of the beat |
| **Phasing** | 50% | All soft, incremental tempo_ratio offsets | Reich-style gradual phase shift |

Templates modify global swing + per-organism overrides. The user can then tweak individual organisms in the matrix.

---

## 5. New DspCommands

| Command | Payload | Handler |
|---------|---------|---------|
| `SetGridDivision(u8)` | 0=free, 4=1/4, 8=1/8, 12=1/8T, 16=1/16, 24=1/16T, 32=1/32 | seq_cell, melodic_cell |
| `SetGlobalSwing(f32)` | [0.0, 1.0] | seq_cell, melodic_cell |
| `SetRhythmSync(u8)` | 0=none, 1=soft, 2=hard | bridge receiver in OrganismModule |
| `SetTempoRatio(f32)` | [0.5, 2.0] | seq_cell, melodic_cell, logic_seq_cell |

All are `Copy`-safe, small payload. Dispatched via existing `cmd_tx` SPSC channel.

---

## 6. Implementation

### Phase 1 — Commands + Backend

1. Add 4 new `DspCommand` variants to `command.rs`
2. Handle `SetGridDivision` in seq_cell + melodic_cell (modify phase quantization)
3. Handle `SetGlobalSwing` in seq_cell + melodic_cell (override/blend with local swing)
4. Handle `SetRhythmSync` in bridge receiver (OrganismModule)
5. Handle `SetTempoRatio` in seq_cell, melodic_cell, logic_seq_cell

### Phase 2 — Groove Panel UI

6. New panel: `src/ui/panels/groove_panel.rs`
7. Grid division dropdown + global swing slider
8. Beat visualizer (16-segment, driven by beat_phase)
9. Organism rhythm matrix (read/write per-organism values)
10. Groove template buttons

### Phase 3 — Integration

11. Register panel in `PanelVisibility` + header tabs (Metronome icon)
12. Keyboard shortcut: `G` or `F4` to toggle groove panel
13. Wire matrix edits to DspCommand dispatch via app.rs
14. Add per-organism groove state to `OrganismUiState` for matrix display

---

## Critical Files

| File | Changes |
|------|---------|
| `src/dsp/command.rs` | 4 new DspCommand variants |
| `src/dsp/cell/seq_cell.rs` | Handle SetGridDivision, SetGlobalSwing, SetTempoRatio |
| `src/dsp/cell/melodic_cell.rs` | Same handlers as seq_cell |
| `src/dsp/cell/logic_seq_cell.rs` | Handle SetTempoRatio |
| `src/organism/module/bridge.rs` | Handle SetRhythmSync (runtime sync mode change) |
| `src/ui/panels/groove_panel.rs` | **NEW** — full panel: grid, swing, matrix, templates, beat viz |
| `src/ui/panels/mod.rs` | Register groove_panel |
| `src/ui/mod.rs` | Add groove to PanelVisibility |
| `src/ui/tabs.rs` | Add groove tab with Metronome icon |
| `src/ui/panels/organism_panel.rs` | Add groove fields to OrganismUiState |
| `src/app.rs` | Groove panel show + DspCommand dispatch |
