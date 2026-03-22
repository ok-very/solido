# Substrate Encoding — Pixel Color as Pitch/Rhythm Energy

**Status**: Spec
**Depends on**: video-substrate Phase 1 (done), energy_grid.rs (done)
**Blocks**: S33-rewrite, S40-rewrite, S41-rewrite, well-lens, nav-reward-rewrite

---

## Goal

Pixel color in the substrate grid encodes available pitch and rhythm energy. Key change = re-encode the substrate color mapping, not jolt the affinity graph. Organisms eat what's there — they don't seek preferred inputs. The Circle of Fifths and raga controls become substrate transforms.

---

## Color → Pitch Mapping

### HSV Hue → Pitch Class (12 bins, 30° each)

| Hue Range | Pitch Class | Note | Color |
|-----------|-------------|------|-------|
| 0°–30° | 0 | C | Red |
| 30°–60° | 1 | C# | Orange |
| 60°–90° | 2 | D | Yellow |
| 90°–120° | 3 | Eb | Yellow-green |
| 120°–150° | 4 | E | Green |
| 150°–180° | 5 | F | Cyan-green |
| 180°–210° | 6 | F# | Cyan |
| 210°–240° | 7 | G | Blue |
| 240°–270° | 8 | Ab | Blue-violet |
| 270°–300° | 9 | A | Violet |
| 300°–330° | 10 | Bb | Magenta |
| 330°–360° | 11 | B | Red-magenta |

Saturation → pitch confidence (low saturation = ambiguous pitch class, weak gravity).
Value/brightness → energy intensity (dark = depleted, bright = abundant).

### Key Change = Hue Rotation

When user changes key via Circle of Fifths:
```
new_hue = (old_hue + key_delta × 30) mod 360
```

This rotates the entire substrate's pitch mapping. A red pixel (C in key of C) becomes a D pixel (in key of D). The visual substrate shifts color — organisms see different pitch energy at their position without any internal parameter change.

**No affinity graph jolt needed.** The substrate simply provides different energy. Organisms metabolize what's there.

### Raga = Hue Filter

When a raga is active, the substrate's hue-to-pitch mapping passes through the raga's degree filter:
- Only raga-active pitch classes carry full energy
- Non-raga pitch classes are attenuated (energy × 0.2)
- Microtonal cents offsets shift the hue bin boundaries (komal Re = narrower bin centered at 90 cents instead of 100)

Effect: the visual substrate appears to lose color in non-raga degrees. A Bhairav raga filter would suppress E, F#, Bb leaving only Sa Re Ga Ma Pa Dha Ni active.

### Scale = Same Mechanism

Western scales work identically — gravity_weights become substrate hue attenuation:
```
For each pixel:
  hue → pitch_class
  energy × scale_weights[pitch_class] → effective energy
```

Major scale: full energy on C D E F G A B, zero on sharps/flats.
Chromatic: all bins at equal energy.

---

## Brightness → Rhythm Energy

Pixel brightness (V in HSV) encodes rhythm energy:
- Bright pixel = high rhythmic energy = faster seq_cell triggering
- Dark pixel = low energy = slower, sparser triggers
- Flashing/flickering video content → rhythmic substrate that organisms sync to

This replaces the tala as the primary rhythm source. Tala becomes an optional overlay that modulates rhythm independently of substrate.

---

## Substrate Grid Integration

### EnergyCell Extension

```rust
pub struct EnergyCell {
    pub rgb: [f32; 3],        // Raw video color energy
    pub energy: f32,          // Mean brightness (cached)
    // NEW: Derived pitch/rhythm encoding
    pub pitch_class: u8,      // Dominant hue → pitch class [0, 11]
    pub pitch_energy: f32,    // Saturation × scale_weight for this PC
    pub rhythm_energy: f32,   // Brightness (V channel)
}
```

### Per-Frame Substrate Update

1. Video frame → replenish RGB in grid cells (existing)
2. Organism depletion → drain RGB (existing)
3. **NEW**: After replenish+deplete, compute derived fields:
   ```
   For each cell:
     (h, s, v) = rgb_to_hsv(cell.rgb)
     cell.pitch_class = (h / 30.0).floor() as u8  // + key_offset
     cell.pitch_energy = s × scale_weights[cell.pitch_class]
     cell.rhythm_energy = v
   ```

