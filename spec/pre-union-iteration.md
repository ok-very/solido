# Pre-Union Organism Iteration — Test Environments & Interaction Standard

**Status**: Spec (next — blocks organism-union Phase 1)
**Depends on**: UI-scales-transport, UI-tempo-groove, UI-inspector-nodegraph, S42 (RECH + SampleRegistry), SAWL (call-response), kill button fix (7939bbd)
**Blocks**: organism-union.md (all phases)
**Priority**: Immediate — must pass before union work begins

---

## Goal

Establish a repeatable observation methodology for tuning organism behavior. Four test environments exercise different interaction regimes (duo, tension, rhythm, full). Each environment is run through an iteration loop until all organisms meet a defined interaction standard. Fixes discovered during iteration are shipped incrementally. Union cannot begin until all four environments pass.

---

## Method: Iteration Loop

```
1. Launch environment (cargo run with env-specific DNA config)
2. Observe 90 seconds, hands-off (no user input after start)
3. Score against checklist (Section 3)
4. Note failures with timestamps + organism names
5. Fix code or DNA (smallest possible change)
6. Re-observe from step 1
7. Environment passes when all checklist items score YES for 3 consecutive runs
```

Each iteration is logged as a dated entry in `spec/iteration-log.md` with: environment name, pass/fail per checklist item, fix applied (if any). This creates a traceable quality record leading into union.

---

## 1. Test Environments

### ENV-1: Harmonic Duo (pairwise basics)

| Property | Value |
|----------|-------|
| **Organisms** | DRON (dron-alpha) + HOSO (hoso-malabar) |
| **Root pitch** | Both root=0 (C) — consonant pair |
| **BPM** | 90 |
| **Active DNA** | dron-alpha.json, hoso-malabar.json |
| **All others** | inactive |
| **Focus** | Pairwise attachment formation, glob state, basic force balance |
| **Why these** | Drone + melodic complement. Same root = should attract. Simplest case — if this doesn't work, nothing will. |

**Expected behavior**: DRON and HOSO should orbit, develop affinity over 30-60s, enter glob state, and maintain musical interaction (DRON's pitch walks toward HOSO's melody).

### ENV-2: Tension Trio (mixed roots, force competition)

| Property | Value |
|----------|-------|
| **Organisms** | ISAO (root=9/A) + SPGL (root=0/C) + ACID (root=9/A) |
| **Root pitch** | Mixed — ISAO+ACID consonant, SPGL dissonant with both |
| **BPM** | 120 |
| **Active DNA** | isao-tomita.json, spgl-kepler.json, acid-kinoko.json |
| **All others** | inactive |
| **Focus** | Three-way force balance, well competition, Chladni transit, harmonic tension |
| **Why these** | Tests whether dissonant organisms maintain healthy separation while consonant ones attract. Three bodies reveal force balance issues invisible in pairs. |

**Expected behavior**: ISAO and ACID should orbit closer (shared root=9), SPGL should maintain distance but not be repelled to the boundary. All three should visit different gravity wells, not lock into one.

### ENV-3: Rhythm Ecology (percussion + melodic, beat sync)

| Property | Value |
|----------|-------|
| **Organisms** | TBLK (tblk-dha) + KKIT (kkit-909) + HOSO (hoso-malabar) + RECH (rech-eighteen) |
| **Root pitch** | All root=0 (C) |
| **BPM** | 108 |
| **Active DNA** | tblk-dha.json, kkit-909.json, hoso-malabar.json, rech-eighteen.json |
| **All others** | inactive |
| **Focus** | Rhythm sync between percussion and melodic, beat interaction, sample playback, diverse cell types |
| **Why these** | Mixes percussion (TBLK strike voices, KKIT drums), sample-based (RECH melodic_cell + sample_cell), and classic synth (HOSO). Tests rhythm_sync: hard vs soft. |

**Expected behavior**: Percussion organisms should develop rhythmic affinity. HOSO's sequencer should respond to beat_phase from TBLK/KKIT. RECH's melodic cells should create counterpoint. Four organisms = moderate force complexity.

### ENV-4: Full Ensemble (stress test, all species)

| Property | Value |
|----------|-------|
| **Organisms** | DRON + HOSO + ISAO + SPGL + TBLK + KKIT + RECH + ACID |
| **Root pitch** | Mixed — root=0 (DRON, HOSO, SPGL, TBLK, KKIT, RECH) + root=9 (ISAO, ACID) |
| **BPM** | 100 |
| **Active DNA** | All 8 base organisms active |
| **Focus** | Mass interaction, master_gain scaling, UI density, kill/despawn under load, Chladni saturation |
| **Why these** | Maximum organism count before union. If the ecology can't sustain 8 organisms interacting healthily, union (which adds more) will fail. |

