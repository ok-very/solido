# Organism Ecology & Identity Reference

Status: **In Progress** — reviewing organisms one at a time via `/org-review`

## Overview

Comprehensive organism review covering ALL systems: sonic character, substrate ecology, chladni node personality, physics/movement, interaction forces, merge outcomes, well behavior, CV exchange, and visual identity. Each organism reviewed as a whole — not just its waste profile.

## System Summary

### Substrate Ecology
- Video frames → SubstrateGrid (rich RGB, "the sauce")
- Organisms deplete at position → pitch histogram → "what you eat IS your scale"
- Organisms deposit waste trails (complement of consumed hue, speed-proportional)
- Waste = mono-nutrient droppings, not rich like video

### Chladni Nodes
- Sub-nodes as mini gravity wells (120px range, bell-curve force)
- Visitors drain host node energy, gain energy themselves
- DNA controls: drain_rate (host generosity), absorption_rate (visitor aggression), regen_rate (recovery)
- Dormant nodes (energy < 0.1) → cooldown 180 ticks → reactivate at 0.3

### Sonic Exchange
- CV patch exports (rms, seq_chaos, melodic_contour, logic_density) → AffinityGraph → imports
- SetScaleWeights: pitch_histogram × (scale_affinity × fidelity) → audio-thread quantization
- Histogram consonance (cosine similarity of consumed pitches) → valence reward

### Physics
- 120Hz fixed substep, exponential drag, proximity viscosity
- 7 interaction modes: Repel, Bounce, Slow, Attach, Glob, Orbit, IntegratePropose
- Sonar detection (400px, 7Hz), curiosity pull toward active neighbors
- Stasis burst after 0.8s at <10 px/s

### Merge
- IntegratePropose: affinity>0.5 + mutual consent + 5s dwell → fusion
- Energy-weighted blend of all continuous params, mass additive
- Gene code: 2-char prefix from each parent (alphabetical)
- Node wells: union + dedup at 20px + cap at 12

## Constants

| Constant | Value | System |
|----------|-------|--------|
| Deposit formula | (speed/max_speed) × consumed × 0.005 | Ecology |
| Waste color | complement of consumed hue (HSV +180°, S=0.7, V=0.5) | Ecology |
| Substrate block size | 16px | Grid |
| Video refresh rate | 0.02/frame | Grid |
| Node well range | 120px | Chladni |
| Node well strength | 4.0 | Chladni |
| Dormant threshold | energy < 0.1 | Chladni |
| Dormant cooldown | 180 ticks (~1.5s) | Chladni |
| Harmonic awareness | 600px | Consonance |
| Consonance threshold | 0.5 | Consonance |
| Sonar range | 400px, 7Hz pings | Physics |
| Merge dwell | 5.0s | Union |
| Merge consent | desire>0.7 AND valence>0.2 | Union |

---

## Reviewed Organisms

### DRON (Ambient Drone)
- **Reviewed**: 2026-03-22
- **Sonic**: Fixed dead export (drone_pitch → cell4.freq instead of seq_chaos). Scale affinity raised 0.3→0.5 (blend now 0.15).
- **Ecology**: Heavy depositor (×2 multiplier). Pure complement waste. Slow browser grazing style.
- **Physics**: Added Slow rule (* at 120px, strength 3.0) — viscous damping field. Desire raised 0.15→0.35.
- **Visual**: RD kept at 0.3 (faint bands). Well response increased via scale_affinity 0.5.
- **Chladni**: Kept at 2 nodes (ellipse). Generous host (drain 0.004, regen 0.020).
- **Role**: Ambient bed that creates a viscous damping field. Heavy trail depositor for slow recycling. Gentle lens follower. Possible (but unlikely) merge candidate at desire 0.35.

### HOSO (Clinical Sequencer)
- **Reviewed**: not yet

### SPGL (Ambient Generative)
- **Reviewed**: not yet

### ACID (TB-303 Bass)
- **Reviewed**: not yet

### TBLK (Tabla Rhythm)
- **Reviewed**: not yet

### KKIT (TR-909 Drum Kit)
- **Reviewed**: not yet

### ISAO (Melodic Explorer)
- **Reviewed**: not yet

### RECH (High-Affinity Synth)
- **Reviewed**: not yet

---

## DNA Quick Reference

### Substrate & Feeding