### Key Change Implementation

Instead of transposing gravity wells:
```rust
// Old: gravity_field.transpose_to_key(delta)
// New: shift the hue→pitch mapping offset
self.substrate_key_offset = (self.substrate_key_offset + delta).rem_euclid(12);
```

The offset is applied during the derived field computation:
```
pitch_class = ((h / 30.0).floor() as i8 + key_offset).rem_euclid(12) as u8
```

No organism state changes. No affinity jolt. The world just recolors.

---

## Organism Consumption Model

### Nutrient Channels = Substrate RGB

The existing 3-channel nutrient system maps directly to substrate RGB:
- Channel 0 = Red substrate energy
- Channel 1 = Green substrate energy
- Channel 2 = Blue substrate energy

Species nutrient profiles become **appetite preferences**:
- DRON [0.7, 0.1, 0.2] → prefers red-rich substrate (low pitches)
- ACID [0.1, 0.7, 0.2] → prefers green-rich substrate (mid pitches)
- KKIT [0.0, 0.8, 0.2] → prefers green/blue (mid-high pitches)

### Feeding: Substrate → Nutrient

```
For organism at (x, y) with species profile P:
  cell = grid.sample(x, y)

  // Weighted consumption: eat more of preferred channels
  for ch in 0..3:
    consumed[ch] = appetite × P[ch] × cell.rgb[ch]
    cell.rgb[ch] -= consumed[ch]
    nutrient_levels[ch] += consumed[ch] × REPLENISH_RATE
```

Organisms consume what they're built to eat. Red-preferring DRON depletes red from the substrate, leaving blue/green for others. **Niche differentiation emerges from appetite × substrate color.**

### Pitch Output = Consumed Pitch Class

The pitch class of what an organism consumed determines what it produces:
```
consumed_hue = weighted_mean_hue(consumed RGB)
consumed_pc = hue_to_pitch_class(consumed_hue + key_offset)
→ SetScaleWeights: boost consumed_pc, attenuate others
```

Organisms don't choose pitch — they produce the pitch of what they ate. A red-grazing DRON in key of C produces C. Move to key of D, the same red pixel now encodes D. The organism follows.

---

## Transport Controls (Circle of Fifths + Raga Chips)

### Circle of Fifths
- Click key → sets `substrate_key_offset` (hue rotation)
- Visual: constellation still shows active scale degrees, but now it represents substrate encoding
- Microtone ring shows raga degree positions in the hue space

### Scale Chips
- Click scale → sets `scale_weights[12]` used to attenuate substrate hue bins
- Major: strong weights on 7 naturals, zero on 5 accidentals
- Chromatic: all 1.0 (no filtering)

### Raga Chips
- Click raga → sets microtonal hue bin boundaries + degree weights
- Effect: substrate pitch encoding shifts to raga's tuning system
- Organism pitch output follows the raga's intervallic structure

---

## Critical Files

| File | Change |
|------|--------|
| `src/substrate/energy_grid.rs` | Add pitch_class, pitch_energy, rhythm_energy to EnergyCell. HSV conversion. Key offset. |
| `src/app.rs` | Replace gravity well dispatch with substrate key offset. Remove affinity jolt on key change. |
| `src/ui/panels/controls.rs` | Circle of Fifths sets substrate_key_offset, not gravity_field.transpose_to_key() |
| `src/organism/registry.rs` | Nutrient feeding from substrate grid instead of org-to-org node wells |
| `src/dsp/organism_dsp.rs` | SetScaleWeights derived from consumed substrate, not from gravity wells |

---

## Verification

1. Change key in Circle of Fifths → substrate visually hue-shifts, organisms produce different pitches without internal state change
2. Select raga → substrate attenuates non-raga hue bins, organisms produce raga-appropriate intervals
3. Bright video → rhythmically active organisms. Dark video → sparse, slow
4. DRON grazes red, ACID grazes green → niche differentiation visible as colored depletion halos
5. No affinity graph jolt on key change — smooth transition
