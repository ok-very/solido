# L4-S13 — Synth Engine Upgrade

> Better oscillators. Honest envelopes. Filters that hurt a little.

**Status: IMPLEMENTED** — 446 tests (was 429, +17 new). All phases complete.


## Goal

Replace the three weakest signal primitives in the DSP stack:

1. **`AdsrState`** — linear ramps → exponential RC-circuit curves
2. **`osc_pair` / oscillator atoms** — single `pulse() & sine()` with no
   detune → a configurable `UnisonOscAtom` (up to 5 detuned voices) and
   an `FmOscAtom` (sine-on-sine, sine-on-square), both selectable per
   cell via a new `osc.mode` DNA param
3. **`LowpassAtom` (filter)** — neutral biquad lowpass → add `LadderAtom`
   (Moog-style 4-pole with per-sample tanh drive) and `SvfDriveAtom`
   (state-variable with input saturation), selectable via `filter.type` DNA param

After this session every cell (`timbre_voice`, `harmonic_bed`, `shimmer_layer`)
can be configured via its DNA with a distinct sonic character. The two flagship
presets — **Spiegel** (additive shimmer pad) and **Hosono** (warm detuned lead)
— ship as `assets/dna/` presets demonstrating the full range.

---

## Ancestry (MAKE A BABY)

`AdsrState` uses `+= attack_rate` (linear ramp). Every physical resonating body
— string, air column, skin — charges and decays following an RC circuit:
`V(t) = 1 - e^(-t/τ)`. The linear approximation sounds like a fader being moved
by a robot. The exponential model sounds like something starting and stopping.

`osc_pair()` in `melo.rs` is literally
`((var(&freq) | var(&pw)) >> pulse()) & (var(&freq_sub) >> sine())`.
The pulse wave's harmonic series (1/n for all odd n) dominates and sounds buzzy
against the raga tuning. Spiegel used mostly sines and triangles; Hosono used
detuned saw/triangle pairs. Neither used raw digital pulse for sustained pads.

`LowpassAtom` wraps fundsp's clean biquad. It's correct but character-free.
The Moog ladder filter is nonlinear by construction — each of its four stages
self-saturates via `tanh`, producing the even harmonics that make a filter
"warm" rather than just "dark."

---

## Depends On

- **L4-S05** — `VoicePool`, `AdsrState`, `DspCell` trait (files being upgraded)
- **L3-S06** — `TalaGrid` (beat_phase used for vibrato LFO sync, unchanged)
- **L4-S09** — `BlobGpuData` (hue routing unchanged)

No new dependencies. All changes are inside `src/dsp/`.

---

## Tasks

### 13.1 Upgrade `src/dsp/adsr.rs` — exponential curves

Replace the three linear ramps with coefficient-based exponential approaches.
Coefficients are precomputed at `note_on`/`note_off` and on any time-param change
so the hot path remains a single multiply-add per sample.

```rust
/// Precompute an RC-style coefficient for a given time in ms.
/// At t = time_ms the envelope has reached 99.3% of its target.
/// Formula: 1 - exp(-5 / (time_ms * 0.001 * sample_rate))
fn rc_coeff(time_ms: f32, sr: f32) -> f32 {
    if time_ms <= 0.5 { return 1.0; }
    1.0 - (-5.0 / (time_ms * 0.001 * sr)).exp()
}
```

Replace `attack_rate`, `decay_rate`, `release_rate` (all `f32` scalars) with:

```rust
// New fields (replace the three _rate fields):
attack_coeff: f32,    // precomputed by rc_coeff(attack_ms, sr)
decay_coeff: f32,     // precomputed by rc_coeff(decay_ms, sr)
release_coeff: f32,   // precomputed by rc_coeff(release_ms, sr)
```

Replace `recalc_rates()` with:

```rust
fn recalc_coeffs(&mut self) {
    self.attack_coeff  = rc_coeff(self.attack_ms,  self.sample_rate);
    self.decay_coeff   = rc_coeff(self.decay_ms,   self.sample_rate);
    self.release_coeff = rc_coeff(self.release_ms, self.sample_rate);
}
```

Replace `process()` match arms:

