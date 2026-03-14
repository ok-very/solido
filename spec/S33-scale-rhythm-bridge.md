# S33 — Scale & Rhythm Bridge

**Status**: Complete (Mar 2026)
**Depends on**: S31 (global clock — tempo sync), S32 (continuous attachment — musical convergence context)
**Blocks**: organism-union.md Phase 6 (raga inheritance)

---

## Goal

Connect tala (rhythm) and raga (pitch/scale) infrastructure to organisms. Currently, organisms run on their own seq_cell patterns with fixed pitches — they cannot hear the tala grid, cannot quantize to a raga, and cannot play in a scale. Everything is rhythm-and-drum-based. This session makes organisms **musically aware** of the tuning system.

---

## Problem Statement

### Signal type/range mismatches block all connections

Research confirmed that **no automatic edges** form between infrastructure scale/rhythm modules and organisms:

| Source → Target | Issue |
|-----------------|-------|
| TalaModule.beat_phase [0,1] → Organism.pitch_hz [20,20000] | Range mismatch |
| TalaModule.beat_trigger (Trigger/Event) → Organism.gate (Trigger/Block) | Rate mismatch |
| RagaModule.gravity_weights (Pattern) → Organism.pitch_hz (Float) | Type mismatch |
| RagaModule.gamaka_config (Pattern) → Organism | Type mismatch |
| RagaModule.raga_hue [0,360] → Organism | Range mismatch |

The **only working paths** are:
- QuantizerModule.pitch_hz [20,20000] → Organism.pitch_hz [20,20000] (works)
- SequencerModule.step_pitch/gate/accent → Organism (works)

But the quantizer only fires when keyboard input provides `raw_pitch`. Without keyboard input, organisms get no scale information.

### Organisms are musically deaf

Each organism's seq_cell plays hardcoded Hz values from DNA (e.g., HOSO: 130.8, 146.8, 164.8, 196.0, 220.0). These are scale-unaware — they happen to be C3/D3/E3/G3/A3 but:
- Changing the raga does nothing to organism pitch
- Changing the tala does nothing to organism rhythm
- Organisms can't adapt their melodies to the current scale
- Cross-organism pitch coordination is impossible without a shared scale

---

## Architecture: Three Bridge Systems

### Bridge 1: Scale Gravity → Organism Pitch Quantization

**Core idea**: Organisms receive gravity weights from RagaModule and quantize their internal seq_cell pitches to the nearest scale degree.

#### New OrganismModule input port

```rust
// New input port on OrganismModule:
gravity_weights_port: PortId,  // SignalType::Pattern, PortRate::Block, range: N/A (Pattern)
```

This port receives `gravity_weights` from RagaModule. Pattern type matches Pattern type — no range issue.

#### Internal quantization

When `gravity_weights` is received, store it on the module. During `apply_pitch_hz()`, quantize the seq_cell's raw pitch to the nearest weighted scale degree:

```rust
fn quantize_to_scale(&self, raw_hz: f32, gravity: &[f32]) -> f32 {
    // Convert Hz to MIDI-like degree
    let midi = 12.0 * (raw_hz / 440.0).log2() + 69.0;
    let octave = (midi / 12.0).floor();
    let degree = midi - octave * 12.0;

    // Find nearest degree with weight > threshold
    let mut best_degree = degree;
    let mut best_dist = f32::MAX;
    for (i, &weight) in gravity.iter().enumerate() {
        if weight < 0.1 { continue; }  // Skip inactive degrees
        let d = i as f32;
        let dist = ((degree - d).abs()).min(12.0 - (degree - d).abs());
        let weighted_dist = dist / weight;  // Higher gravity = shorter effective distance
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_degree = d;
        }
    }

    // Convert back to Hz
    let quantized_midi = octave * 12.0 + best_degree;
    440.0 * 2.0f32.powf((quantized_midi - 69.0) / 12.0)
}
```

#### Fidelity interaction

The personality blend still applies:

