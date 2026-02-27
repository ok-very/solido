# S16 — Normalized Parameter Space

> Every knob turns from 0 to 1. What it means depends on who's listening.

**Layer**: L0 (Module Contract) + L4 (Cells)
**Depends on**: S01 (module contract), S12 (cell composition + unified DNA)
**Status**: Prospect

## Goal

Introduce a normalized `[0, 1]` representation for all cell parameters so that the learning system, mutation operators, and future ML estimators can treat the parameter space uniformly. Currently, `CellDna.params` stores raw native values — frequency in Hz `[20, 16000]`, time in ms `[0.5, 5000]`, counts as floats `[1, 16]`. This heterogeneity makes evolutionary mutation, crossover, and any future parameter estimation operate on incommensurable scales.

## Ancestry (MAKE A BABY)

SpiegeLib normalizes all VST parameters to `[0, 1]` regardless of native range. This unifies the learning target across different synthesizers and simplifies ML models — an MLP doesn't need to know that parameter 3 is Hz and parameter 7 is milliseconds. The synth handles denormalization. MAKE A BABY's Max/MSP patches had `scale` objects everywhere to map between ranges — the normalization was manual and fragile. Solido can make it structural.

## The Problem

### Heterogeneous parameter ranges break uniform operations

Current `CellDna.params` stores native values:

| Cell | Parameter | Range | Scale |
|------|-----------|-------|-------|
| strike_voice | membrane_freq | [40, 800] | Hz (log-perceptual) |
| timbre_voice | attack_ms | [0.5, 500] | ms (log-perceptual) |
| pattern_gen | steps | [1, 16] | count (integer) |
| harmonic_bed | cutoff | [50, 16000] | Hz (log-perceptual) |
| shimmer_layer | diffusion | [0, 1] | linear (already normalized) |
| arpeggiator | rate_hz | [0.5, 20] | Hz (log-perceptual) |
| timbre_voice | filter_q | [0.1, 4.0] | Q (linear or log) |

A mutation operator that adds Gaussian noise `N(0, 0.05)` to a normalized parameter is musically meaningful across all params. The same noise added to native values would be imperceptible on `membrane_freq` (0.05 Hz) and destructive on `diffusion` (5% of total range).

### No scale metadata in CellRegistry

`CellRegistry.param_ranges` stores `(min, max)` per param, but not whether the mapping should be linear, logarithmic, or integer-quantized. Frequency params need log mapping (equal perceptual spacing), time params need log mapping (100ms→200ms is the same "distance" as 1000ms→2000ms), but gain params are linear.

### Mutation and crossover operate on raw values

DNA crossover (`OrganismDna` doesn't have crossover yet, but it's specified in the overview) would need to handle mixed scales. Two-point crossover on native values mixes Hz with ms with counts — meaningless.

### Future ML estimators need uniform input

If Solido ever adds sound-target matching (given reference audio, find DNA params), the estimator needs a uniform `[0, 1]^N` target space. Training on heterogeneous native ranges would require per-parameter loss weighting.

## Architecture Decisions

### AD-1: ParamScale enum describes the mapping curve

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamScale {
    /// Linear mapping: native = min + normalized * (max - min)
    Linear,
    /// Logarithmic mapping: native = min * (max/min)^normalized
    /// Used for frequency, time, rate — perceptually uniform spacing.
    Log,
    /// Integer-quantized linear: round(min + normalized * (max - min))
    /// Used for step counts, octave counts, pattern indices.
    IntLinear,
}
```

Three scales cover all current params. Log handles frequency and time (where doubling matters more than absolute difference). IntLinear handles counts and enum indices. Linear handles gains, mixes, and ratios already in `[0, 1]`.

### AD-2: ParamDescriptor replaces bare (f32, f32) in CellRegistry

```rust
#[derive(Clone, Debug)]
pub struct ParamDescriptor {
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub scale: ParamScale,
    pub semantic: Option<PortSemantic>,  // Links to S15 if applicable
}

impl ParamDescriptor {
    /// Normalized [0,1] → native value
    pub fn to_native(&self, normalized: f32) -> f32 {
        let n = normalized.clamp(0.0, 1.0);
        match self.scale {
            ParamScale::Linear => self.min + n * (self.max - self.min),
            ParamScale::Log => self.min * (self.max / self.min).powf(n),
            ParamScale::IntLinear => (self.min + n * (self.max - self.min)).round(),
        }
    }

    /// Native value → normalized [0,1]
    pub fn to_normalized(&self, native: f32) -> f32 {
        let v = native.clamp(self.min, self.max);
        match self.scale {
            ParamScale::Linear => (v - self.min) / (self.max - self.min),
            ParamScale::Log => (v / self.min).ln() / (self.max / self.min).ln(),
            ParamScale::IntLinear => (v - self.min) / (self.max - self.min),
        }
    }
}
```

Bidirectional mapping. `to_native` is used at cell construction. `to_normalized` is used for mutation, crossover, serialization, and ML export.

### AD-3: CellDna gains a normalized representation alongside native

```rust
pub struct CellDna {
    pub cell_type: String,
    pub params: BTreeMap<String, f32>,           // Native values (backward compat)
    pub string_params: BTreeMap<String, String>,
}

impl CellDna {
    /// Get all params as normalized [0,1] using the registry.
    pub fn normalized_params(&self, registry: &CellRegistry) -> BTreeMap<String, f32> { ... }

