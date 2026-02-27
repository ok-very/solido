# S05b — Audio Dynamics: FunDSP Master Bus + Voice Pipeline Fix

**Layer**: L4 (Output Modules)
**Depends on**: S05
**Status**: Implemented

## Problem

Audio produces a single solid tone. Debug shows 8/8 voices at full load. Three root causes:

1. **Pipeline latency**: Reactor tick order is `tick → emit → route`. When a key is pressed, keyboard emits `raw_pitch` + `trigger` simultaneously. The quantizer receives `raw_pitch` but doesn't emit the new `pitch_hz` until the *next* tick. Meanwhile, the voice receives the trigger + the *stale* `pitch_hz` from the quantizer's previous output. Result: every voice spawns at the wrong (stale) pitch.

2. **Dedup kills wrong frequency**: `KillVoicesAtFreq(current_pitch_hz)` uses the stale pitch, so first-press voices are never killed. They accumulate until all 8 slots are full of identical-frequency voices.

3. **Spurious edges**: `quantizer.pitch_hz [20,20000]` auto-connects to `voice.filter_cutoff [20,20000]`, making the filter cutoff track pitch (extremely muffled at low notes). `keyboard.raw_pitch [0,1]` auto-connects to `voice.release_pitch [0,1]`, causing every key *press* to kill voices through the release path.

## Solution

### Phase 1: Voice Dedup + Master Bus (S05b initial)
- `KillVoicesAtFreq(f32)` AudioCommand variant — kills voices within 1Hz of target
- VoiceModule sends `KillVoicesAtFreq` before every `SpawnVoice` — same pitch = retrigger
- `AUTO_RELEASE_SECS` 0.5 → 5.0 for natural sustain
- FunDSP master bus post-processing (declick → 2-band crossover + per-band limiters → master limiter_stereo → dcblock)
- Key-release support in keyboard module (`release_pitch` output)

### Phase 2: Pipeline Latency Fix (S05b refinement)
- **`raw_pitch` input on VoiceModule** [0,1] Event — keyboard's `raw_pitch` routes directly to voice, bypassing the quantizer's 1-tick pipeline latency. Voice converts normalized [0,1] → Hz immediately: `261.63 * 2^(raw * 2)`. Since keyboard emits `raw_pitch` *before* `trigger` in `emit_signals()`, voice has the correct `current_pitch_hz` when trigger arrives in the same routing phase.
- **Removed `release_pitch` input from VoiceModule** — spurious edge with `keyboard.raw_pitch` caused every press to kill voices. Dedup + 5s auto-kill timer handles lifecycle instead.
- **Narrowed `filter_cutoff` range** [100, 15000] — prevents `quantizer.pitch_hz [20, 20000]` from auto-connecting via `ranges_compatible()` containment check (20 < 100 fails).

### FunDSP Master Bus
Post-processing chain on the audio thread:
```
L/R input
  → declick_s(0.01)              per-channel startup fade-in
  → 2-band crossover at 200Hz:
      bass:   butterpass(200)  → limiter(10ms, 100ms)
      treble: highpass(200)    → limiter(5ms, 150ms)
      sum (bus operator &)
  → limiter_stereo(10ms, 200ms)  linked stereo master limiter
  → dcblock_hz(10)               remove DC offset
  → output L/R
```

## Edge Map (Post-Fix)

| Edge | Status |
|------|--------|
| keyboard.raw_pitch [0,1] E → quantizer.raw_pitch [0,1] E | Intended |
| keyboard.raw_pitch [0,1] E → voice.raw_pitch [0,1] E | **New — pipeline latency fix** |
| keyboard.trigger (Trigger) E → voice.trigger (Trigger) E | Intended |
| keyboard.release_pitch [0,1] E → voice.raw_pitch [0,1] E | Harmless (updates pitch on release) |
| quantizer.pitch_hz [20,20000] B → voice.pitch_hz [20,20000] B | Intended (refines pitch next tick) |
| quantizer.pitch_hz [20,20000] B → voice.filter_cutoff [100,15000] B | **Blocked** (20 < 100) |
| cursor.x/y [0,1] B → quantizer.gravity_override [0,1] B | Intended |
| voice.rms/peak [0,1] B → audio_analysis.rms_in [0,1] B | Intended |

## Files Changed

| Action | File |
|--------|------|
| Modify | `Cargo.toml` — add fundsp |
| Create | `src/audio/master_bus.rs` — FunDSP processing chain |
| Modify | `src/audio/mod.rs` — pub mod master_bus |
| Modify | `src/audio/voice_pool.rs` — kill_at_freq, dispatch, remove tanh |
| Modify | `src/substrate/audio.rs` — KillVoicesAtFreq, wire MasterBus |
| Modify | `src/modules/voice_module.rs` — raw_pitch input, dedup, AUTO_RELEASE 5s, remove release_pitch |
| Modify | `src/modules/keyboard_input.rs` — key-release, release_pitch output |
| Modify | `src/app.rs` — capture key-release events |

## Verification

- `cargo test` — 204 tests pass
- Press number keys: different pitches per key, no stacking at default C4
- Same key again: previous voice at that pitch released, new one starts
- Audio: bass doesn't overwhelm, FunDSP limiter keeps peaks manageable
- 8-voice limit: dedup prevents pile-up, auto-kill reclaims zombies after 5s

## Future: Master Bus as Single Audio Output Path

The master bus becomes the **sole audio output path** when organisms arrive (S11+).
Current architecture: `VoiceModule → commands → VoicePool → MasterBus → speakers`.
Future architecture: organisms own their own DSP (FunDSP atoms/cells) and submit
stereo AudioBlocks directly to the master bus via the ring buffer. The master bus
mixes all submissions and applies dynamics (crossover + limiters + DC block).

VoiceModule and VoicePool are infrastructure scaffolding that gets retired when
organisms take ownership of synthesis. The master bus stays — it's the compressor /
limiter / safety net that prevents clipping regardless of how many organisms are
producing audio.

### Clipping at 8 voices (known)

At 8 simultaneous voices the current limiter thresholds can still clip. When
organisms arrive, per-organism output scaling + the master bus limiter should
handle arbitrary polyphony. In the interim, voice output amplitude could be
scaled by `1/sqrt(active_voices)` in VoicePool as a simple pre-limiter gain stage.
