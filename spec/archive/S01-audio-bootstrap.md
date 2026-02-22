# S01 — Audio Bootstrap

> Silence → sine wave. Prove cpal works inside eframe.

## Goal

Get audio output working alongside the existing wgpu renderer.
A 440Hz sine tone plays when you press a key. That's it.

## Why This First

Everything downstream (voices, gravity, affinity→audio) depends on
a working audio thread. The tricky part is threading: cpal's audio
callback runs on a real-time thread, eframe owns the main thread,
and wgpu owns the GPU thread. They must never block each other.

## Ancestry (MAKE A BABY)

The Max/MSP patch used `cycle~` as its core oscillator and `dac~`
for 6-channel output. We start with the same primitive: a sine
oscillator writing to a stereo output stream.

## Tasks

### 1.1 Add dependencies

```toml
# Cargo.toml additions
cpal = "0.15"
ringbuf = "0.4"
```

### 1.2 Create `src/audio/mod.rs`

- Initialize `cpal` default output device
- Open output stream (44100Hz, stereo, f32 samples)
- Audio callback reads from a lock-free ring buffer
- Expose `AudioEngine::new() -> Self` and `AudioEngine::send_param(ParamEvent)`

```rust
pub struct AudioEngine {
    stream: cpal::Stream,
    tx: ringbuf::Producer<ParamEvent>,
}

pub enum ParamEvent {
    SetFrequency(f32),
    NoteOn,
    NoteOff,
}
```

### 1.3 Audio callback

The callback is allocation-free:
- Read `ParamEvent`s from ring buffer (non-blocking pop)
- Generate samples: `sin(phase * 2π)`, advance phase by `freq / sample_rate`
- Write interleaved stereo (L=R for now)
- Apply simple amplitude envelope (attack=5ms, release=50ms) to avoid clicks

### 1.4 Wire into `app.rs`

- `SolidoApp` holds `Option<AudioEngine>`
- Create `AudioEngine` in `SolidoApp::new()`
- On spacebar press: send `NoteOn` / `NoteOff` toggle
- On up/down arrows: send `SetFrequency` (±50Hz)

### 1.5 Thread safety

- `cpal::Stream` is `Send` but not `Sync` — store in main thread only
- `ringbuf` producer stays in main thread, consumer in audio callback
- No `Arc<Mutex<>>` anywhere near the audio path

## Files Created

```
src/audio/mod.rs     — AudioEngine, cpal setup, ring buffer, sine gen
```

## Files Modified

```
src/main.rs          — add `mod audio;`
src/app.rs           — hold AudioEngine, keyboard → NoteOn/NoteOff
Cargo.toml           — add cpal, ringbuf
```

## Verification

1. `cargo run` — window opens as before, SDF organisms render
2. Press spacebar — 440Hz sine tone plays through default audio device
3. Press up/down — pitch shifts audibly
4. Press spacebar again — tone stops cleanly (no click)
5. No audio glitches during window resize or organism interaction
6. `RUST_LOG=info cargo run` — no warnings about buffer underruns

## Constraints

- Audio callback must be lock-free: no allocations, no mutexes, no panics
- Block size should match cpal's preferred (typically 256-1024 samples)
- If cpal fails to initialize (no audio device), log warning and continue without audio
