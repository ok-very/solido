# Audio-Rate Crossfeed Between Organisms

**Status**: Planned (future work)
**Depends on**: interaction_params DNA extension, proximity_energy

## Goal

Route a fraction of organism A's stereo audio output into organism B's DSP
input, creating acoustic coupling. Like plugging one synth's output into
another's audio input jack. When two organisms have strong affinity and are
close, their sounds bleed into each other -- a filter organism reshapes a
drone's timbre, a drum pattern modulates through an acid line's diode ladder.

## Architecture: Option A (Recommended)

Post-mix crossfeed matrix in audio callback. After all organisms tick, each
organism's output is scaled by the crossfeed coefficient and injected into a
per-organism `crossfeed_in: [f32; 2]` buffer. On the NEXT frame,
`OrganismDsp::tick()` reads `crossfeed_in` as additive input summed into the
first cell's audio (topological root). One-frame latency (22us at 44.1kHz)
is inaudible.

Pros: Minimal change to OrganismDsp. Mirrors reverb/tape bus send pattern.
Coefficients are Shared handles -- no new sync primitives.

### Coefficient Computation (control thread, per-frame)

```
coeff(A->B) = affinity_weight(A,B) * proximity_factor(A,B) * species_scale
```

- `affinity_weight`: from AffinityGraph edge weight [0,1] (strongest edge)
- `proximity_factor`: `1.0 - (distance / crossfeed_range).clamp(0,1)`
- `species_scale`: per-species DNA param. DRON=0.3, ACID=0.6, SPGL=0.1,
  HOSO=0.5, TBLK=0.2, KKIT=0.0 (drums reject external audio bleed)
- Final coefficient hard-clamped to [0.0, 0.4]

## RT Safety

- Coefficients: one `Shared` (atomic f32) per active pair. Flat `Vec<Shared>`
  indexed by `(src_idx * N + dst_idx)`. Pre-allocated to MAX_CHANNELS^2 (256).
- No allocation on audio thread. Matrix is fixed-size.
- Only active pairs (coefficient > 0.001) iterated via sparse index.
- Sparse index rebuilt on control thread, swapped via SPSC channel.

## Feedback Loop Mitigation

Bidirectional crossfeed (A->B and B->A) creates a delay-line feedback loop.
Three layered defenses:

1. **Coefficient ceiling**: Max 0.4 per pair. Round-trip gain A->B->A =
   0.4 * 0.4 = 0.16, well below unity. Stable by construction.
2. **DC blocker on injection**: 1-pole highpass (20 Hz) on each organism's
   crossfeed input. Prevents DC buildup from asymmetric saturation.
3. **Energy gate**: If crossfeed input exceeds 2x organism's own RMS
   (measured over 512 samples), coefficient is momentarily halved.

## DNA Schema

```json
"crossfeed": {
  "susceptibility": 0.5,
  "range": 200.0,
  "max_coefficient": 0.3
}
```

Optional on OrganismDna. Defaults: susceptibility=0.0, range=200, max=0.3.

## CPU Budget

Full N x N at N=6 is 36 multiplies per sample -- negligible. At N=16
(MAX_CHANNELS), 256 multiplies still under 1% of a core at 44.1kHz.
Sparse approach (4-8 active pairs typical) is preferred regardless.

## Critical Files

| File | Change |
|------|--------|
| `src/substrate/audio.rs` | Crossfeed matrix multiply in callback |
| `src/dsp/organism_dsp.rs` | `crossfeed_in: [f32; 2]` injection field |
| `src/organism/dna.rs` | `CrossfeedDna` struct |
| `src/affinity/graph.rs` | Source of edge weights for coefficients |
| `src/app.rs` | Coefficient computation in control loop |
