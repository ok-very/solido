# S42 — RECH Organism + SampleRegistry

**Status:** Active
**Session:** S42
**Depends on:** S12 (cell DNA), S14 (wiring), S31 (transport), S33 (scale/rhythm bridge), S41 (raga activation), Generative reactor (walk_cell, chaos pipeline)

## Goal

Ship the RECH organism — Steve Reich-inspired phase-shifting mallet percussion using walk-cell harmonic discovery — plus the SampleRegistry infrastructure for shared sample buffer loading.

## Species Identity

- **Code:** `RECH`, **Name:** `rech-eighteen`
- **Seed:** `197618`, **Root pitch class:** 0 (C)
- **Generative identity:** Three mallet voices (marimba, bells, xylophone) at slightly different tempo ratios create gradual phase drift. Walk-cell discovers harmonic centers via scale gravity, modulating all voices. The 11-chord-like harmonic progression emerges from gravity well navigation rather than pre-programmed sequence.

## Architecture

### SampleRegistry

New module `src/samples/mod.rs` — global sample cache with URI-based resolution.

- **URI scheme:** `{collection}:{instrument}:{note}:{dynamic}` (e.g., `uiowa:marimba:c4:mf`)
- **Cache:** `HashMap<String, Arc<Vec<f32>>>` — shared across organisms via Arc
- **Integration:** `SampleCell::new_with_registry()` resolves `sample_uri` string param via registry
- **SampleData enum:** `Owned(Vec<f32>)` for legacy path loading, `Shared(Arc<Vec<f32>>)` for registry — tick code unchanged via Index trait

### Sample Pipeline

Python tooling (`tools/samples/fetch_uiowa.py`) downloads UIowa MIS samples from Wayback Machine archive:
- Source: `https://web.archive.org/web/20260209041538/https://theremin.music.uiowa.edu/`
- Processing: AIFF → WAV 48kHz mono, silence removal, peak normalization, 3s trim
- Manifest: `tools/samples/uiowa_manifest.json` maps instruments to Wayback URLs

### Cell Architecture (10 cells)

| Idx | Type | Role |
|-----|------|------|
| 0 | walk_cell | Chord discovery (harmonic walker, gravity-driven) |
| 1 | seq_cell | Voice A (marimba pattern, ratio=1.0) |
| 2 | seq_cell | Voice B (bells pattern, ratio=1.003) |
| 3 | seq_cell | Voice C (xylo pattern, ratio=0.997) |
| 4 | sample_cell | Marimba (uiowa:marimba:c4:mf) |
| 5 | sample_cell | Bells (uiowa:bells:c6:mf) |
| 6 | sample_cell | Xylophone (uiowa:xylophone:c5:mf) |
| 7 | logic_seq_cell | Additive density gate (euclidean) |
| 8 | filter_cell | Resonant body / spectral shaping |
| 9 | mixer_cell | Terminal bus |

### Wiring (10 wires)

- Walk → sample tune (Replace mode, gains 1.0/2.0/1.5 for octave/fifth offsets)
- Seq → sample triggers
- Samples → filter → mixer audio chain

### Phase Drift

Voices run at tempo_ratios 1.0, 1.003, and 0.997. At 130 BPM, this creates approximately:
- Full phase cycle between A and B: ~333 beats (~2.5 minutes)
- Full phase cycle between A and C: ~333 beats (~2.5 minutes)
- Rhythmic alignment events occur periodically, creating emergent accent patterns

## Organism DSP Refactoring

### CombinedTuning extraction
`CombinedTuning` struct, `cents_distance()`, `quantize_to_tuning()`, and `quantize_to_scale_fast()` extracted to `src/dsp/combined_tuning.rs`.

### Test extraction
~1450 lines of tests extracted from `organism_dsp.rs` to `src/dsp/organism_dsp_tests.rs` via `#[cfg(test)] #[path]` include. Implementation file reduced from 2230 to ~700 lines.

## DNA Schema

See `assets/dna/rech-eighteen.json` for complete DNA.

Key personality traits:
- `fidelity: 0.7` — follows scale with room for chromatic excursion
- `scale_affinity: 0.9` — strongly attracted to harmonic structure
- `base_chaos: 0.02` — minimal baseline chaos (process-dominated, not random)
- `chaos_sensitivity: 0.15` — moderate arousal response

## Critical Files

| File | Action |
|------|--------|
| `src/samples/mod.rs` | **Created** — SampleRegistry with Arc cache, URI resolution |
| `src/dsp/combined_tuning.rs` | **Created** — extracted CombinedTuning + quantization |
| `src/dsp/organism_dsp_tests.rs` | **Created** — extracted ~1450 lines of tests |
| `src/dsp/cell/sample_cell.rs` | **Edited** — SampleData enum, new_with_registry() |
| `src/dsp/cell/mod.rs` | **Edited** — build_cell passes registry to sample_cell |
| `src/dsp/organism_dsp.rs` | **Edited** — from_dna accepts registry, CombinedTuning import |
| `src/app.rs` | **Edited** — holds SampleRegistry, passes to spawn |
| `tools/samples/fetch_uiowa.py` | **Created** — Wayback Machine fetcher |
| `tools/samples/uiowa_manifest.json` | **Created** — instrument URL mapping |
| `assets/dna/rech-eighteen.json` | **Created** — RECH organism DNA |

## Verification

1. `cargo test --no-default-features` — all tests pass (no regressions)
2. RECH DNA loads without error (10 cells, 10 wires)
3. SampleRegistry: URI resolution, cache hits, Arc sharing
4. Backward compat: existing organisms still load via sample_path
5. Phase drift: cells 1/2/3 have different tempo_ratios confirmed in DNA
