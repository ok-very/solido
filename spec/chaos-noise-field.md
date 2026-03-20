# Chaos Noise Field — Design Brief

**Status**: Design
**Replaces**: Unified chaos pipeline (Michaelis-Menten 3-source model)
**Blocks**: Pre-union iteration (chaos behavior is a checklist item)

## Problem

The current chaos system has three separate pressure sources (arousal×sensitivity, navigation accumulator, interaction import) feeding Michaelis-Menten compression. All three are proxies for the same concept ("how stimulated is this organism"). The result is:
- Unnecessarily complex data flow (5 hops, 3 threads, 4 files)
- No organic variation — chaos tracks arousal like a meter needle
- Opaque to the user — can't see or feel what's driving the change
- XY pad is disconnected (manual Kaoscillator, not integrated with chaos)

## Solution

Replace the three-source pipeline with a per-organism noise field modulated by arousal.

### Core Formula

```
chaos(t) = base_chaos + max_chaos × clamp(arousal × noise(t), 0, 1)
```

- `base_chaos` — DNA personality floor (species default, e.g. ACID=0.15)
- `max_chaos` — DNA ceiling above base (e.g. 0.3)
- `arousal` — organism's integrated excitement [0,1] (already computed by emotion system)
- `noise(t)` — per-organism noise generator, output [0,1], runs at control-thread rate (~60Hz)

### Noise Types (per-species DNA field)

```rust
pub enum NoiseType {
    /// Integrated white noise. Slow correlated drift.
    /// Good for: DRON, HOSO (ambient, gradual evolution)
    Brownian,
    /// 1/f noise. Self-similar across timescales.
    /// Good for: ISAO, SPGL (natural, balanced variation)
    Pink,
    /// Band-limited to organism's rhythmic rate.
    /// Chaos modulation loosely syncs to beat grid.
    /// Good for: ACID, TBLK, KKIT (rhythm-primary species)
    Spectral,
}
```

### Noise Generator (control thread, RT-safe)

```rust
pub struct ChaosNoiseGen {
    noise_type: NoiseType,
    state: f32,         // current noise output [0, 1]
    velocity: f32,      // for Brownian integration
    rng: u32,           // xorshift PRNG state
    pink_octaves: [f32; 8], // for pink noise
}
```

Update rate: once per frame (~60Hz). Cheap — one xorshift + a few multiplies.

**Brownian**: `velocity += white_noise * step; velocity *= damping; state += velocity; state = state.clamp(0, 1)`
**Pink**: 8-octave Voss-McCartney algorithm. Each octave updates at half the rate of the previous.
**Spectral**: White noise → one-pole lowpass at `bpm / 60 × grid_division` Hz. Chaos breathes with the beat.

### XY Pad Integration

The XY pad becomes a **viewport onto the chaos field**:
- **X axis**: noise field output (horizontal drift = chaos variation)
- **Y axis**: arousal (vertical position = excitement level)
- **Autonomous mode**: pad crosshair moves on its own, showing the organism's generative state
- **Override mode**: user grabs the pad → values snap to pointer → releases → fades back to autonomous

This requires a mode flag on the xy_pad_cell:
```rust
pub enum XyPadMode {
    Manual,     // User controls both axes (current behavior)
    Autonomous, // Chaos field drives X, arousal drives Y
    Hybrid,     // Autonomous until touched, then manual with fade-back
}
```

### Saturation Visualization

The noise amplitude (arousal × noise range) is directly visible as the **spread** of the XY pad crosshair movement:
- Low arousal: crosshair stays near center, small jitter
- High arousal: crosshair sweeps widely, large excursions
- The pad IS the saturation display — no separate visualization needed

### DNA Fields

```rust
// Replace chaos_sensitivity with noise_type
pub struct OrganismDna {
    pub base_chaos: f32,       // kept — personality floor
    pub max_chaos: f32,        // kept — ceiling above base
    pub noise_type: NoiseType, // NEW — replaces chaos_sensitivity
    // chaos_sensitivity: REMOVED
}
```

### What Gets Removed

- `chaos_sensitivity` DNA field → replaced by `noise_type`
- `nav_chaos_accum` HashMap in app.rs → navigation already feeds arousal
- `chaos_interaction_pressure` on OrganismModule → interaction already feeds arousal via harmonic tension
- `compute_unified_chaos()` function → replaced by noise formula
- Michaelis-Menten saturation → noise naturally bounds output
- Three-source pressure summation → single arousal × noise multiply

### What Gets Added

- `ChaosNoiseGen` per organism (control thread, ~32 bytes each)
- `noise_type` DNA field per species
- XY pad autonomous/hybrid mode
- Chaos field visualization on XY pad

### Implementation Order

1. `ChaosNoiseGen` struct + update() for all three noise types
2. Replace chaos pipeline in app.rs: remove 3-source, add noise × arousal
3. Add `noise_type` to OrganismDna, set per-species defaults
4. Remove `chaos_sensitivity`, `nav_chaos_accum`, `chaos_interaction_pressure`
5. XY pad autonomous mode (separate step — can ship noise field first)
6. Hybrid mode (grab-to-override with fade-back)

### Per-Species Defaults

| Species | base_chaos | max_chaos | noise_type | Rationale |
|---------|-----------|-----------|------------|-----------|
| DRON    | 0.02      | 0.15      | Brownian   | Ambient, slow evolution |
| HOSO    | 0.05      | 0.20      | Brownian   | Layered pads, gentle drift |
| SPGL    | 0.08      | 0.30      | Pink       | Sparkly, natural variation |
| ACID    | 0.15      | 0.40      | Spectral   | 303 chaos, beat-synced |
| TBLK    | 0.10      | 0.35      | Spectral   | Rhythm-forward |
| KKIT    | 0.05      | 0.25      | Spectral   | Percussive, beat-locked |
| ISAO    | 0.08      | 0.20      | Pink       | Melodic, balanced |
| RECH    | 0.06      | 0.25      | Pink       | Mallet, natural |