    /// Set params from normalized [0,1] values using the registry.
    pub fn set_from_normalized(
        &mut self,
        normalized: &BTreeMap<String, f32>,
        registry: &CellRegistry,
    ) { ... }
}
```

Native storage stays as-is — `param_or()` continues to work unchanged. Normalization is a view, not the storage format. This preserves backward compatibility with existing DNA serialization.

### AD-4: CellRegistry stores ParamDescriptor instead of (f32, f32)

```rust
pub struct CellRegistry {
    factories: HashMap<String, CellFactory>,
    param_descriptors: HashMap<String, Vec<ParamDescriptor>>,  // was param_ranges
}
```

Each cell type registers its descriptors with scale metadata. The existing `param_ranges()` method returns a compatible view for code that only needs `(min, max)`.

### AD-5: String params get an EnumDescriptor

```rust
#[derive(Clone, Debug)]
pub struct EnumDescriptor {
    pub name: String,
    pub variants: Vec<String>,
    pub default: String,
}

impl EnumDescriptor {
    /// Normalized [0,1] → variant string (uniform quantization)
    pub fn to_variant(&self, normalized: f32) -> &str { ... }

    /// Variant string → normalized [0,1]
    pub fn to_normalized(&self, variant: &str) -> f32 { ... }
}
```

This lets mutation operators handle modal parameters (oscillator type, filter mode) uniformly — `0.0` = first variant, `1.0` = last variant, intermediate values quantize.

### AD-6: Mutation operators work in normalized space

```rust
pub fn mutate_params(
    dna: &mut CellDna,
    registry: &CellRegistry,
    rng: &mut impl Rng,
    sigma: f32,  // mutation strength in [0,1] space
) {
    let descriptors = registry.descriptors_for(&dna.cell_type);
    for desc in descriptors {
        if let Some(native) = dna.params.get(&desc.name) {
            let norm = desc.to_normalized(*native);
            let mutated = (norm + rng.sample::<f32, _>(StandardNormal) * sigma).clamp(0.0, 1.0);
            dna.params.insert(desc.name.clone(), desc.to_native(mutated));
        }
    }
}
```

Gaussian mutation in `[0, 1]` space has uniform musical meaning across all params. A `sigma` of 0.05 means "5% of the perceptual range" whether it's frequency, time, or gain.

## Implementation

### 1. Add ParamScale and ParamDescriptor

`src/dsp/cell/mod.rs`: New types. `ParamDescriptor` with `to_native()` / `to_normalized()`.

### 2. Upgrade CellRegistry to store descriptors

`src/dsp/cell/mod.rs`: Replace `param_ranges: HashMap<String, HashMap<String, (f32, f32)>>` with `param_descriptors: HashMap<String, Vec<ParamDescriptor>>`. Add backward-compat `param_ranges()` method that extracts `(min, max)`.

### 3. Annotate all cell params with scale

Each cell's registration block in `CellRegistry::new()` upgrades from `(min, max)` to full descriptors with scale:

| Parameter category | Scale |
|-------------------|-------|
| Frequency (Hz): membrane_freq, root_hz, cutoff, freq, filter_base, filter_depth | Log |
| Time (ms): attack_ms, decay_ms, release_ms | Log |
| Rate (Hz): bpm, rate_hz, pan_rate, pwm_rate, filter_lfo_rate, vibrato_rate | Log |
| Count: steps, hits, octaves, pattern | IntLinear |
| Gain/mix/ratio: click_mix, sustain, diffusion, feedback, etc. | Linear |
| Q factor: filter_q, resonance | Log |
| Cents: detune_cents, vibrato_depth | Linear |
| Bandwidth: bandwidth | Log |

### 4. Add normalized_params / set_from_normalized to CellDna

`src/organism/dna.rs`: Methods that use registry to convert. These are the public API for mutation, crossover, and ML export.

### 5. Add EnumDescriptor for string params

`src/dsp/cell/mod.rs`: Registration of valid variants per string param. Cells register `["sine_cluster", "pulse", "noise", ...]` for `osc.mode`.

### 6. Add mutation operators

`src/organism/mutation.rs` (new): `mutate_params()`, `crossover_params()` using normalized space. These are building blocks for future DNA evolution (specified in overview but not yet implemented).

## Files Created

| File | Description |
|------|-------------|
| `src/organism/mutation.rs` | Mutation/crossover operators in normalized space |

## Files Modified

| File | Changes |
|------|---------|
| `src/dsp/cell/mod.rs` | `ParamScale`, `ParamDescriptor`, `EnumDescriptor`, upgraded `CellRegistry` |
| `src/organism/dna.rs` | `normalized_params()`, `set_from_normalized()` on CellDna |

## Verification

- [ ] `ParamDescriptor::to_native(to_normalized(x)) ≈ x` for all params (round-trip)
- [ ] Log-scaled frequency: normalized 0.5 maps to geometric mean of min/max
- [ ] IntLinear: normalized 0.0 → min rounded, 1.0 → max rounded
- [ ] `param_ranges()` backward-compat method returns same values as before
- [ ] `param_or()` continues to work unchanged (native storage preserved)
- [ ] Mutation with `sigma=0.05` produces musically reasonable variation across all cell types
- [ ] String param normalization: uniform distribution across variants
- [ ] Crossover of two CellDna produces valid params within registered ranges
- [ ] All existing tests pass (no storage format change)