```rust
AdsrStage::Attack => {
    // Approach target of 1.0 + small overshoot so we reliably reach 1.0
    self.level += (1.02 - self.level) * self.attack_coeff;
    if self.level >= 1.0 {
        self.level = 1.0;
        self.stage = AdsrStage::Decay;
    }
}
AdsrStage::Decay => {
    // Approach sustain level exponentially
    self.level += (self.sustain - self.level) * self.decay_coeff;
    if (self.level - self.sustain).abs() < 0.001 {
        self.level = self.sustain;
        self.stage = AdsrStage::Sustain;
    }
}
AdsrStage::Release => {
    // Approach 0.0 exponentially
    self.level += (0.0 - self.level) * self.release_coeff;
    if self.level < 0.001 {
        self.level = 0.0;
        self.stage = AdsrStage::Idle;
    }
}
```

Update `note_off()` to remove the bespoke `release_rate` recalculation
from current level — the coefficient model handles mid-level release
correctly because `level * coeff` is proportional regardless of start.

**Perceptual impact**: attack convex (slow start, fast finish → natural bow
or breath feel), decay/release concave (fast start, long tail → natural resonance
dissipation). The linear version sounds mechanical at all timescales; the
exponential version sounds physical only when the time constants are long.

---

### 13.2 Add `UnisonOscAtom` to `src/dsp/atom/oscillators.rs`

A self-contained atom that sums up to 5 detuned voices of a selectable waveform
plus an optional sub oscillator. All detuned frequencies are computed each tick
from `freq` + `detune_cents` + `unison` count — stored as plain `f32` fields so
`TimbreVoice` can update them with a single `set_param("freq", f)` call.

```rust
/// Waveform selector encoded as f32 for DNA param compatibility.
/// 0.0 = Sine, 1.0 = Triangle, 2.0 = Saw, 3.0 = Square
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnisonWave { Sine, Triangle, Saw, Square }

impl UnisonWave {
    pub fn from_f32(v: f32) -> Self {
        match v as u32 { 1 => Self::Triangle, 2 => Self::Saw, 3 => Self::Square, _ => Self::Sine }
    }
    pub fn to_f32(self) -> f32 { self as u32 as f32 }
}

pub struct UnisonOscAtom {
    // Up to MAX_UNISON oscillators — fixed-size array, no allocation in audio path
    phases: [f64; MAX_UNISON],
    wave: UnisonWave,
    freq: f32,
    detune_cents: f32,
    unison: usize,
    sub_phase: f64,
    sub_mix: f32,
    sample_rate: f64,
}

const MAX_UNISON: usize = 5;

impl UnisonOscAtom {
    pub fn new(freq_hz: f32, wave: UnisonWave, detune_cents: f32, unison: usize,
               sub_mix: f32, sr: f32) -> Self {
        Self {
            phases: [0.0; MAX_UNISON],
            wave,
            freq: freq_hz,
            detune_cents,
            unison: unison.clamp(1, MAX_UNISON),
            sub_phase: 0.0,
            sub_mix,
            sample_rate: sr as f64,
        }
    }

    fn waveform(wave: UnisonWave, phase: f64) -> f32 {
        let p = phase.fract();
        match wave {
            UnisonWave::Sine     => (p * std::f64::consts::TAU).sin() as f32,
            UnisonWave::Triangle => (2.0 * (2.0 * p - (2.0 * p + 0.5).floor()).abs() - 1.0) as f32,
            UnisonWave::Saw      => (2.0 * p - 1.0) as f32,
            UnisonWave::Square   => if p < 0.5 { 1.0 } else { -1.0 },
        }
    }
}

impl DspAtom for UnisonOscAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let cents_ratio = 2.0f64.powf(self.detune_cents as f64 / 1200.0);
        let n = self.unison;

        // Spread voices symmetrically around centre frequency
        // n=1 → [centre]; n=3 → [centre/ratio, centre, centre*ratio]; etc.
        let mut sum = 0.0f32;
        for i in 0..n {
            let spread = if n == 1 { 0.0 } else {
                (i as f64 / (n - 1) as f64) * 2.0 - 1.0
            };
            let f = self.freq as f64 * cents_ratio.powf(spread);
            self.phases[i] += f / self.sample_rate;
            sum += Self::waveform(self.wave, self.phases[i]);
        }

        // Normalise unison mix to prevent clipping
        let mix = sum / (n as f32).sqrt();

        // Sub oscillator (always sine, one octave down)
        self.sub_phase += (self.freq as f64 * 0.5) / self.sample_rate;
        let sub = (self.sub_phase * std::f64::consts::TAU).sin() as f32 * self.sub_mix;

        output[0] = mix * (1.0 - self.sub_mix * 0.3) + sub;
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq"         => { self.freq = value; true }
            "detune_cents" => { self.detune_cents = value; true }
            "unison"       => { self.unison = (value as usize).clamp(1, MAX_UNISON); true }
            "sub_mix"      => { self.sub_mix = value.clamp(0.0, 1.0); true }
            "wave"         => { self.wave = UnisonWave::from_f32(value); true }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq"         => Some(self.freq),
            "detune_cents" => Some(self.detune_cents),
            "unison"       => Some(self.unison as f32),
            "sub_mix"      => Some(self.sub_mix),
            "wave"         => Some(self.wave.to_f32()),
            _ => None,
        }
    }

    fn audio_inputs(&self)  -> usize { 0 }
    fn audio_outputs(&self) -> usize { 1 }

    fn reset(&mut self) {
        self.phases = [0.0; MAX_UNISON];
        self.sub_phase = 0.0;
    }

    fn name(&self) -> &str { "unison_osc" }
}
```