**Expected behavior**: Organisms should self-organize into 2-3 clusters (root=0 group, root=9 group, with outliers). No permanent Chladni locking. Audio should be clear with dynamic master_gain. All UI controls responsive.

---

## 2. Environment Configuration

Each environment is a JSON preset file in `assets/env/` specifying which DNA files are active and global settings:

```json
{
  "name": "env-1-harmonic-duo",
  "bpm": 90,
  "active": ["dron-alpha", "hoso-malabar"],
  "notes": "Pairwise consonant interaction baseline"
}
```

**Implementation**: `load_environment(path)` sets DNA active flags and BPM before the app startup loop. Alternatively, each environment can be a shell script that copies/modifies DNA files and launches `cargo run`. The simplest approach: a `--env` CLI arg that loads the preset and overrides DNA active flags + BPM.

---

## 3. Interaction Standard (Checklist)

Each item is scored YES/NO per observation run.

### 3a. Visual Identity

| # | Criterion | Notes |
|---|-----------|-------|
| V1 | Each organism displays its name (not just ID number) in the viewport | Floating label, not just panel |
| V2 | Organism hue is visually distinct from all others on screen | No two organisms look identical |
| V3 | Chladni sub-nodes appear as proportionate features on the organism perimeter | Not giant lobes dominating the body |
| V4 | Species icon is visible in the organism panel row | Phosphor icon matches species |

### 3b. Force & Movement

| # | Criterion | Notes |
|---|-----------|-------|
| F1 | No organism is stationary for >15 seconds | Must be moving, even if slowly |
| F2 | No organism is locked to a single Chladni node well for >20 seconds | Must transit between hosts or escape |
| F3 | At least one pair shows visible attraction (closing distance) within 60 seconds | Affinity system is producing force |
| F4 | No organism is permanently stuck at the world boundary | Boundary forces must be balanced |
| F5 | Gravity wells show occupancy turnover (organisms arrive and depart) | Wells are not permanent traps |

### 3c. Audio

| # | Criterion | Notes |
|---|-----------|-------|
| A1 | No audible clicks, pops, or DC offset during 90-second run | Clean audio output |
| A2 | Each organism is audibly distinguishable (different timbre/rhythm) | Not a homogeneous wash |
| A3 | Kill button silences the organism within 0.5 seconds | No lingering audio after kill |
| A4 | Reverb/tape delay tails fade naturally after kill (no infinite echo) | Send buses clean up |
| A5 | Master volume stays controlled (no sudden jumps when organisms spawn/die) | Dynamic master_gain works |

### 3d. Interaction & Musicality

| # | Criterion | Notes |
|---|-----------|-------|
| M1 | At least one pair enters glob state (visible SDF merging) within 90 seconds | Affinity threshold is reachable |
| M2 | Consonant pairs (same root) show higher affinity than dissonant pairs | Harmonic interaction is audible in force behavior |
| M3 | Organisms respond to BPM changes ([ ] / ] keys) | Transport bridge works |
| M4 | Scale weights propagate (organisms quantize to gravity well keys) | Pitch gravity is active |

### 3e. UI Controls

| # | Criterion | Notes |
|---|-----------|-------|
| U1 | Kill button removes organism from screen and panel | Fixed in 7939bbd |
| U2 | Mute checkbox silences organism immediately | Shared handle works |
| U3 | Synth detail panel opens on organism row click | Selection works |
| U4 | Modulation target params are visually distinct from static params | Color/style difference |
| U5 | Send sliders (reverb/tape) produce audible change | Bus sends functional |

---

## 4. Known Issues to Fix During Iteration

These are anticipated failures from the current codebase. Each will be addressed as iteration reveals them:

| Issue | Likely Environment | Checklist Item | Fix Area |
|-------|-------------------|----------------|----------|
| Chladni node locking | ENV-2, ENV-4 | F2, F5 | `chladni.rs` force constants, dormancy tuning |
| No floating name labels | All | V1 | `app.rs` hover tag → name label |
| No modulation target highlighting | All | U4 | `synth_detail.rs` + `CellUiState` modulated params |
| Tape delay ghost on kill | ENV-4 | A3, A4 | Zero organism contribution in delay buffer on despawn |
| Master gain step on kill | ENV-4 | A5 | Slew the `dynamic_master_gain` transition |
| Identical interaction DNA | ENV-2, ENV-3 | F3, M2 | Species-specific interaction rules per interaction-tuning.md |
| Desire starts identical | ENV-1, ENV-2 | M1 | Per-species `desire_to_connect` in DNA |
| Glob flicker near threshold | ENV-1 | M1 | Hysteresis on glob on/off thresholds |

---

## 5. Implementation Phases

### Phase A — Infrastructure (env loader + labels + param highlighting)

