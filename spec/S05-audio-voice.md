# L4-S05 — First Output: Audio Voice Module

> Build a synthesis voice. The modern `cycle~`.

## Goal

Replace the audio substrate's silence with a proper synthesis voice:
oscillator + state-variable filter + ADSR envelope, managed by a
VoicePool, wrapped as a VoiceModule that receives `pitch_hz` and
`trigger` through the affinity graph. After this session, pressing
keys produces quantized Bhairav tones through speakers.

## Ancestry (MAKE A BABY)

The Max/MSP patch ran 5+ parallel `cycle~` voices, each with:
- Sine oscillator (core sound)
- `phasor~` for phase/FM modulation
- `line~` for smooth parameter ramps
- `tapin~/tapout~` for delay feedback
- `overdrive~` / `degrade~` for character

We start simpler: one oscillator type, one filter, one envelope.
The voice pool handles spawn/kill/crossfade lifecycle.

## Depends On

- L0-S01 (AudioSubstrate — cpal stream, ringbuf)
- L1-S02 (SeedReactor — VoiceModule registers with it)
- L3-S04 (QuantizerModule — source of pitch_hz signals)

## Tasks

### 5.1 Create `src/audio/voice.rs`

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

### 5.2 ADSR envelope

Simple state machine (no external dependency — just math):

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

### 5.3 State-variable filter

Chamberlin SVF (simple, stable, good for real-time):

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

### 5.4 Create `src/audio/voice_pool.rs`

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
Voice pool is fixed-size array, not Vec — predictable allocation.
All DSP is f32 (not f64) — matches cpal output format.

### 5.5 Wire VoicePool into AudioSubstrate

Replace silence with the voice pool:
- AudioSubstrate's ring buffer carries `AudioCommand` (already defined in S01)
- Audio callback: drain commands, then `pool.process_block(buffer)`
- No fundsp dependency — hand-rolled DSP for simplicity and debuggability

### 5.6 Create `src/modules/voice_module.rs` — VoiceModule

```rust
pub struct VoiceModule {
    schema: ModuleSchema,
    audio_tx: ringbuf::Producer<AudioCommand>,
    current_rms: f32,
    current_peak: f32,
    active_count: u32,
    max_voices: usize,
}
```

**Schema**:
- Inputs:
  - `pitch_hz` (Float, Event) — from QuantizerModule
  - `trigger` (Trigger, Event) — note-on
  - `filter_cutoff` (Float, Block) — cutoff Hz
  - `amplitude` (Float, Block) — volume 0.0-1.0
- Outputs:
  - `rms` (Float, Block) — current audio RMS level
  - `peak` (Float, Block) — current peak level
  - `is_active` (Bool, Block) — true if any voice playing
  - `voice_count` (Float, Block) — number of active voices

VoiceModule sits on the control thread, sends AudioCommands to the
audio thread via ringbuf. Analysis data flows back through a second
ringbuf (audio→control) carrying `AnalysisFrame { rms: f32, peak: f32 }`.

**Custom UI panel** (Tiered UI):
- Waveform type selector (sine only for now)
- Filter cutoff slider 20-20000 Hz
- Filter resonance slider 0.0-1.0
- ADSR sliders: A/D/S/R
- Voice count display
- Kill all button

### 5.7 Audio analysis feedback

Lightweight audio analysis on the audio thread:
- RMS level from the last rendered audio block
- Peak level from the last rendered audio block
- Send via second ring buffer: audio thread → control thread
- VoiceModule reads this each tick() and emits as output signals

This closes the feedback loop: the AudioAnalysisModule from S03
can now receive real data from VoiceModule through the reactor.

### 5.8 End-to-end path

```
keyboard_input → [affinity edge] → quantizer → [affinity edge] → voice_module → sound
                                                                       ↓
                                                               audio_analysis ← rms/peak
```

## Files Created

```
src/audio/voice.rs            — Voice, sine osc, SvfState, AdsrState
src/audio/voice_pool.rs       — VoicePool, spawn/kill/process
src/modules/voice_module.rs   — VoiceModule (Module impl)
```

## Files Modified

```
src/audio/mod.rs              — pub mod voice, voice_pool;
src/substrate/audio.rs        — VoicePool in audio callback
src/modules/mod.rs            — add pub mod voice_module;
src/app.rs                    — register VoiceModule with SeedReactor
```

## Verification

1. `cargo run` — window + audio both work
2. Press 1 — a Bhairav tone plays with audible attack envelope
3. Press 2, 3, 4 — additional tones layer (polyphonic)
4. Release keys — tones release smoothly (no click)
5. Rapid key mashing — no crashes, voices recycle, max 8 simultaneous
6. Filter audible: voices sound warmer than raw sine (cutoff < 5kHz)
7. No glitches during voice spawn/kill
8. Debug log: affinity edges between keyboard→quantizer and
   quantizer→voice strengthen over time
9. Audio analysis: RMS/peak values appear in debug log
10. No audio underruns during normal operation

## Design Decisions

- Hand-rolled DSP (sine, SVF, ADSR) rather than fundsp graph:
  simpler, no framework dependency for basic operations, easier to
  debug. fundsp can be introduced later for complex voice graphs.
- Voice pool is fixed-size array, not Vec — predictable allocation.
- All DSP is f32 (not f64) — matches cpal output format.
- Audio callback must be lock-free: no allocations, no mutexes, no panics.
