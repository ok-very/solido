# S34 — Audio Polish & Gain Recalibration

**Status**: Complete (Mar 2026)
**Depends on**: S30 (interaction tuning — pitch fix), S31 (transport — pause for tuning)
**Blocks**: Nothing (can run in parallel with S32/S33)

---

## Goal

Fix immediate audio issues: HOSO frequencies still too low after S30 pitch fix, verify reverb is audible, confirm tape delay UI exists, and recalibrate gain staging for the current 2-organism configuration. This is the "make it sound right now" session.

---

## Issue 1: HOSO Malabar Frequencies Too Low

### Background

S30 added `WireMode::Replace` for pitch wires, fixing the octave-shift bug (freq was `base + slew` instead of just `slew`). HOSO dropped from ~261 Hz to ~130 Hz. Filter cutoff was raised from 800 → 1600 Hz to compensate.

### Problem

User reports frequencies still sound too low. The DNA pitches are:
```
130.8, 146.8, 164.8, 196.0, 220.0 Hz (C3, D3, E3, G3, A3)
```

This is a **bass register** — appropriate for a bass synth but potentially too low for HOSO's intended "clinical sequenced synth" character (Hosono's Cochin Moon). The original DNA was written assuming additive pitch (base 130.8 + slew), which doubled the effective pitch to the C4 range.

### Fix options

**Option A — Raise DNA pitches one octave**:
```json
"pitches": "261.6,293.7,329.6,261.6,293.7,329.6,392.0,261.6,293.7,329.6,261.6,392.0,293.7,329.6,261.6,440.0"
```
Puts HOSO back in C4-A4 range where it was designed to live. Simple, correct.

**Option B — Add octave field to seq_cell**:
```json
"params": { "bpm": 130, "gate_length": 0.4, "swing": 0.0, "octave_offset": 1 }
```
More flexible — organisms can shift octave without rewriting pitch arrays. But adds complexity.

**Recommendation**: Option A. The pitches in DNA should be the actual intended Hz values. Replace mode now plays them literally.

### Filter / accent retune

After raising pitches to C4 range:

| Parameter | Current | New | Reason |
|-----------|---------|-----|--------|
| filter cutoff | 1600 | 2400 | Higher pitch content needs higher cutoff |
| filter res | 0.6 | 0.5 | Slightly less resonance — Moog at 0.6 can be whistly |
| accent mod gain | 3000 | 4000 | Proportional: sweep reaches 2400 + 4000 = 6400 Hz on accents |
| osc gain | 1.0 | 0.85 | C4 range has more perceived loudness — slight trim |
| LFO pw depth | 0.15 | 0.12 | Subtle PWM — less extreme at higher pitch |

---

## Issue 2: Reverb Audibility

### Investigation

Research confirms reverb IS fully wired:
- DNA sends parsed (DRON=0.4, HOSO=0.2)
- ReverbBus receives organism outputs, sums × send level, processes through FunDSP reverb_stereo
- Wet return is mixed into `bus_out` (no MASTER_GAIN scaling — by design)
- Emotion bridge boosts send by proximity × 0.3

### Why it might not be audible

1. **Only 2 organisms active** — total dry output is ~0.10 peak (-20 dBFS). Reverb of 0.10 × 0.2 send × 0.5 return = 0.01 peak. That's **-40 dBFS** — barely audible.

2. **HOSO reverb send is only 0.2** — very subtle. DRON is 0.4 but drones mask reverb tails.

3. **Return level is 0.5** — conservative.

### Fixes

| Parameter | Current | New | Effect |
|-----------|---------|-----|--------|
| HOSO reverb send | 0.2 | 0.35 | More reverb from melodic content |
| DRON reverb send | 0.4 | 0.4 | Already fine |
| Reverb return_level | 0.5 | 0.7 | Louder wet signal globally |
| MASTER_GAIN | 0.65 | 0.85 | Louder overall — we only have 2 organisms, not 6 |

### Gain staging recalibration for 2-organism mode

The gain budget was designed for 6 simultaneous organisms:
```
worst_case = 6 × 0.70 × 0.65 = 2.73 (headroom for limiter)
```

With 2 organisms:
```
current = 2 × 0.65 × 0.65 = 0.845 (well under 1.0 — too quiet)
```

**Dynamic MASTER_GAIN**: Scale based on active organism count:
```rust
let active_count = organisms.iter().filter(|o| o.active).count();
let master = match active_count {
    0..=2 => 0.85,
    3..=4 => 0.70,
    5..=6 => 0.55,
    _ => 0.45,
};
```

Or simpler: `MASTER_GAIN = 1.3 / sqrt(active_count)` — constant-power scaling.

---

## Issue 3: Tape Delay UI

### Investigation

S26 spec says tape delay UI was added. Checking current state:

The tape delay bus exists (`src/audio/tape_delay_bus.rs`). DNA sends are parsed. But the tape delay bus handles were stored as `_tape_delay_bus_handles` (underscore prefix = unused) in app.rs.

S30 may have connected these, but the UI in `organism_panel.rs` needs verification:
- Does a tape delay section exist in the panel?
- Is the send level slider connected to the Shared handle?
- Is the return level adjustable?

### Required tape delay UI

In organism panel, per-organism:
```
┌─ Tape Delay ──────────────┐
│  Send: [====●====] 0.35   │
│  Time: [====●====] 0.3s   │  (read-only — from DNA)
│  Feedback: [==●====] 0.5  │  (read-only — from DNA)
└───────────────────────────┘
```

Only `send` needs to be a live slider (it's a Shared handle). Time and feedback are baked at construction and would need new Shared handles to be adjustable at runtime.

---

## Issue 4: Active Organism Management

Currently, which organisms are active is determined by `"active": true/false` in DNA files. There's no runtime toggle. Only DRON and HOSO are active.

**Quick fix** (before S31's full transport):
- Add organism enable/disable buttons to the organism panel header
- Toggle sends `DspCommand::Panic` + fades VoiceBus strip gain to 0
- Re-enable spawns a new OrganismDsp and fades in

**Full fix**: S31 transport controls with per-organism checkboxes.

---

## Critical Files

| File | Changes |
|------|---------|
| `assets/dna/hoso-malabar.json` | Raise pitches to C4 range, retune filter/accent/LFO |
| `src/audio/gain_staging.rs` | Dynamic MASTER_GAIN based on active organism count |
| `src/audio/reverb_bus.rs` | Raise default return_level 0.5 → 0.7 |
| `src/ui/panels/organism_panel.rs` | Verify/add tape delay UI section, organism enable toggle |
| `src/app.rs` | Wire tape delay handles to UI (remove underscore prefix if needed) |

## Verification

1. HOSO plays in C4 range (261-440 Hz) — brighter, more present
2. Reverb clearly audible on HOSO notes (reverb tail after gate close)
3. Overall volume comfortable — not too quiet with only 2 organisms
4. Tape delay send slider visible and functional in organism panel
5. Can enable/disable organisms from UI without restart
