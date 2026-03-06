# S31 — Transport & Global Clock

**Status**: Complete (Mar 2026)
**Depends on**: S29 (mood-driven interactions)
**Blocks**: S32 (continuous attachment — needs pause for tuning), S33 (scale bridge — needs clock sync)

---

## Goal

Establish a single authoritative clock, reconcile the three independent BPM systems, add transport controls (play/pause/restart), and revive the controls panel as the central simulation cockpit.

---

## Problem Statement

### Three independent BPMs — no coordination

| Owner | BPM Source | Default | UI Control | Notes |
|-------|-----------|---------|------------|-------|
| **TalaModule** | `TalaGrid.tempo_bpm` | 120 | Controls panel slider [20, 300] | Drives `beat_trigger`, `beat_phase`, `beat_weight` |
| **SequencerModule** | `Shared` handle | 120 | None | Drives `step_pitch`, `step_gate` timing. Disconnected from everything. |
| **Per-organism seq_cell** | DNA `params.bpm` | Varies (HOSO=130, ACID=130, etc.) | Organism panel slider | Each organism has its own tempo |

All three calculate beats independently from `dt`. They drift if values differ. There is no global clock pulse or phase reference.

### No transport controls

- No play/pause button — simulation runs immediately and forever
- No restart — must kill and relaunch
- No way to mute all organisms simultaneously
- Keyboard shortcut `P` resets gravity but doesn't pause

### Controls panel is stale

- Has gravity/raga/tala selectors — these work
- Tempo slider controls TalaModule only — organisms ignore it
- No transport section (play/pause/stop)
- No global mute/solo
- No organism enable/disable toggles

---

## Architecture: Global Clock

### The GlobalClock struct

```rust
pub struct GlobalClock {
    pub bpm: Shared,              // Authoritative tempo [20, 300]
    pub playing: Shared,          // 1.0 = playing, 0.0 = paused
    pub phase: f64,               // [0.0, 1.0) — beat phase accumulator
    pub beat_count: u64,          // Monotonic beat counter
    pub sample_rate: f32,
}
```

**Ownership**: Created in `SeedReactor`, shared via `Arc<GlobalClock>` to all consumers.

**Tick**: Advances `phase` by `bpm/60.0 * dt` when `playing > 0.5`. Wraps at 1.0, increments `beat_count`.

### BPM reconciliation

**The global clock BPM is the master**. Per-organism BPMs become **ratios** relative to global:

```rust
// In DNA:
"params": { "tempo_ratio": 1.0, "swing": 0.0 }  // 1.0 = match global, 0.5 = half-time, 2.0 = double-time

// In seq_cell tick:
let effective_bpm = global_bpm * self.tempo_ratio;
```

**Rename**: DNA `bpm` → `tempo_ratio`. Values: 0.25 (quarter-time), 0.5 (half-time), 1.0 (lock), 2.0 (double-time), 3.0/2.0 (3:2 polyrhythm), etc.

**Current DNA mapping**:

| Organism | Current BPM | Global=130 | New ratio |
|----------|------------|------------|-----------|
| HOSO | 130 | 130 | 1.0 |
| ACID | 130 | 130 | 1.0 |
| TBLK logic_seq | 4Hz / 3Hz | — | Rate-based, not BPM |
| KKIT | 130 | 130 | 1.0 |
| DRON | No seq_cell | — | N/A |
| SPGL | No seq_cell | — | N/A |

### Clock distribution

```
GlobalClock.bpm (Shared)
    ├──→ TalaModule.tempo_bpm (reads directly)
    ├──→ SequencerModule.bpm (reads directly)
    └──→ Per-organism seq_cell (reads × tempo_ratio)
         └── via DspCommand::SetTempo(f32) sent on clock change
```

**SequencerModule**: Remove its own `bpm` Shared. Read from `GlobalClock.bpm` directly.

**TalaModule**: Remove its own `tempo_bpm`. Read from `GlobalClock.bpm`. `SetTempo` event now writes to `GlobalClock.bpm` instead.

