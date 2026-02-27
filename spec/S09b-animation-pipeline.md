# S09b — Animation Pipeline

**Layer**: L4-L5 (Rendering + Organism Sim)
**Status**: Active
**Dependencies**: S09 (visual outputs), S12 (cell-dna), S13 (first organisms)

## Overview

This spec documents the animation pipeline end-to-end: how organism visual state is driven by DNA, physics, and emotion. It supersedes S09's SDF smin rendering description with the additive potential field approach.

## Architecture Decisions

### AD-1: Per-organism emotion is the single source for visual parameters

`OrganismState.arousal` and `OrganismState.valence` are the authoritative source for GPU visual params (`thermal_temp`, `hue_shift`, `glow`). The global `aggregate_emotion` is used only for `GravityState` (pitch/rhythm gravity), not per-organism rendering.

### AD-2: Emotion bridge runs in the app update loop

After `reactor.tick()`, the app bridges reactor emotion to visual state:

```
reactor.graph.emotions[module_id] → OrganismState.arousal/valence
```

This keeps `ModuleCore` headless-safe (S01 contract). The app is the integration layer.

### AD-3: DNA provides initial emotion; reactor overrides continuously

At spawn: `org.arousal = dna.emotion.base_arousal`. Each frame: reactor emotion smoothly overrides via exponential smoothing (alpha = dt * 3.0, ~0.3s time constant). DNA defaults persist until the reactor's Hebbian learning produces meaningful emotion.

### AD-4: Sonar infrastructure provides periodic neighbor detection

A `Sonar` utility (`src/organism/sonar.rs`) pings at ~7 Hz, detecting all organism pairs within `max_range` (400px). Detection generates a gentle "curiosity" attraction pulling organisms toward each other at macro scale. DNA interaction rules (repel, bounce, attach) fire at their natural short ranges (20-60px) when organisms are close enough.

DNA interaction ranges stay in body-relative units — no pixel inflation. Sonar provides viewport-aware awareness as infrastructure. Organisms approach via curiosity, then interact via DNA rules, creating orbit/equilibrium dynamics (e.g., TBLK approaches, repels at 25px, settles into dynamic orbit).

Sonar is infrastructure: static ping rate, not organism-bound. Lives at the registry level.

### AD-5: Additive potential fields replace SDF smin

The shader uses `potential = r / max(dist, 0.5)` per lobe, summed globally. A threshold (`cross_smin_k = 1.5`) determines the blob boundary. Color is potential-weighted average of organism colors. This replaces S09's SDF `smin()` approach for cross-organism blending.

## Rendering Model

### Additive Potential Fields (Shadertoy Approach)

Each lobe emits a scalar potential field:

```
potential(pixel, lobe) = lobe.radius / max(distance(pixel, lobe.center), 0.5)
```

All lobe potentials from all organisms sum into a global field. The blob boundary is where `total_field >= threshold` (1.5). Color at each pixel is the potential-weighted average of contributing organism colors.

This naturally produces merging when organisms overlap — no explicit cross-organism logic needed.

### Fragment Shader Pipeline

```
1. evalField(pixel) → FieldHit { total, weighted_color, glow }
   - For each organism:
     - Compute org_color (thermal_palette + hue_rotate + clamp)
     - Apply beat pulse: r *= 1.0 + sin(pulse_phase * TAU) * pulse_amp * 0.3
     - Sum lobe potentials
     - Accumulate weighted color
2. Early-out if total < threshold * 0.3
3. body_color = weighted_color / total  (normalize weighted average)
4. fill = smoothstep(threshold - edge_width, threshold, total)
5. glow_halo in transition zone
6. Composite: background → glow → body fill
```

## Data Flow

### Spawn (Frame 0)

```
OrganismDna
  ├─ body → OrganismState (lobe_count, core_radius, pseudopod_gain, speeds)
  ├─ render → OrganismState (base_hue, base_glow, pulse_response, smin_k, edge_softness)
  ├─ physics → OrganismState (mass, drag, max_speed, interaction_rules)
  └─ emotion → OrganismState (arousal = base_arousal, valence = base_valence)
```