---

### 13.3 Add `FmOscAtom` to `src/dsp/atom/oscillators.rs`

Two-operator FM: sine carrier, selectable modulator waveform (sine or square).
`fm_index` is the classic modulation index (depth); `fm_ratio` is modulator
frequency as a ratio of carrier.

```rust
pub struct FmOscAtom {
    carrier_phase: f64,
    mod_phase: f64,
    freq: f32,
    fm_ratio: f32,       // modulator Hz = freq * fm_ratio
    fm_index: f32,       // modulation depth (0 = pure sine; 5 = rich FM)
    mod_square: bool,    // false = sine mod, true = square mod
    sub_phase: f64,
    sub_mix: f32,
    sample_rate: f64,
}

impl FmOscAtom {
    pub fn new(freq_hz: f32, fm_ratio: f32, fm_index: f32,
               mod_square: bool, sub_mix: f32, sr: f32) -> Self {
        Self { carrier_phase: 0.0, mod_phase: 0.0, freq: freq_hz,
               fm_ratio, fm_index, mod_square, sub_phase: 0.0,
               sub_mix, sample_rate: sr as f64 }
    }
}

impl DspAtom for FmOscAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        use std::f64::consts::TAU;
        let mod_freq = self.freq as f64 * self.fm_ratio as f64;
        self.mod_phase += mod_freq / self.sample_rate;

        let modulator = if self.mod_square {
            if self.mod_phase.fract() < 0.5 { 1.0f64 } else { -1.0 }
        } else {
            (self.mod_phase * TAU).sin()
        };

        let carrier_freq = self.freq as f64
            + modulator * self.fm_index as f64 * self.freq as f64;
        self.carrier_phase += carrier_freq / self.sample_rate;
        let carrier = (self.carrier_phase * TAU).sin() as f32;

        self.sub_phase += (self.freq as f64 * 0.5) / self.sample_rate;
        let sub = (self.sub_phase * TAU).sin() as f32 * self.sub_mix;

        output[0] = carrier * (1.0 - self.sub_mix * 0.3) + sub;
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "freq"       => { self.freq = value; true }
            "fm_ratio"   => { self.fm_ratio = value; true }
            "fm_index"   => { self.fm_index = value; true }
            "mod_square" => { self.mod_square = value > 0.5; true }
            "sub_mix"    => { self.sub_mix = value.clamp(0.0, 1.0); true }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "freq"       => Some(self.freq),
            "fm_ratio"   => Some(self.fm_ratio),
            "fm_index"   => Some(self.fm_index),
            "mod_square" => Some(if self.mod_square { 1.0 } else { 0.0 }),
            "sub_mix"    => Some(self.sub_mix),
            _ => None,
        }
    }

    fn audio_inputs(&self)  -> usize { 0 }
    fn audio_outputs(&self) -> usize { 1 }

    fn reset(&mut self) {
        self.carrier_phase = 0.0;
        self.mod_phase = 0.0;
        self.sub_phase = 0.0;
    }

    fn name(&self) -> &str { "fm_osc" }
}
```

