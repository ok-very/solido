# S03 — Voice Engine

> Build a synthesis voice. The modern `cycle~`.

## Goal

Replace the raw sine from S01 with a proper synthesis voice:
oscillator + state-variable filter + ADSR envelope.
Multiple voices can run simultaneously via a voice pool.

## Ancestry (MAKE A BABY)

The Max/MSP patch ran 5+ parallel `cycle~` voices, each with:
- Sine oscillator (core sound)
- `phasor~` for phase/FM modulation
- `line~` for smooth parameter ramps
- `tapin~/tapout~` for delay feedback
- `overdrive~` / `degrade~` for character

We start simpler: one oscillator type, one filter, one envelope.
The voice pool handles spawn/kill/crossfade lifecycle.

## Tasks

### 3.1 Add fundsp dependency

```toml
fundsp = "0.21"
```

fundsp provides audio-rate DSP nodes that compose into graphs.
We use it for oscillator + filter + envelope inside each voice.

### 3.2 Create `src/audio/voice.rs`

A voice is a self-contained synthesis unit:

```rust
pub struct Voice {
    pub id: u64,
    pub frequency: f32,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub amplitude: f32,
    pub active: bool,
    // internal DSP state
    phase: f64,
    envelope: AdsrState,
    svf: SvfState,
}
```

Voice DSP chain:
```
sine(freq) → svf_lowpass(cutoff, Q) → * amplitude → * envelope
```

### 3.3 ADSR envelope

Simple state machine (no fundsp dependency — just math):

```rust
pub struct AdsrState {
    pub attack_ms: f32,    // default: 10
    pub decay_ms: f32,     // default: 100
    pub sustain: f32,      // default: 0.7
    pub release_ms: f32,   // default: 200
    stage: AdsrStage,
    level: f32,
}

enum AdsrStage { Idle, Attack, Decay, Sustain, Release }
```

- `note_on()` → Attack stage
- `note_off()` → Release stage
- `process(dt) -> f32` — returns current envelope level

### 3.4 State-variable filter

Implement Chamberlin SVF (simple, stable, good for real-time):

```rust
pub struct SvfState {
    low: f32,
    band: f32,
    high: f32,
    notch: f32,
}
```

- `process(input, cutoff_hz, resonance, sample_rate) -> f32`
- Output mode: lowpass (default), switchable later
- Cutoff range: 20Hz–20kHz
- Resonance: 0.0–1.0 (maps to Q)

### 3.5 Create `src/audio/voice_pool.rs`

```rust
pub struct VoicePool {
    voices: Vec<Voice>,
    next_id: u64,
    max_voices: usize,  // default: 8
}
```

- `spawn(freq, cutoff, amplitude) -> VoiceId`
- `kill(id)` — triggers release, voice recycled when envelope reaches 0
- `set_param(id, param, value)` — frequency, cutoff, resonance, amplitude
- `process_block(output: &mut [f32], sample_rate: f32)` — mix all active voices

Voice stealing: if pool full, kill oldest voice in Release stage.

### 3.6 Wire VoicePool into AudioEngine

Replace S01's raw sine with the voice pool:
- AudioEngine's ring buffer now carries `VoiceEvent`:
  ```rust
  pub enum VoiceEvent {
      Spawn { freq: f32, cutoff: f32, amp: f32 },
      Kill(u64),
      SetParam { id: u64, param: VoiceParam, value: f32 },
  }
  ```
- Audio callback: drain events, then `pool.process_block(buffer)`

### 3.7 Test from keyboard

Temporary keyboard mapping in `app.rs`:
- Number keys 1-7: spawn voices at Bhairav scale degrees (hardcoded Hz)
- Backspace: kill most recent voice
- The tuning system (S02) isn't wired yet — just hardcode frequencies for now

## Files Created

```
src/audio/voice.rs        — Voice, sine osc, SvfState, AdsrState
src/audio/voice_pool.rs   — VoicePool, spawn/kill/process
```

## Files Modified

```
src/audio/mod.rs          — VoiceEvent enum, AudioEngine uses VoicePool
src/app.rs                — keyboard → VoiceEvent dispatch
Cargo.toml                — add fundsp (optional — may hand-roll DSP instead)
```

## Verification

1. `cargo run` — window + audio both work
2. Press 1 — a tone plays with audible attack envelope
3. Press 2, 3, 4 — additional tones layer (polyphonic)
4. Press backspace — tones release smoothly (no click)
5. Rapid key mashing — no crashes, voices recycle, max 8 simultaneous
6. Filter audible: voices sound warmer than raw sine (cutoff < 5kHz)
7. No glitches during voice spawn/kill

## Design Decisions

- Hand-rolled DSP (sine, SVF, ADSR) rather than fundsp graph:
  simpler, no framework dependency for basic operations, easier to
  debug. fundsp can be introduced later for complex voice graphs.
- Voice pool is fixed-size array, not Vec — predictable allocation.
- All DSP is f32 (not f64) — matches cpal output format.
