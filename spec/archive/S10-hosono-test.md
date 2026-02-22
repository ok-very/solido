# S10 — The Hosono Test

> The system passes when it sounds like it meant to do that.

## Goal

Integration test. Wire everything together and verify the full
texture ↔ music continuum works end to end. The system should be
able to sustain a recognizable raga, dissolve into texture, and
recoalesce into a different raga — all driven by gravity state
changes — without sounding broken.

## The Test (from microtonal_gravity_plan)

The system passes if it can do all of the following without user
intervention (after initial setup):

1. Sustain a recognizable Bhairav texture for 30+ seconds
2. Dissolve that texture into free microtonal drift when gravity drops
3. Re-coalesce into a different raga (Yaman) as gravity rises again
4. Sound like it *meant* to do that

## Depends On

Everything. This is the integration session.

## Tasks

### 10.1 Automation script

Create a timed automation sequence that tests the full continuum:

```rust
pub struct AutomationStep {
    pub time_sec: f32,
    pub action: AutoAction,
}

pub enum AutoAction {
    SetRaga(String),
    SetGravity(f32),
    SetTempo(f64),
    SetEuclideanHits(u32),
    SpawnDrone,
    KillAll,
    MorphRaga(String, u32),  // target, morph_blocks
}
```

The Hosono Test sequence:
```
 0s  SetRaga("bhairav"), SetGravity(0.8), SpawnDrone, SetTempo(72)
     SetEuclideanHits(5)
     // Locked Bhairav: clear scale, steady tala, defined groove

30s  SetGravity(0.5)
     // Loosening: pitches start to bend between degrees

40s  SetGravity(0.2), SetEuclideanHits(2)
     // Dissolving: mostly free pitch, sparse triggers

50s  SetGravity(0.05)
     // Pure texture: microtonal drift, no recognizable scale

60s  MorphRaga("yaman", 120), SetGravity(0.3)
     // Beginning to coalesce: new gravity weights fading in

70s  SetGravity(0.6), SetEuclideanHits(4)
     // Reforming: Yaman intervals emerging, rhythm returning

80s  SetGravity(0.8), SetTempo(90)
     // Locked Yaman: bright evening raga, faster tempo

90s  — end of test —
```

### 10.2 Run and record

Use the existing `Recorder` from 0.5 to capture:
- Visual frames (SDF blob renderer output)
- Audio (via cpal; add WAV file writer or use system capture)
- State log: gravity, raga, tempo, voice count per frame

### 10.3 Evaluation criteria

**Pass/Fail (automated checks):**
- [ ] No audio underruns during 90s test
- [ ] No crashes or panics
- [ ] Voice count stays ≤ max_voices
- [ ] Gravity values change at scheduled times
- [ ] Raga morph completes without error
- [ ] Frame rate stays above 30fps throughout

**Subjective (human evaluation):**
- [ ] 0-30s: Can you identify it as Bhairav? (komal Re/Dha audible)
- [ ] 40-50s: Does the dissolution sound intentional, not broken?
- [ ] 50-60s: Does it feel like ambient texture, not noise?
- [ ] 60-80s: Can you hear Yaman emerging? (teevra Ma audible)
- [ ] 80-90s: Is the groove re-established?
- [ ] Overall: Does the system sound like it *meant* to do that?

### 10.4 Edge cases to verify

- Rapid raga switching (< 1 second): morph handles gracefully
- Gravity 0→1 snap: no audio discontinuities
- All voices killed then respawned: clean recovery
- Tempo change during active pattern: no timing glitches
- Running for 10+ minutes: stable, no memory growth

### 10.5 Performance profiling

- Audio thread CPU usage
- Control thread (gravity/affinity) CPU usage
- GPU frame time
- Memory allocation rate (should be ~0 in steady state)

### 10.6 Document findings

Write `spec/RESULTS.md` with:
- What worked
- What needs tuning (gravity curve exponent, morph speed, etc.)
- Subjective audio quality notes
- Performance numbers
- Screenshots/waveform captures of key moments
- List of issues for future sessions

## Files Created

```
src/automation.rs        — AutomationStep, AutoAction, test sequence
spec/RESULTS.md          — filled in after running the test
```

## Files Modified

```
src/app.rs               — automation runner, triggered by hotkey
```

## Verification

The test itself IS the verification. Run the 90-second Hosono
sequence and evaluate against the criteria above.

## What Success Looks Like

You press a key. The system starts playing Bhairav: you can hear
the komal Re, the strong Ma, the steady 16-beat pulse. Over 30
seconds, the pitch starts to wander. The rhythm thins. By a minute
in, it's pure microtonal shimmer — ambient, floating, no scale.
Then new intervals appear: brighter, more open. The teevra Ma of
Yaman. The rhythm firms up. By 90 seconds you're in a different
raga, a different mood, and the transition felt like the system
exhaled and inhaled.

The blobs match: sharp-edged and pulsing during locked raga,
soft-edged and glowing during texture, reforming as the new
raga takes hold.

That's the Hosono Test.