```rust
let quantized = self.quantize_to_scale(raw_hz, &gravity_weights);
let actual = lerp(raw_hz, quantized, self.fidelity * scale_affinity);
```

- DRON (fidelity=0.3): mostly ignores scale, drones on whatever
- HOSO (fidelity=0.9): strongly follows scale
- SPGL (fidelity=0.1): almost ignores scale
- ACID (fidelity=0.8): follows scale but with slides between degrees

#### Scale affinity (new DNA field)

```json
"scale_affinity": 0.8  // [0, 1] — how much this organism follows external scales
```

Separate from `fidelity` (which governs response to external pitch prompts). Scale affinity specifically controls raga quantization strength.

### Bridge 2: Tala Beat Grid → Organism Rhythm Sync

**Core idea**: Organisms receive beat phase/trigger from TalaModule and optionally sync their seq_cell step advance to the tala grid.

#### New OrganismModule input ports

```rust
beat_phase_port: PortId,    // SignalType::Float [0.0, 1.0], PortRate::Block
beat_trigger_port: PortId,  // SignalType::Trigger, PortRate::Event
```

**Range fix**: `beat_phase` [0,1] now has a matching input port on organisms with range [0,1], not the [20,20000] pitch port. New dedicated port.

#### Beat sync modes (DNA field)

```json
"rhythm_sync": "soft"  // "none" | "soft" | "hard"
```

| Mode | Behavior |
|------|----------|
| `none` | Ignore tala entirely. Seq_cell runs on internal clock. |
| `soft` | Phase nudge: seq_cell phase drifts toward tala beat phase over ~2 beats. Creates loose sync — organisms are "in the groove" but not mechanical. |
| `hard` | Phase lock: seq_cell resets phase on tala sam (downbeat). Tight grid. |

#### Soft sync implementation

```rust
// In OrganismModule tick, when beat_phase is received:
if self.rhythm_sync == "soft" {
    let phase_error = tala_phase - seq_phase;
    // Wrap to [-0.5, 0.5]
    let wrapped = if phase_error > 0.5 { phase_error - 1.0 }
                  else if phase_error < -0.5 { phase_error + 1.0 }
                  else { phase_error };
    // Nudge: 10% correction per beat
    let nudge = wrapped * 0.1 * rhythm_affinity;
    // Send phase adjustment to seq_cell via DspCommand::NudgePhase(f32)
}
```

#### Rhythm affinity (new DNA field)

```json
"rhythm_affinity": 0.5  // [0, 1] — how strongly this organism syncs to external rhythm
```