1. Environment preset loader (`--env` CLI arg or `assets/env/` JSON)
2. Floating organism name labels in viewport (replace `[N]` with name)
3. Modulation target param highlighting in synth_detail (wired params colored differently)
4. Create `spec/iteration-log.md` template

### Phase B — Force & Interaction Tuning (ENV-1 + ENV-2 focus)

5. Species-specific `desire_to_connect` in DNA files
6. Chladni force tuning (NODE_WELL_RANGE, NODE_WELL_STRENGTH, dormancy cycle)
7. Glob hysteresis (on threshold 0.25, off threshold 0.15)
8. Interaction DNA differentiation (species-specific orbit/repel profiles)

### Phase C — Audio Polish (ENV-3 + ENV-4 focus)

9. Zero tape delay buffer contribution on organism kill
10. Slew dynamic_master_gain transitions (100ms ramp instead of instant step)
11. Verify sample_cell playback under load (RECH in ENV-3)

### Phase D — Validation (all 4 environments)

12. Run ENV-1 through iteration loop → 3 consecutive passes
13. Run ENV-2 through iteration loop → 3 consecutive passes
14. Run ENV-3 through iteration loop → 3 consecutive passes
15. Run ENV-4 through iteration loop → 3 consecutive passes
16. Tag commit as `pre-union-ready`

---

## 6. Dependency Graph Update

```
                        (completed)
    S32 + S33 + S35 + S38 + S39 + S40 + S41 + SAT
    S42 (RECH) + SAWL (call-response) + kill fix
                            |
                            v
    ┌────────── UI Revisions (parallel) ──────────┐
    │  UI-scales-transport   (western scales,     │
    │                         transport panel)     │
    │  UI-tempo-groove       (groove panel,       │
    │                         quantization grid)   │
    │  UI-inspector-nodegraph (live node graph,   │
    │                         structured readout)  │
    └─────────────────┬───────────────────────────┘
                      │
                      v
          ┌─── pre-union-iteration ───┐
          │  Phase A: infrastructure  │
          │  Phase B: force tuning    │
          │  Phase C: audio polish    │
          │  Phase D: validation      │
          └───────────┬───────────────┘
                      │
                      v
            organism-union Phase 1
            (glob state polish)
                      │
                      v
            organism-union Phase 2
            (gene code + identity)
                      │
                      v
            organism-union Phase 3
            (cell combination engine)
```

The organism-union spec's dependency line changes from:
```
Depends on: S32, S33, S35, S38, S39, S40, S41, SAT — all complete
```
to:
```
Depends on: pre-union-iteration (all 4 environments pass)
```

---

## 7. Union Spec Gaps (to address during or after iteration)

The organism-union spec (written 2026-03-04) predates 5 new cell types. Update needed:

| Cell | Added | Pruning Rule for Union |
|------|-------|----------------------|
| walk_cell | 2026-03-14 | Keep both — different walk trajectories produce distinct pitch contours |
| melodic_cell | 2026-03-14 | Keep both — unified walk+rhythm creates unique generative patterns |
| sample_cell | 2026-03-14 | Keep all — each references different samples via SampleRegistry URI |
| xy_pad_cell | 2026-03-16 | Keep at most one — human interface controller, not duplicatable |
| call_response_cell | 2026-03-16 | Keep one — phrase bank is per-organism identity; union picks higher-generation parent's bank |

Additional union considerations:
- **SampleRegistry**: Child organism needs registry access for inherited sample_cells
- **Phrase bank**: call_response_cell's phrase bank source must be resolved for child (inherit from one parent or merge)
- **Melodic cell gravity**: walk contour + scale quantization params need blending strategy (not just parameter average)

---

## Critical Files

| File | Changes |
|------|---------|
| `spec/pre-union-iteration.md` | **This spec** |
| `spec/organism-union.md` | Update dependency line + add new cell pruning rules |
| `spec/iteration-log.md` | **NEW** — dated observation log |
| `assets/env/env-1-harmonic-duo.json` | **NEW** — environment preset |
| `assets/env/env-2-tension-trio.json` | **NEW** — environment preset |
| `assets/env/env-3-rhythm-ecology.json` | **NEW** — environment preset |
| `assets/env/env-4-full-ensemble.json` | **NEW** — environment preset |
| `src/app.rs` | Floating name labels, env loader, master_gain slew |
| `src/ui/panels/synth_detail.rs` | Modulation target highlighting |
| `src/ui/panels/organism_panel.rs` | CellUiState modulated_params field |
| `src/organism/chladni.rs` | Force constant tuning |
| `src/organism/registry.rs` | Glob hysteresis, species interaction DNA |
| `src/audio/tape_delay_bus.rs` | Zero buffer on organism kill |
| `src/audio/gain_staging.rs` | Slew dynamic_master_gain |
| `assets/dna/*.json` | Per-species desire_to_connect, interaction rules |
