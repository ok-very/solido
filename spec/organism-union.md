# Organism Union — Cell Combination & Species Genesis

**Status**: Spec (blocked by pre-union-iteration)
**Depends on**: pre-union-iteration (all 4 test environments pass interaction standard), S32, S33, S35, S38, S39, S40, S41, SAT — all complete Mar 2026
**Priority**: After pre-union iteration loop completes

---

## Goal

When two organisms reach maximum mutual affinity and sustained proximity, they **fuse into a new organism** whose DSP chain is a meaningful combination of both parents' cells. The result is a new species with its own identity, pruned of redundancies, producing novel sound from the merged topology. This is the reproductive act of the system — the creation of genuinely new music from interaction.

---

## Current State (what exists)

- `execute_fusion()` in `registry.rs`: spawns a new OrganismState at centroid, area-conserving radius, energy-weighted parameter merge. **Visual/physics only — no audio DNA merging.**
- `check_integrations()`: scans pairs for dwell > 5s + mutual consent.
- `consents_to_integrate()`: dynamic — `desire_to_connect > 0.7 && valence > 0.2`.
- Glob state: visual merging of SDF territories when affinity > threshold. **This is the current ceiling and is good enough for now.**

**What's missing**: The actual combination of cells, wires, and musical identity.

---

## 1. Gene Code & Naming

Each organism species has a 4-letter code: DRON, HOSO, ACID, SPGL, TBLK, KKIT.

**Union naming rule**: Take first 2 letters of each parent, alphabetical order.

| Parent A | Parent B | Child Code | Example Name |
|----------|----------|------------|--------------|
| ACID     | DRON     | ACDR       | acdr-001     |
| DRON     | HOSO     | DRHO       | drho-001     |
| ACID     | HOSO     | ACHO       | acho-001     |
| TBLK     | KKIT     | KKtb       | kktb-001     |

- Suffix is a monotonic counter (001, 002, ...) per session.
- The gene code is stored as `species` on OrganismState.
- If a hybrid fuses with another organism, the same rule applies recursively: take first 2 letters of each parent's 4-letter code, alphabetical.
- Second-generation example: ACDR + DRHO → ACDR (alphabetical first 2 of each: AC from ACDR, DR from DRHO).

---

## 2. Cell Combination Rules

The union organism's DSP chain is built from both parents' cells. This is **not** a naive concatenation — it requires pruning and topology merging.

### 2a. Redundancy Pruning

Cells are categorized by function:

| Role | Cell Types | Pruning Rule |
|------|-----------|--------------|
| **Oscillator** | osc_cell, saw_bank_cell | Keep both — different timbres create richness |
| **Filter** | filter_cell, diode_filter_cell | Keep both — serial or parallel filter topology |
| **Mixer** | mixer_cell | Merge into one terminal mixer — never duplicate |
| **Sequencer** | seq_cell, logic_seq_cell | Keep both — polyrhythmic interaction is the point |
| **Envelope** | env_cell, accent_env_cell | Keep one per voice path — prune identical duplicates |
| **LFO/Modulator** | lfo_cell, slew_cell, func_gen_cell | Keep if target exists, prune orphaned modulators |
| **Percussion** | strike_voice_cell, noise_burst_cell, drum_voice_cell | Keep all — each is a unique voice |
| **Sample** | sample_cell | Keep all — each references distinct samples via SampleRegistry URI |
| **Walker** | walk_cell, melodic_cell | Keep both — different walk trajectories / rhythm patterns produce distinct contours |
| **Interface** | xy_pad_cell | Keep at most one — human controller, not duplicatable |
| **Call-Response** | call_response_cell | Keep one — phrase bank is per-organism identity; pick higher-generation parent's bank |

**Pruning algorithm**:
1. Union all cells from both parents into a candidate pool
2. Merge mixer_cells into a single terminal mixer
3. Remove duplicate modulators targeting the same parameter on the same cell
4. Remove orphaned cells (no audio path to terminal mixer after wire merge)
5. Topological sort the result; if cycles, break the weakest-gain wire

### 2b. Wire Merging