**seq_cell**: New `DspCommand::SetTempo(f32)` sent from control thread when global BPM changes. seq_cell stores effective BPM internally. Keeps its `tempo_ratio` from DNA.

### Phase sync (optional, can defer)

For organisms to sync on downbeats: seq_cell could receive `DspCommand::SyncPhase(f64)` from the global clock at beat boundaries. This enables tight rhythmic locking. **Defer to S33** — not needed for basic transport.

---

## Transport Controls

### UI Layout (controls panel)

```
┌─ Transport ─────────────────────┐
│  [▶ Play] [⏸ Pause] [⏹ Stop]   │
│  BPM: [====●=====] 130          │
│  ☐ Metronome click              │
├─ Organisms ─────────────────────┤
│  ☑ DRON    [M] [S]  ratio: 1.0  │
│  ☑ HOSO    [M] [S]  ratio: 1.0  │
│  ☐ SPGL    [M] [S]  ratio: 1.0  │
│  ☐ ACID    [M] [S]  ratio: 1.0  │
│  ☐ TBLK    [M] [S]  ratio: 1.0  │
│  ☐ KKIT    [M] [S]  ratio: 1.0  │
├─ Gravity ───────────────────────┤
│  (existing gravity controls)    │
├─ Raga ──────────────────────────┤
│  (existing raga selector)       │
├─ Tala ──────────────────────────┤
│  (existing tala selector)       │
│  (remove tempo slider — now     │
│   in Transport section above)   │
└─────────────────────────────────┘
```

### Transport behavior

| Action | Effect |
|--------|--------|
| **Play** | `GlobalClock.playing = 1.0`. All seq_cells advance. Physics tick. |
| **Pause** | `GlobalClock.playing = 0.0`. seq_cells freeze (gate held). Physics freeze. Audio continues (envelopes decay naturally). |
| **Stop** | Pause + reset phase to 0. Send `DspCommand::Panic` to all organisms (release all notes). |
| **BPM slider** | Updates `GlobalClock.bpm`. Propagates to all consumers. |

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| Space | Toggle play/pause |
| Escape | Stop (pause + reset) |
| `+` / `-` | BPM ±5 |
| `[` / `]` | BPM ±1 (fine) |

### Organism enable/disable

Checkboxes in transport section control `OrganismDna.active`. When toggled:
- Active → inactive: Send `DspCommand::Panic`, fade gain to 0 over 100ms
- Inactive → active: Spawn DSP, fade in over 200ms

M (mute) and S (solo) buttons map to existing VoiceBus strip controls.

---

## Controls Panel Revival

### Current state (deprecated sections)

The controls panel works but feels disconnected because:
1. Tempo slider only affects TalaModule — organisms ignore it
2. No transport (play/pause) — feels like a settings menu, not a cockpit
3. No organism management — must edit DNA `active` field and restart

### New structure

Move transport to the top. Keep gravity/raga/tala below. Add organism toggles. The transport section is the primary interaction point.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/reactor/mod.rs` | Add `GlobalClock` struct, share to modules |
| `src/modules/tala_module.rs` | Read BPM from GlobalClock instead of own field |
| `src/modules/sequencer.rs` | Read BPM from GlobalClock |
| `src/dsp/cell/seq_cell.rs` | Accept `DspCommand::SetTempo`, use tempo_ratio × global |
| `src/dsp/command.rs` | Add `DspCommand::SetTempo(f32)` variant |
| `src/organism/dna.rs` | Rename `bpm` → `tempo_ratio` in seq_cell params |
| `src/ui/panels/controls.rs` | Add transport section, organism toggles |
| `src/app.rs` | Wire GlobalClock, keyboard shortcuts |
| `assets/dna/*.json` | Update seq_cell params: `bpm` → `tempo_ratio` |

## Verification

1. `cargo test` — all pass
2. Space bar pauses/resumes all organisms simultaneously
3. BPM slider changes all organisms' tempo proportionally
4. HOSO at ratio=1.0 and KKIT at ratio=1.0 stay phase-locked
5. TBLK at ratio=0.5 plays at half the global tempo