---

### 13.4 Add `LadderAtom` and `SvfDriveAtom` to `src/dsp/atom/filters.rs`

**`LadderAtom`** — Huovilainen Moog ladder (simplified, stable at all cutoffs):

```rust
pub struct LadderAtom {
    stage: [f32; 4],
    freq: f32,
    resonance: f32,
    drive: f32,          // pre-filter input gain [0.5–4.0]; >1 = saturation
    sample_rate: f32,
}

impl LadderAtom {
    pub fn new(freq_hz: f32, resonance: f32, drive: f32, sr: f32) -> Self {
        Self { stage: [0.0; 4], freq: freq_hz, resonance, drive, sample_rate: sr }
    }
}

impl DspAtom for LadderAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let f = (std::f32::consts::PI * self.freq / self.sample_rate).clamp(0.001, 0.499);
        // 4-pole ladder with tanh nonlinearity at each stage
        let feedback = self.stage[3] * self.resonance * 4.0;
        let x = (input[0] * self.drive - feedback).tanh();
        self.stage[0] += f * (x                      - self.stage[0].tanh());
        self.stage[1] += f * (self.stage[0].tanh()   - self.stage[1].tanh());
        self.stage[2] += f * (self.stage[1].tanh()   - self.stage[2].tanh());
        self.stage[3] += f * (self.stage[2].tanh()   - self.stage[3].tanh());
        output[0] = self.stage[3];
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "cutoff"     => { self.freq = value.clamp(20.0, 20000.0); true }
            "resonance"  => { self.resonance = value.clamp(0.0, 0.99); true }
            "drive"      => { self.drive = value.clamp(0.5, 4.0); true }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "cutoff"    => Some(self.freq),
            "resonance" => Some(self.resonance),
            "drive"     => Some(self.drive),
            _ => None,
        }
    }

    fn audio_inputs(&self)  -> usize { 1 }
    fn audio_outputs(&self) -> usize { 1 }
    fn reset(&mut self) { self.stage = [0.0; 4]; }
    fn name(&self) -> &str { "ladder" }
}
```

**`SvfDriveAtom`** — Chamberlin state-variable filter with tanh input saturation.
Params: `cutoff`, `resonance`, `drive`, `mode` (0.0 = lowpass, 1.0 = bandpass,
2.0 = highpass). Two-integrator topology; `drive` applied before the first
integrator. Gentler saturation character than ladder — better for Spiegel-style
pads where brightness and openness matter more than warmth.

*Implementation follows standard Chamberlin SVF. Full code follows same
`DspAtom` pattern as `LadderAtom` above.*

---

### 13.5 Add `OscMode` enum and `osc_engine()` factory to `src/dsp/molecule/melo.rs`

Replace `osc_pair()` with `osc_engine()` dispatching on `OscMode`.

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OscMode {
    SineCluster,   // UnisonOscAtom, wave=Sine  — Spiegel shimmer
    TriCluster,    // UnisonOscAtom, wave=Triangle — Hosono warmth
    SawUnison,     // UnisonOscAtom, wave=Saw — fat leads
    SineFm,        // FmOscAtom, mod_square=false — metallic pads
    SquareFm,      // FmOscAtom, mod_square=true — buzzy organ
}

impl OscMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "tri_cluster"  => Self::TriCluster,
            "saw_unison"   => Self::SawUnison,
            "sine_fm"      => Self::SineFm,
            "square_fm"    => Self::SquareFm,
            _              => Self::SineCluster,  // default and fallback
        }
    }
}