- Audio wires from both parents are kept verbatim (cell indices remapped to unified pool).
- Cross-parent wires are **not auto-generated** — they emerge from the affinity graph over time as the new organism's internal Hebbian learning discovers productive connections.
- Trigger wires: sequencers from parent A can trigger voices from parent B only if an explicit trigger wire is created (future: affinity-driven trigger routing).
- Modulation wires: kept per-parent. Cross-parent modulation is a later evolution.

### 2c. Parameter Inheritance

For cells that appear in both parents (e.g., both have an osc_cell):
- Each keeps its own parameters — they are distinct cells in the new chain.
- The new organism's render params (hue, glow, pulse_response) are energy-weighted averages of parents.

---

## 3. Musical Evolution Through Union

Union should produce **audible change** — not just visual merging. The child organism sounds different from either parent.

### 3a. Immediate Changes (at fusion moment)

- New cell topology → new signal routing → new timbre
- Combined sequencers create polyrhythmic patterns neither parent had alone
- Filter topology changes (serial vs parallel) alter spectral character

### 3b. Raga/Scale Progression (future, requires L3 Processing)

- If either parent has raga affinity tags, the child inherits the union of both tag sets.
- Quantizer modules (L3) respond to new affinity tags → scale selection shifts.
- Example: DRON (Bhairav raga) + HOSO (Malabar) → child organism's quantizer receives both gravity weight sets → creates a hybrid scale that drifts between parents.
- This is the musical "genetics" — ragas combine like alleles.

### 3c. Long-Term Adaptation (post-union Hebbian learning)

- The child organism's internal affinity graph starts fresh (no inherited edge weights).
- Over time, productive internal connections strengthen → the organism "learns" its own voice.
- Cross-parent cell connections that produce consonance/rhythm get reinforced.
- Unproductive connections prune → the organism simplifies itself over generations.

---

## 4. Baseline Ruleset

### 4a. When Union Triggers

| Condition | Threshold | Notes |
|-----------|-----------|-------|
| Pairwise affinity | > 0.5 | Sustained, not spike |
| Mutual consent | Both `desire_to_connect > 0.7` AND `valence > 0.2` | Emotional readiness |
| Dwell time | > 5.0 seconds | Must maintain proximity continuously |
| Proximity | Within 500px center-to-center | Dwell resets on separation |
| Population cap | Total organisms < 12 | Prevent runaway reproduction |

### 4b. What Union Produces

| Property | Rule |
|----------|------|
| Position | Centroid of parents |
| Radius | Area-conserving: `sqrt(a² + b²)` |
| Mass | Additive: `a + b` |
| Species code | 4-letter gene code (section 1) |
| Name | `{gene_code}-{counter}` |
| Cells | Combined + pruned (section 2) |
| Wires | Per-parent preserved, single terminal mixer |
| Emotion | Energy-weighted average of valence/arousal |
| desire_to_connect | Reset to 0.1 (post-fusion refractory period) |
| Interaction rules | Union of both parents' rules, deduplicated |
| Affinity tags | Union of both tag sets |
| Reverb/delay sends | Energy-weighted average |
| Fidelity | Average of parents |

### 4c. Post-Union Behavior

- **Refractory period**: `desire_to_connect` starts at 0.1 and must re-adapt upward. Prevents immediate re-fusion.
- **Parent despawn**: Both parents removed from simulation.
- **Ledger event**: Record `(parent_a, parent_b, child_id, child_gene_code, timestamp)` for explainability.
- **Audio crossfade**: 0.5s linear crossfade from parents' mixed output to child output. Prevents audio pop.

---

## 5. UI Requirements

### 5a. Organism Inspector (post-union)

The existing organism panel needs to display the combined cell chain:

- **Cell list**: Show all cells with provenance indicator (parent A color / parent B color / shared)
- **Wire diagram**: Visual graph of cell→cell connections, color-coded by parent origin
- **Gene code badge**: Prominent display of 4-letter species code
- **Lineage tree**: Simple parent₁ + parent₂ → child display
- **Audio meters**: Per-cell audio energy, same as current but with more cells

### 5b. Glob State UI (current priority)

Before union, the glob state should be visible:

- **Affinity readout**: Show pairwise affinity value between globbed organisms
- **Desire indicators**: Small visual cue on each organism showing desire_to_connect level
- **Dwell progress**: When dwell timer is active, show progress toward fusion threshold

---

## 6. Implementation Phases