### Per-Frame Update

```
1. reactor.tick(dt)
   └─ Updates reactor.graph.emotions[module_id] via Hebbian learning

2. Emotion bridge (app.rs)
   └─ For each OrganismModule:
      org.arousal += (emotion.arousal - org.arousal) * min(dt * 3.0, 1.0)
      org.valence += (emotion.valence - org.valence) * min(dt * 3.0, 1.0)

3. organism_registry.tick(dt)
   ├─ apply_boundary_forces()
   ├─ sonar.tick(dt) → curiosity forces (macro-scale attraction)
   ├─ apply_interactions(dt)  // DNA ranges, no scaling
   └─ For each organism:
       ├─ energy = lerp(energy, 0.3 + arousal * 0.7, dt * 2.0)
       ├─ velocity += forces; velocity *= drag
       ├─ heading = atan2(vy, vx) or wander at 0.5 rad/s if slow
       ├─ update_lobe_targets() based on heading + energy
       └─ lobes lerp toward targets at extension/retraction speed

4. build_gpu_payload(beat_phase)
   └─ For each organism:
       thermal_temp = org.arousal           // [0,1] → thermal palette
       hue_shift   = base_hue + valence*0.1 // species hue + mood shift
       glow        = base_glow + arousal*0.5 // breathing halo
       pulse_phase = beat_phase              // global beat sync
       pulse_amp   = pulse_response          // per-species beat sensitivity

5. GPU shader evaluates additive potential field
```

## Animation Parameter Table

| Visual Effect | GPU Field | Source | Driven By |
|---|---|---|---|
| Color temperature | thermal_temp | org.arousal | Reactor emotion (Hebbian) |
| Species hue | hue_shift | base_hue + valence*0.1 | DNA + reactor emotion |
| Glow halo | glow | base_glow + arousal*0.5 | DNA + reactor emotion |
| Beat breathing | pulse_phase, pulse_amp | beat_phase, pulse_response | Global beat + DNA sensitivity |
| Blob shape | lobe offsets/radii | LobeState targets | heading + energy (from arousal) |
| Pseudopod extension | pseudopod_gain * energy | energy tracks arousal | Reactor emotion → energy |
| Heading direction | heading | velocity or wander | Interaction forces / idle wander |
| Position | pos | velocity integration | Interaction forces + drag + boundary |

## Organism Visual Identity

| Species | base_arousal | base_valence | base_hue | Visual |
|---|---|---|---|---|
| TBLK | 0.7 | -0.2 | 0.05 | Hot, orange/yellow, sharp transients |
| DRON | 0.3 | 0.3 | 0.6 | Cool, blue/cyan, diffuse glow |
| MELO | 0.5 | 0.1 | 0.35 | Mid, green/yellow, darting |

## Sonar + Interaction Model

Sonar (infrastructure, 400px max_range, ~7 Hz ping) provides macro-scale curiosity attraction.
DNA interaction rules provide micro-scale specific behavior at natural body-unit ranges.

| Layer | Range | Force | Purpose |
|---|---|---|---|
| Sonar curiosity | 0-400px | Gentle mutual attraction (8.0 strength, quadratic falloff) | Bring organisms into proximity |
| TBLK repel * | 0-25px | Push all away | Close-range separation |
| DRON slow * | 0-40px | Viscous drag | Deceleration zone |
| MELO bounce * | 0-20px | Elastic collision | Sharp deflection |
| MELO attach dron | 0-60px | Spring toward rest_length | Tethering |

Emergent behavior: organisms approach via curiosity, then DNA rules create equilibrium orbits, sticky zones, or tether bonds.

## Verification

- [ ] Three organisms have visually distinct colors from frame 1
- [ ] Overlapping organisms blend colors (no black)
- [ ] TBLK repels nearby organisms visibly
- [ ] MELO tethers toward DRON at ~240px range
- [ ] Pseudopods animate with heading changes
- [ ] Beat pulse visibly breathes TBLK (highest pulse_response)
- [ ] 60fps with 3 organisms