- DRON: 0.1 (drones don't need rhythm)
- HOSO: 0.7 (ensemble player, syncs to groove)
- SPGL: 0.0 (deliberately desynchronized — the beauty of SPGL)
- ACID: 0.8 (acid lines lock to grid)
- TBLK: 0.6 (tabla follows but with own polyrhythmic identity)
- KKIT: 0.9 (drum machine — tightest sync)

### Bridge 3: Gamaka Ornaments → Organism Pitch Expression

**Core idea**: RagaModule's gamaka config (slide time, vibrato depth, vibrato rate) modulates the organism's slew_cell and LFO.

#### Gamaka reception

```rust
// New input port:
gamaka_config_port: PortId,  // SignalType::Pattern, PortRate::Block
```

When received, unpack 3 floats: `[slide_ms, vib_depth_cents, vib_rate_hz]`

Apply to organism DSP:
- `slide_ms` → slew_cell `rise` and `fall` params via DspCommand
- `vib_depth_cents` → LFO `depth` param
- `vib_rate_hz` → LFO `rate` param

This makes HOSO's pitch slides and vibrato respond to raga selection. Bhairav gets slow deep bends, Malabar gets quick ornamental turns.

---

## Routing: How Edges Form

### Infrastructure → Organism (AffinityGraph)

New organism ports with correct types/ranges will auto-discover:

| Source | Target | Type Match | Range Match |
|--------|--------|------------|-------------|
| RagaModule.gravity_weights (Pattern/Block) | Organism.gravity_weights (Pattern/Block) | Pattern = Pattern | N/A for Pattern |
| RagaModule.gamaka_config (Pattern/Block) | Organism.gamaka_config (Pattern/Block) | Pattern = Pattern | N/A for Pattern |
| TalaModule.beat_phase (Float [0,1]/Block) | Organism.beat_phase (Float [0,1]/Block) | Float = Float | [0,1] ⊆ [0,1] |
| TalaModule.beat_trigger (Trigger/Event) | Organism.beat_trigger (Trigger/Event) | Trigger = Trigger | N/A |

All edges auto-create via `discover_organism_edges()`. Hebbian learning strengthens connections where organisms respond productively (valence increases).

### Organism → Organism (cross-organism learning)

With bridge ports, organisms can now learn from each other:
- ACID.seq_pitch → HOSO.pitch_hz (already works)
- HOSO.seq_pitch → TBLK (TBLK can adapt membrane tuning to melodic context)

The spectral_centroid and seq_pitch ports from S30's bridge data enable this.

---

## DNA Schema Additions

```json
{
  "scale_affinity": 0.8,
  "rhythm_affinity": 0.7,
  "rhythm_sync": "soft",

  "affinity_biases": [
    { "port_name": "gravity_weights", "bias": 0.8 },
    { "port_name": "beat_phase", "bias": 0.6 },
    { "port_name": "gamaka_config", "bias": 0.5 }
  ]
}
```

### Per-organism defaults

| Species | scale_affinity | rhythm_affinity | rhythm_sync | Character |
|---------|---------------|-----------------|-------------|-----------|
| DRON | 0.3 | 0.1 | none | Drones in whatever key, ignores rhythm |
| HOSO | 0.8 | 0.7 | soft | Follows scale closely, grooves loosely |
| SPGL | 0.1 | 0.0 | none | Ignores everything — own cosmic rhythm |
| ACID | 0.7 | 0.8 | hard | Follows scale with slides, locks to grid |
| TBLK | 0.2 | 0.6 | soft | Pitch barely affected, rhythm loosely synced |
| KKIT | 0.0 | 0.9 | hard | No pitch (drums), tight grid lock |

---

## New DspCommands

```rust
pub enum DspCommand {
    // ... existing ...
    SetTempo(f32),         // From S31: global BPM × ratio
    NudgePhase(f32),       // Phase correction [-0.5, 0.5]
    SetSlewRate(f32, f32), // (rise_ms, fall_ms) from gamaka
    SetLfoParams(f32, f32), // (rate_hz, depth) from gamaka
}
```

---

## Critical Files

| File | Changes |
|------|---------|
| `src/organism/module.rs` | New input ports (gravity_weights, beat_phase, beat_trigger, gamaka_config), quantize_to_scale(), rhythm sync |
| `src/organism/dna.rs` | `scale_affinity`, `rhythm_affinity`, `rhythm_sync` fields |
| `src/dsp/command.rs` | `NudgePhase`, `SetSlewRate`, `SetLfoParams` commands |
| `src/dsp/cell/seq_cell.rs` | Accept `NudgePhase` — adjust step phase accumulator |
| `src/dsp/organism_dsp.rs` | Route new DspCommands to cells |
| `src/modules/tala_module.rs` | Ensure beat_trigger uses Event rate (may already be correct) |
| `assets/dna/*.json` | Add `scale_affinity`, `rhythm_affinity`, `rhythm_sync` |

## Verification

1. Set raga to Bhairav → HOSO pitch snaps to Bhairav degrees (C Db E F G Ab B)
2. Change raga to Yaman → HOSO pitch shifts to Yaman degrees (C D E F# G A B)
3. DRON barely responds to raga change (scale_affinity=0.3)
4. SPGL completely ignores raga (scale_affinity=0.1)
5. Tap tempo → ACID and KKIT lock within 2 beats
6. HOSO loosely follows tempo — slight drift but mostly in groove
7. Gamaka changes → HOSO slide time and vibrato depth respond