### Phase 1 — Glob State Polish (current)
- Tune affinity thresholds, attraction strength, visual merging
- Add debug overlay for affinity values
- No code changes to union/fusion

### Phase 2 — Gene Code & Identity
- Implement naming system (4-letter code + counter)
- Store lineage on OrganismState
- Ledger recording of fusion events

### Phase 3 — Cell Combination Engine
- `combine_dna(a: &OrganismDna, b: &OrganismDna) -> OrganismDna`
- Redundancy pruning algorithm
- Wire remapping
- New OrganismDsp from combined DNA

### Phase 4 — Audio Crossfade
- Temporary dual-output during crossfade window
- VoiceBus channel handoff

### Phase 5 — UI
- Cell provenance display
- Lineage tree
- Glob state indicators

### Phase 6 — Musical Genetics (requires L3)
- Raga tag inheritance
- Gravity weight blending
- Scale morph on fusion

---

## Critical Files

| File | Changes |
|------|---------|
| `src/organism/dna.rs` | `combine_dna()`, gene code generation, lineage fields |
| `src/organism/registry.rs` | Replace `execute_fusion()` with cell-combination version |
| `src/organism/sim.rs` | Lineage tracking, refractory period |
| `src/dsp/organism_dsp.rs` | Build OrganismDsp from combined DNA |
| `src/ui/panels/organism_panel.rs` | Cell provenance display, lineage tree |
| `src/app.rs` | Audio crossfade, VoiceBus channel handoff |
| `src/substrate/audio.rs` | Dual-output during crossfade |

---

## Resolved Design Questions

1. **Population pressure**: Fusion allowed when organism count >= 3 (prevents extinction). Below 3, blocked.
2. **Cell count ceiling**: 16 cells max. If combined pool exceeds 16 after pruning, drop lowest-gain cells.
3. **Symmetric vs asymmetric fusion**: 50/50 cell contribution. Both parents contribute equally — the pruning algorithm handles redundancy, not favoritism.
4. **Death/splitting**: Irreversible with memory. Parent gene codes stored on child (`parent_codes` field on OrganismState). No split pathway.
5. **Third-generation naming**: For F2+, take first 2 letters of each parent's gene code, alphabetical. If collision, append generation counter: ACDR-G2-001.

---

## 7. Dual-Root Discovery

Hybrid organisms (created by fusion) inherit both parents' tonal roots. A consonance-weighted drift mechanism resolves this to a single root over ~30 seconds.

**Fields on OrganismState** (`src/organism/sim.rs`):
- `alt_root_pitch_class: Option<u8>` — secondary root from fusion parent (None for non-hybrids)
- `root_blend: f32` — blend weight [0,1] between primary (0.0) and alt (1.0)
- `root_blend_commit_timer: f32` — seconds past commit threshold

**Algorithm** (`OrganismRegistry::tick_dual_root`):
1. For each hybrid (alt_root_pitch_class.is_some()):
2. Compute aggregate consonance for primary vs alt root against nearby wells (within 600px), weighted by inverse distance
3. Uses `consonance_weight()` from `gravity_well.rs` — Tenney-height-based semitone table
4. Nudge blend: `blend += 0.01 * (consonance_alt - consonance_primary)` per frame
5. When blend > 0.8 or < 0.2 for 5+ seconds: commit winner as root, clear alt
6. During blend period, the gravity field uses whichever root blend is closer to (threshold 0.5)

---

## 8. Node Well Union

When organisms fuse, their node wells (Chladni sub-node mini gravity wells) merge.

**Algorithm** (`merge_node_wells` in `registry.rs`):
1. Union all wells from both parents
2. Deduplicate overlapping wells (within 20px center-to-center): keep higher-energy one
3. Cap at 12 wells max — prune lowest-energy wells

This preserves both parents' ecological footprint while preventing excessive node density.

---

## 9. Lineage Data Model

**Fields on OrganismState** (`src/organism/sim.rs`):
- `parent_codes: Option<(String, String)>` — parent species codes (None for base species)

Gene code generation: `gene_code_from_parents()` takes first 2 chars of each parent (alphabetical order).
Examples: ACID + DRON → ACDR, DRON + HOSO → DRHO.

The species code is stored as `species` on OrganismState and used for interaction rule matching.