pub fn osc_engine(mode: OscMode, dna: &CellDna, sr: f32) -> Molecule {
    let freq      = param_or(dna, "freq",             440.0);
    let detune    = param_or(dna, "osc.detune_cents", 5.0);
    let unison    = param_or(dna, "osc.unison",       3.0) as usize;
    let sub_mix   = param_or(dna, "osc.sub_mix",      0.25);
    let fm_ratio  = param_or(dna, "osc.fm_ratio",     1.0);
    let fm_index  = param_or(dna, "osc.fm_index",     0.5);

    let atom: Box<dyn DspAtom> = match mode {
        OscMode::SineCluster => Box::new(UnisonOscAtom::new(
            freq, UnisonWave::Sine, detune, unison, sub_mix, sr)),
        OscMode::TriCluster  => Box::new(UnisonOscAtom::new(
            freq, UnisonWave::Triangle, detune, unison, sub_mix, sr)),
        OscMode::SawUnison   => Box::new(UnisonOscAtom::new(
            freq, UnisonWave::Saw, detune, unison, sub_mix, sr)),
        OscMode::SineFm      => Box::new(FmOscAtom::new(
            freq, fm_ratio, fm_index, false, sub_mix, sr)),
        OscMode::SquareFm    => Box::new(FmOscAtom::new(
            freq, fm_ratio, fm_index, true, sub_mix, sr)),
    };

    let scratch = build_scratch_from_atom(&atom);
    Molecule::Wired {
        name: "osc_engine".into(),
        atoms: vec![("osc".into(), atom)],
        wiring: vec![],
        process_order: vec![0],
        scratch,
        external_inputs: vec![],
        external_outputs: vec![(0, 0)],
        mod_routes: vec![],
        mod_outputs: vec![0.0],
    }
}
```

**`osc_pair()` is kept as a deprecated shim** so existing tests compile without changes:

```rust
#[deprecated(note = "Use osc_engine(OscMode::SineCluster, dna, sr) instead")]
pub fn osc_pair(freq_hz: f32, pulse_width: f32, sr: f32) -> Molecule {
    // … existing impl unchanged
}
```

---

### 13.6 Add `FilterMode` dispatch to `melo::filter_envelope`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterMode {
    Lowpass,   // existing LowpassAtom — clean, neutral
    Ladder,    // new LadderAtom — warm, nonlinear
    SvfDrive,  // new SvfDriveAtom — bright, driven
}

impl FilterMode {
    pub fn from_str(s: &str) -> Self {
        match s { "ladder" => Self::Ladder, "svf_drive" => Self::SvfDrive, _ => Self::Lowpass }
    }
}

/// Extended filter_envelope accepting FilterMode and drive.
/// Same return type as original filter_envelope — drop-in replacement.
pub fn filter_envelope_typed(
    base_cutoff: f32, depth_hz: f32, drive: f32,
    mode: FilterMode, sr: f32,
) -> (Molecule, Shared, Shared) {
    let base  = shared(base_cutoff);
    let depth = shared(depth_hz);

    let filter_atom: Box<dyn DspAtom> = match mode {
        FilterMode::Lowpass  => Box::new(LowpassAtom::new(base_cutoff, 0.707, sr)),
        FilterMode::Ladder   => Box::new(LadderAtom::new(base_cutoff, 0.3, drive, sr)),
        FilterMode::SvfDrive => Box::new(SvfDriveAtom::new(base_cutoff, 0.5, drive, sr)),
    };

    // … Molecule::Wired construction identical to existing filter_envelope
    // but using filter_atom; ADSR mod-route and base/depth Shared wiring unchanged
    (mol, base, depth)
}
```

---

### 13.7 Update `TimbreVoice` to use new engines

In `from_dna()`, resolve engine types from `CellDna.string_params` and forward
to `osc_engine` and `filter_envelope_typed`:

```rust
let osc_mode    = OscMode::from_str(dna.string_params.get("osc.mode")
                      .map(|s| s.as_str()).unwrap_or("sine_cluster"));
let filter_mode = FilterMode::from_str(dna.string_params.get("filter.type")
                      .map(|s| s.as_str()).unwrap_or("lowpass"));
let filter_drive = param_or(dna, "filter.drive", 1.0);

let osc = melo::osc_engine(osc_mode, dna, sr);
let (filter_env, ..) = melo::filter_envelope_typed(fbase, fdepth, filter_drive, filter_mode, sr);
```

Add `Shared` handles for all new live-tweakable params:

