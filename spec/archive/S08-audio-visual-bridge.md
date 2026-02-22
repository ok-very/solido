# S08 — Audio ↔ Visual Bridge

> The blobs learn to show what they hear.

## Goal

Wire the gravity/affinity state into the existing SDF blob renderer
so the organisms visually respond to the audio system. Gravity
strength affects blob edge sharpness. Emotion drives thermal color.
Tala beat drives pulse. The system becomes audiovisual.

## Ancestry (MAKE A BABY)

The Max/MSP patch had `multiSlider` displays and color-coded GIFs
(blue, red, purple, yellow) to represent different voice groups.
We replace that with the thermal SDF shader from the emotive color
system plan: SDF depth → thermal palette, with arousal driving
overall temperature.

## Depends On

- S04 (PitchGravity state)
- S06 (TalaGrid beat events)
- S07 (AffinityGraph emotions)
- Existing organism_renderer.rs (wgpu pipeline)

## Tasks

### 8.1 Create `src/tuning/gravity_control.rs`

The emotion-to-gravity mapping from the microtonal plan:

```rust
pub struct GravityState {
    pub pitch_gravity: f32,
    pub rhythm_gravity: f32,
    pub gamaka_depth: f32,
    pub morph_speed: f32,
}

impl GravityState {
    pub fn from_emotion(emotion: &ModuleEmotion) -> Self {
        let base_gravity = emotion.valence * 0.5 + 0.5;
        let arousal_pull = emotion.arousal * 0.6;
        let pitch_gravity = (base_gravity - arousal_pull).clamp(0.0, 1.0);
        let rhythm_gravity = (base_gravity - arousal_pull * 0.5).clamp(0.0, 1.0);
        let gamaka_depth = emotion.arousal.clamp(0.0, 1.0);
        let morph_speed = (-emotion.valence * 0.5 + 0.5).clamp(0.1, 2.0);
        Self { pitch_gravity, rhythm_gravity, gamaka_depth, morph_speed }
    }
}
```

The texture ↔ music continuum:
| Emotion State | Gravity | Sound |
|---------------|---------|-------|
| Calm, positive | high | Locked raga, clean tala |
| Slightly aroused | medium | Subtle gamaka, slight swing |
| High arousal | low | Pitch drifts, rhythm dissolves |
| Panic | zero | Free spectral drone — pure texture |

### 8.2 Add thermal uniforms to the shader

Extend `Uniforms` in `organism_renderer.rs`:

```rust
pub struct Uniforms {
    // existing fields...
    pub viewport: [f32; 2],
    pub time: f32,
    pub organism_count: f32,
    pub dpr: f32,
    // new audio-driven fields:
    pub beat_phase: f32,       // 0.0–1.0 within current beat
    pub gravity_strength: f32, // overall pitch gravity
    pub arousal: f32,          // drives thermal temperature
    pub valence: f32,          // drives color hue shift
}
```

### 8.3 Modify organism.wgsl

Add to the fragment shader:
- **Beat pulse**: organism scale oscillates with `beat_phase`
  ```wgsl
  let pulse = 1.0 + sin(uniforms.beat_phase * 6.283) * 0.02 * uniforms.arousal;
  // Apply pulse to organism dimensions before SDF evaluation
  ```

- **Gravity → edge sharpness**: low gravity = softer SDF edges
  ```wgsl
  let edge_softness = mix(4.0, 0.5, uniforms.gravity_strength);
  // Use in smoothstep threshold for SDF boundary
  ```

- **Arousal → glow intensity**: high arousal = brighter glow halo
  ```wgsl
  let glow = exp(-max(field, 0.0) * 0.03) * (0.1 + uniforms.arousal * 0.3);
  ```

- **Valence → color temperature**: negative valence shifts cool,
  positive shifts warm
  ```wgsl
  let temp_bias = uniforms.valence * 0.15;
  ```

### 8.4 Consult smoothman

At this point, consult the smoothman agent for:
- Verifying the SDF edge softness parameter doesn't create artifacts
- Ensuring the beat pulse modulation on SDF dimensions is smooth
- Reviewing the thermal/glow additions to the existing shader

### 8.5 Feed gravity state into shader each frame

In `app.rs` update loop:
1. Compute `GravityState::from_emotion(...)` (or from manual sliders)
2. Read `TalaGrid.phase` for beat_phase
3. Pack into extended `Uniforms`
4. Pass to organism_renderer via existing paint callback

### 8.6 Audio analysis feedback (simple)

Lightweight audio analysis on the control thread:
- RMS level from the last rendered audio block
- Feed RMS as an additional arousal input
- This closes the minimal feedback loop: audio → analysis → blob visual

Implementation: add a second ring buffer from audio thread → main thread
carrying `AnalysisFrame { rms: f32, peak: f32 }`.

## Files Created

```
src/tuning/gravity_control.rs  — GravityState, from_emotion mapping
```

## Files Modified

```
src/tuning/mod.rs              — pub mod gravity_control;
src/renderer/organism_renderer.rs — extended Uniforms, shader changes
organism.wgsl                   — beat pulse, edge softness, glow, temp
src/audio/mod.rs               — analysis ring buffer (audio→main)
src/app.rs                     — gravity state → uniforms pipeline
```

## Verification

1. Blob edges soften when gravity drops (audible: pitch drifts free)
2. Blob edges sharpen when gravity rises (audible: pitch locks to scale)
3. Blobs pulse with the tala beat (visible rhythmic breathing)
4. High arousal → blobs glow brighter, warmer colors
5. Low valence → blobs shift toward cooler tones
6. RMS from audio analysis causes subtle blob intensity changes
7. No visual artifacts from the shader modifications
8. Performance: still 60fps with the additional uniforms

## The Moment

This is where the project becomes audiovisual. Before S08, you have
separate audio and visual systems. After S08, moving a gravity slider
simultaneously changes the pitch quantization you hear AND the visual
sharpness you see. The system becomes synesthetic.
