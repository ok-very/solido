# Substrate Paradigm — Spec Rewrite Dependency Graph

**Status**: Planning
**Context**: Video substrate changes the fundamental model — organisms metabolize what the substrate feeds them, rather than seeking preferred inputs. Several existing specs assume the old seek/satisfy model and need rewriting.

---

## Dependency Graph

```
video-substrate.md (Phase 2-5) ◄──────────────────────────┐
  │                                                         │
  ├─► substrate-encoding.md [NEW]                          │
  │     How pixel color maps to pitch/rhythm energy.        │
  │     Key change = re-encode substrate, not jolt graph.   │
  │     Transport controls (Circle of Fifths, raga chips)   │
  │     become substrate color transforms.                  │
  │     Depends on: video-substrate Phase 1 (done)          │
  │                                                         │
  ├─► block-grid-vision.md [NEW]                           │
  │     Retool CV features to sample from energy grid.      │
  │     Block triangulation replaces raw 160px CV.          │
  │     video_cv_cell reads grid cells, not global averages.│
  │     Depends on: energy_grid.rs (done), video-substrate  │
  │                                                         │
  ├─► S33-rewrite: Scale/Rhythm Bridge [REWRITE]           │
  │     gravity_weights → substrate pitch encoding          │
  │     beat_phase → substrate rhythm encoding              │
  │     Organisms receive, not seek.                        │
  │     Depends on: substrate-encoding.md                   │
  │                                                         │
  ├─► S40-rewrite: Harmonic Interaction [REWRITE]          │
  │     Consonance emerges from consumed substrate.         │
  │     Organisms build harmony from what they ate,         │
  │     not from matching preferences.                      │
  │     Depends on: substrate-encoding.md                   │
  │                                                         │
  ├─► S41-rewrite: Raga Activation [REWRITE]               │
  │     Microtonal overlay → substrate color mapping.       │
  │     Raga = a filter on how substrate encodes pitch.     │
  │     Depends on: substrate-encoding.md                   │
  │                                                         │
  ├─► well-lens.md [REWRITE from gravity_well]             │
  │     Wells focus substrate energy (convex lens UV warp). │
  │     Organisms follow food concentration, not harmony.   │
  │     Energy state machine stays (drain/regen).           │
  │     Depends on: video-substrate Phase 4                 │
  │                                                         │
  ├─► nav-reward-rewrite.md [REWRITE from S39]             │
  │     Reframe: substrate richness, not well consonance.   │
  │     Arrival reward = found rich substrate.              │
  │     Trapping penalty = local substrate depleted.        │
  │     Depends on: well-lens.md, block-grid-vision.md     │
  │                                                         │
  └─► organism-satisfaction.md [REWRITE from SAT]          │
        Satisfaction = processing quality, not input match. │
        Hebbian learning: good music from bad substrate     │
        = strengthen that metabolism pathway.               │
        Depends on: S33-rewrite, S40-rewrite               │
                                                            │
video-substrate.md Phase 6 (VLM) ◄─────────────────────────┘
  Semantic consciousness sits on top of all rewrites.
  VLM labels drive organism dictionary, evolving semiotics.
```

## Implementation Order

### Wave 1: Foundation (can start now)
1. **substrate-encoding.md** — how pixels become pitch/rhythm. This unblocks everything.
2. **block-grid-vision.md** — retool CV to grid sampling. Unblocks local sight.

### Wave 2: Organism rewrites (needs Wave 1 specs)
3. **S33-rewrite** — bridge receives substrate, not preferences
4. **S41-rewrite** — raga as substrate filter
5. **S40-rewrite** — consonance from consumption

### Wave 3: Environment rewrites (needs Wave 2)
6. **well-lens.md** — wells focus substrate
7. **nav-reward-rewrite.md** — richness, not consonance

### Wave 4: Learning rewrite (needs Wave 2+3)
8. **organism-satisfaction.md** — satisfaction from processing quality

### Wave 5: Consciousness (needs everything)
9. **video-substrate Phase 6** — VLM semantic layer

## What's NOT Changing
- Audio DSP path (cells, modulation wires, buses) — untouched
- Physical forces (drag, repulsion, attachment) — untouched
- Chaos noise field — untouched (still drives seq_cell mutation)
- Transport/GlobalClock — untouched
- UI panels (organism, synth_detail, MC20, groove) — mostly untouched
- video_cv_cell — stays, but rewired to read from grid instead of global broadcast

## What's Done (Ship It)
- ✅ Video decoder (ffmpeg-next, stride-aware, 480px)
- ✅ SubstrateGrid (CPU energy grid, block-based, deplete/replenish)
- ✅ GPU substrate texture (grid→RGBA8→composite shader)
- ✅ Gaussian bloom + glow (projected light through pixel blocks)
- ✅ Depletion visualization (alpha darkening)
- ✅ video_cv_cell (sensory organ, SetVideoFeatures command)
- ✅ Microtone ring on Circle of Fifths
- ✅ Tala enable/disable in groove panel
- ✅ Video perception panel (feature bars + status)
- ✅ video-substrate.md spec (Phases 1-6)