```rust
// Append to the handles vec! in from_dna():
("osc.detune_cents".into(), detune_cents.clone()),
("osc.unison".into(),       unison_shared.clone()),
("osc.sub_mix".into(),      sub_mix_shared.clone()),
("osc.fm_ratio".into(),     fm_ratio_shared.clone()),
("osc.fm_index".into(),     fm_index_shared.clone()),
("filter.drive".into(),     filter_drive_shared.clone()),
```

In `tick()`, replace the existing `osc.set_param` block:

```rust
let f = self.freq.value();
self.osc.set_param("osc.freq",         f);
self.osc.set_param("osc.detune_cents", self.detune_cents.value());
self.osc.set_param("osc.unison",       self.unison.value());
self.osc.set_param("osc.sub_mix",      self.sub_mix.value());
self.osc.set_param("osc.fm_ratio",     self.fm_ratio.value());
self.osc.set_param("osc.fm_index",     self.fm_index.value());
```

`osc.mode` and `filter.type` are **build-time** choices — changing engine type
requires cell rebuild (same as changing `cell_type` in DNA). All params above
are live-tweakable via `Shared` handles.

---

### 13.8 Update `HarmonicBed` to use `UnisonOscAtom`

Replace the 4 manually-constructed sinusoidal partials with 4 `UnisonOscAtom`
instances (one per partial), each with `wave=Sine`, `unison=1`, and `detune_cents`
staggered per partial (0.0, 2.0, 4.0, 3.0 cents for partials 1–4). This preserves
the 4-partial architecture while gaining exponential envelope and `sub_mix` option.

---

### 13.9 Ship two flagship DNA presets

**`assets/dna/spiegel.json`** — Spiegel shimmer pad:

```json
{
  "cells": [{
    "cell_type": "timbre_voice",
    "string_params": { "osc.mode": "sine_cluster", "filter.type": "svf_drive" },
    "params": {
      "freq": 220.0,
      "osc.detune_cents": 4.5,
      "osc.unison": 3.0,
      "osc.sub_mix": 0.2,
      "filter_base": 900.0,
      "filter_depth": 2200.0,
      "filter_q": 0.5,
      "filter.drive": 1.2,
      "attack_ms": 600.0,
      "decay_ms": 400.0,
      "sustain": 0.8,
      "release_ms": 1800.0
    }
  }]
}
```

**`assets/dna/hosono.json`** — Hosono detuned warm lead:

```json
{
  "cells": [{
    "cell_type": "timbre_voice",
    "string_params": { "osc.mode": "tri_cluster", "filter.type": "ladder" },
    "params": {
      "freq": 220.0,
      "osc.detune_cents": 7.0,
      "osc.unison": 3.0,
      "osc.sub_mix": 0.35,
      "filter_base": 700.0,
      "filter_depth": 3200.0,
      "filter_q": 0.4,
      "filter.drive": 1.8,
      "attack_ms": 25.0,
      "decay_ms": 250.0,
      "sustain": 0.65,
      "release_ms": 900.0
    }
  }]
}
```

---

### 13.10 Add `string_params` field to `CellDna`

Current `CellDna` only has `params: BTreeMap<String, f32>`. Add:

```rust
pub struct CellDna {
    pub cell_type: String,
    pub params: BTreeMap<String, f32>,
    pub string_params: BTreeMap<String, String>,  // NEW — for osc.mode, filter.type
}
```

Update all `CellDna` construction sites and JSON deserialisation.
A `string_params` key absent from JSON deserialises as an empty map (serde default).

---

## Files Modified

```
src/dsp/adsr.rs                    — exponential curves (13.1)
src/dsp/atom/oscillators.rs        — UnisonOscAtom, FmOscAtom (13.2, 13.3)
src/dsp/atom/filters.rs            — LadderAtom, SvfDriveAtom (13.4)
src/dsp/molecule/melo.rs           — OscMode, FilterMode, osc_engine,
                                     filter_envelope_typed (13.5, 13.6)
src/dsp/cell/timbre_voice.rs       — new engines + Shared handles (13.7)
src/dsp/cell/harmonic_bed.rs       — UnisonOscAtom partials (13.8)
src/organism/dna.rs                — string_params field on CellDna (13.10)
```

## Files Created