| Species | Absorb | Drain | Regen | Sight | Sensitivity | Speed | Mass | Drag |
|---------|--------|-------|-------|-------|-------------|-------|------|------|
| DRON | 0.002 | 0.004 | 0.020 | 6 | 0.3 | 80 | 1.5 | 0.98 |
| HOSO | 0.004 | 0.006 | 0.015 | 4 | 0.7 | 60 | 1.2 | 0.93 |
| SPGL | 0.003 | 0.003 | 0.025 | 8 | 0.2 | 40 | 2.0 | 0.98 |
| ACID | 0.008 | 0.010 | 0.008 | 3 | 0.9 | 100 | 1.0 | 0.88 |
| TBLK | 0.005 | 0.005 | 0.015 | 3 | 0.8 | 120 | 1.0 | 0.91 |
| KKIT | 0.007 | 0.008 | 0.010 | 2 | 0.9 | 80 | 1.0 | 0.95 |
| ISAO | 0.006 | 0.007 | 0.012 | 4 | 0.6 | 70 | 1.2 | 0.91 |
| RECH | 0.005 | 0.006 | 0.014 | 5 | 0.5 | 90 | 1.1 | 0.92 |

### Personality

| Species | Fidelity | Scale Aff | Rhythm Aff | Chaos | Desire | Valence | Arousal |
|---------|----------|-----------|------------|-------|--------|---------|---------|
| DRON | 0.3 | 0.3 | 0.1 | 0.0 | 0.15 | 0.3 | 0.3 |
| HOSO | 0.9 | 0.8 | 0.7 | 0.05 | 0.70 | 0.1 | 0.6 |
| SPGL | 0.1 | 0.9 | 0.3 | 0.0 | 0.10 | 0.5 | 0.3 |
| ACID | 0.8 | 0.7 | 0.8 | 0.15 | 0.50 | 0.2 | 0.8 |
| TBLK | 0.5 | 0.2 | 0.6 | 0.0 | 0.40 | 0.1 | 0.6 |
| KKIT | 0.95 | 0.0 | 0.9 | 0.0 | 0.60 | 0.3 | 0.7 |
| ISAO | 0.8 | 0.8 | 0.5 | 0.08 | 0.70 | 0.4 | 0.4 |
| RECH | 0.7 | 0.9 | 0.85 | 0.02 | 0.60 | 0.4 | 0.5 |

### Visual (RD + Body)

| Species | RD React | Feed | Kill | RD Scale | Hue | Harmonics | Chladni m/n |
|---------|----------|------|------|----------|-----|-----------|-------------|
| DRON | 0.3 | 0.025 | 0.060 | 2.5 | — | 8 | 2/1 |
| HOSO | 0.6 | 0.035 | 0.065 | 2.0 | — | 6 | — |
| SPGL | 0.2 | 0.035 | 0.065 | 2.0 | — | 8 | — |
| ACID | 0.9 | 0.035 | 0.065 | 1.5 | — | 6 | — |
| TBLK | 0.5 | — | — | — | — | 4 | — |
| KKIT | 0.4 | — | — | — | — | 4 | — |
| ISAO | 0.6 | — | — | — | — | — | — |
| RECH | 0.3 | — | — | — | — | — | — |

### Sonic Exchange

| Species | Exports | Imports From | Scale Blend |
|---------|---------|--------------|-------------|
| DRON | rms, seq_chaos | acid,hoso,spgl → filter | 0.09 |
| HOSO | rms, seq_chaos, contour | * → filter, chaos | 0.72 |
| SPGL | rms | acid,hoso,dron → filter | 0.09 |
| ACID | rms, seq_chaos, contour | * → filter, chaos | 0.56 |
| TBLK | rhythm_density, logic_density | (none) | 0.10 |
| KKIT | rhythm_density, logic_density | (none) | 0.00 |
| ISAO | rms, seq_chaos, contour | * → filter, chaos | 0.64 |
| RECH | rms, seq_chaos, contour, logic_density | * → filter, chaos | 0.63 |

---

## Ecology Matrix (filled as reviews complete)

| Producer | Waste Color | Consumers | Deposit Style |
|----------|------------|-----------|---------------|
| DRON | complement (pure) | recyclers (slow) | heavy (×2) |
| HOSO | TBD | TBD | TBD |
| SPGL | TBD | TBD | TBD |
| ACID | TBD | TBD | TBD |
| TBLK | TBD | TBD | TBD |
| KKIT | TBD | TBD | TBD |
| ISAO | TBD | TBD | TBD |
| RECH | TBD | TBD | TBD |

## Decision Log

*(Filled per review. Format: organism, question#, decision, rationale.)*
