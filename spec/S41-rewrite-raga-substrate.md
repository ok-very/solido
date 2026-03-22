# S41 Rewrite — Raga as Substrate Filter

**Status**: Spec (rewrite of S41 Raga Activation)
**Depends on**: substrate-encoding.md, S33-rewrite
**Blocks**: organism-satisfaction rewrite

---

## What Changes

**Before (S41)**: RagaModule outputs microtonal cents/weights. App transposes per organism root_pitch_class and direction preference, dispatches SetMicroTuning to audio thread. CombinedTuning merges 12-TET + microtones. Organisms quantize to raga degrees.

**After**: Raga is a filter on the substrate's hue→pitch encoding. When a raga is active, the substrate attenuates non-raga pitch classes. Organisms eat what's available — the raga shapes what's available. Microtonal cents offsets shift the hue bin boundaries. The visual substrate changes color when you select a raga.

---

## Raga as Substrate Hue Filter

### Filter Application

When raga is active, during the per-cell derived field computation in `SubstrateGrid`:

```rust
// Standard pitch class from hue
let pc = ((hue / 30.0).floor() as i8 + key_offset).rem_euclid(12) as u8;

// Apply raga degree filter
let raga_weight = if raga_active {
    raga_degree_weights[pc]  // 0.0 for non-raga PCs, 1.0+ for raga PCs
} else {
    1.0  // No filtering
};

cell.pitch_energy = saturation * scale_weights[pc] * raga_weight;
```

Non-raga pitch classes get their energy attenuated. The substrate visually shifts — colors corresponding to non-raga degrees darken or desaturate.

### Microtonal Hue Bin Boundaries

Standard mapping: 30° per bin, centered at 0°, 30°, 60°, etc.

With raga microtonal offsets, bin boundaries shift:
```
// Standard: bin for C = [345°, 15°) (±15° around 0°)
// Bhairav komal Re at 90 cents (instead of 100):
//   bin for Re shifts from [15°, 45°) to [12°, 42°)
//   This narrows/widens adjacent bins proportionally
```

Effect: microtonal raga tunings are encoded into the substrate geometry. Organisms eating near the adjusted bin boundaries produce microtonal intervals naturally — not because a quantizer forces them, but because the substrate provides that pitch energy at that hue.

### Visual Effect

When raga activates:
- Non-raga hue ranges visually darken (energy attenuated)
- Raga-active hue ranges stay bright
- Vadi (most important degree) gets a boost → that hue range becomes the brightest
- Samvadi gets a smaller boost
- The Circle of Fifths microtone ring already shows this — now the substrate matches

---

## Gamaka from Substrate Gradient

### Old: Explicit Gamaka Config
```
RagaModule → gamaka_config_port → [slide_ms, vib_depth_cents, vib_rate_hz]
  → SetSlewRate, SetLfoParams commands
```

### New: Gradient-Driven Ornaments

When an organism sits between two raga degree hue regions, the substrate gradient creates a natural pull between pitches. The organism's pitch oscillates between the two — this IS gamaka.

```
// At organism position, sample substrate pitch energy in a small neighborhood
// If two adjacent raga degrees are both energetic:
//   pitch oscillates between them → natural ornament
// If one dominates:
//   pitch locks to that degree → stable tone
```

The gamaka emerges from substrate topology rather than explicit configuration. The slew_cell and LFO still exist in the DSP chain — they respond to the modulation that substrate gradient creates.

However, explicit gamaka config stays as a DNA-level artistic control:
- `slew_curve` (linear/expo) determines the character of the ornament
- `gamaka_config` can still be set by the bridge for fine control
- The substrate provides the opportunity, DNA determines the style

---

## Aroha/Avaroha from Movement Direction

### Old: DirectionTracker + Soft Preference
```
Track organism melodic direction (ascending/descending)
  → apply_direction_preference() weights aroha or avaroha degrees
  → SetMicroTuning with direction-adjusted weights
```

### New: Movement Through Substrate Encodes Direction

As an organism physically moves across the substrate:
- Moving from low-hue to high-hue region = ascending (aroha-like)
- Moving from high-hue to low-hue = descending (avaroha-like)
- The consumed pitch histogram naturally reflects the direction of travel

No explicit direction tracking needed — the substrate topology and organism movement trajectory encode aroha/avaroha natively. The pitch histogram captures the sequence of consumed pitches, which IS the melodic contour.

---

## SetMicroTuning: Still Dispatched?

Yes, but simplified. Instead of per-organism transposition + direction preference:

```rust
// When raga is active, dispatch the raga's micro cents globally
// (substrate handles per-organism variation via local consumption)
if raga_active {
    let (cents, weights, count) = raga_module.micro_tuning();
    reactor.broadcast_organism_command(
        DspCommand::SetMicroTuning { cents, weights, count, blend: 1.0 }
    );
}
```

The `blend` is now always 1.0 when raga is active (the substrate already filtered non-raga degrees). Per-organism `scale_affinity` still controls how strictly the audio-thread quantizer snaps — but the available pitches are already substrate-shaped.

---

## What Gets Removed

- Per-organism micro_tuning transposition by root_pitch_class (substrate handles this)
- `apply_direction_preference()` (movement through substrate encodes direction)
- Complex per-organism SetMicroTuning dispatch with org_cents/org_weights (simplified to global broadcast)
- DirectionTracker (melodic direction emerges from substrate trajectory)

## What Stays

- RagaModule with 5 ragas, gamaka config, .scl tuning files
- SetMicroTuning DspCommand and CombinedTuning on audio thread
- Raga chips in controls panel (now they filter the substrate)
- Microtone ring on Circle of Fifths (now shows substrate hue filter)
- slew_cell, LFO for ornament rendering in DSP chain

---

## Critical Files

| File | Change |
|------|--------|
| `src/substrate/energy_grid.rs` | Add raga hue filter to derived field computation |
| `src/app.rs` | Simplify raga dispatch — global SetMicroTuning, remove per-organism transposition |
| `src/modules/raga_module.rs` | Keep as-is, expose raga_degree_weights for substrate filter |
| `src/tuning/raga.rs` | Keep raga_to_micro_tuning(), add degree_weights_for_filter() |
| `src/organism/module/bridge.rs` | Remove direction tracking, simplify bridge |

---

## Verification

1. Select Bhairav → substrate visually attenuates non-Bhairav hue ranges → organisms produce Bhairav intervals
2. Organism at boundary between two raga degrees → pitch oscillates (emergent gamaka)
3. Organism moving from red→green region → ascending pitch contour (emergent aroha)
4. Deselect raga → all hue ranges restore to equal energy → chromatic freedom
5. Vadi degree's hue range is brightest → organisms naturally gravitate and produce that pitch most