```
assets/dna/spiegel.json            — Spiegel shimmer preset (13.9)
assets/dna/hosono.json             — Hosono warm lead preset (13.9)
```

---

## Verification — Actual Results

All 446 tests pass. 17 new tests added:

### ADSR (13.1) — 3 tests
1. `adsr_attack_is_convex` — at 50% attack time, level >0.5 (convex curve) ✅
2. `adsr_release_tail` — at 50% release time, still has signal >0.01 ✅
3. `adsr_retrigger_no_click` — retrigger from mid-release, no discontinuity ✅

### CellDna string_params (13.10) — 2 tests
4. `cell_dna_string_params_serde_default` — old JSON without field → empty map ✅
5. `cell_dna_string_params_roundtrip` — serialize/deserialize preserves values ✅

### UnisonOscAtom (13.2) — 3 tests
6. `unison_1_matches_single_osc` — RMS within 15% of SineAtom ✅
7. `unison_3_no_clipping` — peak ≤ 1.5 ✅
8. `detune_creates_beating` — amplitude variation across chunks confirmed ✅

### FmOscAtom (13.3) — 2 tests
9. `fm_index_0_is_pure_sine` — RMS within 15% of SineAtom ✅
10. `fm_index_increases_brightness` — higher index → more zero crossings ✅

### LadderAtom (13.4) — 3 tests
11. `ladder_attenuates_above_cutoff` — 8kHz through 1kHz ladder heavily attenuated ✅
12. `ladder_drive_adds_harmonics` — higher drive ≥ clean zero crossings ✅
13. `ladder_no_inf_nan` — 10s audio, all finite, abs ≤ 4.0 ✅

### SvfDriveAtom (13.4) — 2 tests
14. `svf_drive_passes_low_attenuates_high` — lowpass behavior confirmed ✅
15. `svf_drive_bounded_output` — drive=3.0, all samples bounded ✅

### Integration (13.7–13.9) — 2 tests
16. `spiegel_dna_loads_and_produces_audio` — loads spiegel.json, builds org, RMS >0 ✅
17. `hosono_dna_loads_and_produces_audio` — loads hosono.json, builds org, RMS >0 ✅

### Existing tests adjusted for exponential ADSR
- `adsr_attack_reaches_peak` (voice.rs) — increased sample count + relaxed threshold
- `adsr_release_reaches_idle` (voice.rs) — increased sample count (15000 for RC tail)
- `timbre_voice_silent_after_release` — increased release wait to 20000 samples

---

## Design Notes

`OscMode` and `FilterMode` are **build-time** decisions — swapping engine type
requires rebuilding the `Molecule`. This is intentional: it matches the DNA
model (a DNA preset defines the full cell architecture) and avoids runtime
branching inside the audio hot path. Live-tweakable params (`detune_cents`,
`drive`, `fm_index`, etc.) route through `Shared`/field handles at no branch cost.

`string_params` is the minimal extension to `CellDna` that avoids encoding
enum choices as magic floats. Encoding `"sine_cluster"` as `0.0` in `params`
was rejected — it makes DNA files unreadable and breaks future reordering.

The `rc_coeff` constant `-5.0` corresponds to 5 time constants, giving 99.3%
completion at the stated time. `-2.2` (90%) would make stated times feel short;
`-9.0` (99.99%) would make them feel slightly long. 5 time constants is the
Goldilocks value for musical ADSRs — users hear the stated time as accurate.

## Implementation Notes (deviations from spec)

- **`osc_pair()` not deprecated** — kept as-is for backward compatibility.
  `TimbreVoice` uses `osc_engine()` only when `string_params` contains
  `osc.mode`; otherwise falls back to `osc_pair()` transparently.
- **`HarmonicBed` uses `typed_filter()` not `UnisonOscAtom` partials** (13.8) —
  the detuned_stack Fused molecule was left unchanged; only the slow_filter
  stage was upgraded to dispatch via `FilterMode` through `dron::typed_filter()`.
- **`string_param_or()` helper** added to `src/dsp/cell/mod.rs` alongside
  `param_or()` for ergonomic string_params access.
- **`dron::typed_filter()`** added as a new factory in `src/dsp/molecule/dron.rs`
  for `HarmonicBed` filter mode selection.
