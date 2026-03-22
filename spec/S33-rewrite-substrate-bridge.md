# S33 Rewrite — Substrate Bridge (Scale/Rhythm)

**Status**: Spec (rewrite of S33 Scale/Rhythm Bridge)
**Depends on**: substrate-encoding.md, block-grid-vision.md
**Blocks**: organism-satisfaction rewrite

---

## What Changes

**Before (S33)**: OrganismModule receives gravity_weights and beat_phase signals from ScaleModule/TalaModule via the affinity graph. Bridge dispatches SetScaleWeights and NudgePhase to the audio thread. Organisms seek scale conformity — `scale_affinity × fidelity` controls how hard they snap.

**After**: Organisms receive pitch/rhythm energy from the substrate grid at their position. No seeking. The bridge translates consumed substrate energy into DSP commands. Scale affinity becomes metabolism efficiency — how well the organism converts substrate pitch energy into musical output.

---

## Pitch Bridge

### Old Flow
```
ScaleModule → gravity_weights [f32; 12] → AffinityGraph → OrganismModule
  → effective_weights(pos, org_root, base) → SetScaleWeights(weights, blend)
  → audio thread quantizes pitch
```

### New Flow
```
SubstrateGrid → local_sight(org_pos) → dominant_pc, pitch_energy, pitch_diversity
  → OrganismModule receives consumed pitch class from feeding
  → SetScaleWeights derived from CONSUMED pitch distribution
  → scale_affinity = metabolism efficiency (how cleanly organism quantizes)
```

### Consumed Pitch Distribution

Each frame, the organism feeds from the substrate. The consumed RGB maps to pitch via hue:

```rust
// In app.rs substrate feeding loop:
let consumed_rgb = substrate_grid.deplete(org.position, radius, appetite);
let (consumed_h, consumed_s, _) = rgb_to_hsv(consumed_rgb);
let consumed_pc = ((consumed_h / 30.0) as i8 + key_offset).rem_euclid(12) as u8;

// Build a 12-element consumption histogram (rolling EWMA)
org.pitch_histogram[consumed_pc] += consumed_s;  // saturation weights pitch confidence
for i in 0..12 { org.pitch_histogram[i] *= 0.98; }  // decay older consumption

// Derive scale weights from consumption history
let weights = org.pitch_histogram;  // what you ate IS your scale
```

### SetScaleWeights Dispatch

```rust
// blend = scale_affinity (DNA) — metabolism efficiency
// High affinity: organism quantizes cleanly to consumed pitches
// Low affinity: organism plays loosely, drifts from consumed pitch
org_mod.send_command(DspCommand::SetScaleWeights(weights, scale_affinity));
```

The quantizer on the audio thread stays exactly the same — `quantize_to_tuning()` is unchanged. Only the source of the weights changes: from gravity wells to consumption histogram.

---

## Rhythm Bridge

### Old Flow
```
TalaModule → beat_phase [0,1] → AffinityGraph → OrganismModule
  → soft/hard sync nudge → NudgePhase command
  → seq_cell phase correction
```

### New Flow
```
SubstrateGrid → local rhythm_energy (brightness at organism position)
  → Modulates seq_cell tempo_ratio via video_cv_cell
  → Bright substrate = fast, dark = slow
  → Tala remains as optional overlay (enable/disable in groove panel)
```

### Brightness → Tempo Modulation

The video_cv_cell already routes brightness to DSP targets. For rhythm:

```
brightness → video_cv_cell ch0 → modulation wire → seq_cell.tempo_ratio
```

DNA wiring in organisms that want rhythm-from-substrate:
```json
{
  "src_cell": <video_cv_idx>, "dst_cell": <seq_cell_idx>,
  "wire_type": { "Modulation": { "target_param": "tempo_ratio", "source_channel": 0 } },
  "gain": 0.5, "mode": "Add"
}
```

Effect: bright substrate → tempo_ratio increases → faster sequencing. Dark → slower. Flickering video → rhythmic pulsing.

### Tala as Optional Overlay

TalaModule stays (already has enable/disable). When enabled, it provides additional phase sync on top of substrate rhythm. When disabled, rhythm is purely substrate-driven.

This gives the user two rhythm layers:
1. **Substrate rhythm** (automatic, from video brightness)
2. **Tala rhythm** (manual, from groove panel)

---

## What Gets Removed

- `gravity_field.effective_weights()` — no longer called per organism for pitch
- `gravity_field.transpose_to_key()` — replaced by substrate key_offset
- Affinity graph jolt on key change — no longer needed
- ScaleModule → OrganismModule gravity_weights signal routing — substrate replaces this
- The complex 3-layer effective_weights computation (base + well blend + normalize)

## What Stays

- `CombinedTuning` and `quantize_to_tuning()` on the audio thread — unchanged
- `SetScaleWeights` and `SetMicroTuning` DspCommand variants — unchanged
- `scale_affinity` and `fidelity` DNA fields — reinterpreted as metabolism efficiency
- Raga microtonal overlay — now derived from substrate hue filter (see S41-rewrite)
- seq_cell, tempo_ratio, NudgePhase — all unchanged
- TalaModule — stays as optional overlay

---

## New DNA Fields

```rust
pub pitch_histogram_decay: f32,  // EWMA decay [0.95, 0.995]. Slow = long memory, fast = reactive.
```

Species defaults:
| Species | pitch_histogram_decay | Personality |
|---------|----------------------|-------------|
| DRON | 0.995 | Long memory, slow scale shifts |
| HOSO | 0.98 | Moderate, follows substrate changes |
| ACID | 0.96 | Fast reactor, pitch follows substrate quickly |
| ISAO | 0.98 | Moderate |
| KKIT | 0.99 | Stable, percussion doesn't need fast pitch tracking |

---

## Critical Files

| File | Change |
|------|--------|
| `src/app.rs` | Replace gravity well dispatch with substrate consumption → pitch histogram → SetScaleWeights |
| `src/organism/sim.rs` | Add `pitch_histogram: [f32; 12]` to OrganismState |
| `src/organism/dna.rs` | Add `pitch_histogram_decay` |
| `src/tuning/gravity_well.rs` | Keep for well lens UV warp, remove pitch dispatch role |
| `assets/dna/*.json` | Add pitch_histogram_decay to all organisms |

---

## Verification

1. No key change jolt — smooth transition when changing key (substrate hue rotates, organisms gradually shift)
2. Organism near red substrate produces C-range pitches (in key of C). Move to green → produces E-range.
3. ACID (fast decay) shifts pitch within ~1 second of moving to new substrate color. DRON takes ~5 seconds.
4. scale_affinity=0 organism plays freely regardless of substrate. scale_affinity=1 quantizes strictly.
5. Bright video → fast rhythms. Dark scene → sparse. Flickering → rhythmic pulsing.
