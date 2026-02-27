# S15 — Port Semantic Tags

> Not every Float is the same Float. A frequency is not an amplitude is not a ratio.

**Layer**: L0 (Module Contract)
**Depends on**: S01 (module contract), S02 (routing backbone)
**Status**: Prospect

## Goal

Add lightweight semantic tags to ports so that edge discovery and exploration can distinguish between type-compatible but semantically incompatible connections. Currently, any Float output can connect to any Float input if ranges overlap. This wastes exploration edges discovering that "amplitude → pitch_hz" is useless, and makes Hebbian learning do work that type metadata could prevent.

## Ancestry (MAKE A BABY)

Max/MSP distinguishes signal (`~`) from message inlets by convention and color. But within signals, a `cycle~ frequency` inlet and a `*~ gain` inlet are both signal-rate floats — Max relies on the user to know the difference. Solido can do better: semantic tags make the system know.

## The Problem

Consider the current port landscape for 3 organisms + infrastructure:

| Port | Type | Range | Semantic |
|------|------|-------|----------|
| quantizer.pitch_hz | Float | [20, 20000] | Frequency |
| voice.pitch_hz | Float | [20, 20000] | Frequency |
| organism:tblk.rms | Float | [0, 1] | Level |
| organism:dron.rms | Float | [0, 1] | Level |
| tala.beat_phase | Float | [0, 1] | Phase |
| gravity.strength | Float | [0, 1] | Amount |

Range checking prevents `pitch_hz [20,20000]` → `rms [0,1]` (range mismatch). But `rms [0,1]` → `beat_phase [0,1]` passes range check despite being semantically meaningless. Exploration creates this edge, Hebbian learning spends 1000+ ticks pruning it.

## Architecture Decisions

### AD-1: Semantic tags are optional, additive constraints

Tags don't replace type or range checking — they're an additional filter during edge discovery. Ports without tags remain compatible with everything (backward compatible). Tags only restrict when BOTH ports have them and they're incompatible.

### AD-2: Small fixed enum, not open strings

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSemantic {
    /// Frequency in Hz (pitch, cutoff, rate)
    Frequency,
    /// Amplitude / level / gain [0,1] or [-1,1]
    Level,
    /// Phase or position within a cycle [0,1]
    Phase,
    /// Trigger / gate (discrete events)
    Gate,
    /// Ratio or multiplier (detune, FM index, mix amount)
    Ratio,
    /// Spatial position (pan, x/y coordinate)
    Spatial,
    /// Time duration (attack, decay, delay time)
    Duration,
}
```

Fixed enum, not strings. This keeps compatibility checks branchless and avoids typo-based misconnections. New semantics require a code change — intentional friction.

### AD-3: Compatibility matrix is permissive, not restrictive

Most pairings are **compatible** (same tag or either tag is None). Only clearly wrong pairings are blocked:

| → | Freq | Level | Phase | Gate | Ratio | Spatial | Duration |
|---|------|-------|-------|------|-------|---------|----------|
| **Freq** | yes | no | no | no | no | no | no |
| **Level** | no | yes | yes | no | yes | no | no |
| **Phase** | no | yes | yes | no | no | no | no |
| **Gate** | no | no | no | yes | no | no | no |
| **Ratio** | no | yes | no | no | yes | no | no |
| **Spatial** | no | no | no | no | no | yes | no |
| **Duration** | no | no | no | no | no | no | yes |

Level↔Phase: compatible (phase can modulate level, level can gate phase).
Level↔Ratio: compatible (gain is a ratio of amplitude).

### AD-4: Infrastructure edges use semantics strictly; organism edges use them as soft hints

- **Infrastructure** (`InfrastructureRouter`): Semantic mismatch → edge NOT created. This matches the existing name-matching behavior but makes it type-safe.
- **Organism** (`AffinityGraph`): Semantic mismatch → edge created at lower initial weight (0.2 instead of 0.5). Hebbian learning can still strengthen it if it turns out to be productive. This preserves serendipitous discovery while biasing toward meaningful connections.

## Implementation

### 1. Add PortSemantic to Port

```rust
pub struct Port {
    pub id: PortId,
    pub name: Arc<str>,
    // ... existing fields
    pub semantic: Option<PortSemantic>,  // NEW
}
```

Builder method: `.with_semantic(PortSemantic::Frequency)`

### 2. Compatibility function

```rust
pub fn semantics_compatible(out: Option<PortSemantic>, inp: Option<PortSemantic>) -> bool {
    match (out, inp) {
        (None, _) | (_, None) => true,  // untagged ports are always compatible
        (Some(a), Some(b)) => COMPAT_MATRIX[a as usize][b as usize],
    }
}
```

### 3. Update edge discovery

- `reactor/mod.rs`: `discover_organism_edges()` adds `semantics_compatible()` check
- `reactor/infrastructure.rs`: `discover_infra_edges()` adds semantic check (replacing some name checks)

### 4. Tag existing ports

Retrofit semantic tags onto all existing module ports. Examples:
- `quantizer.pitch_hz` → `Frequency`
- `voice.freq` → `Frequency`
- `audio_analysis.rms` → `Level`
- `tala.beat_phase` → `Phase`
- `organism:*.rms` → `Level`
- `organism:*.is_active` → `Gate`

## Files Modified

| File | Changes |
|------|---------|
| `src/module/port.rs` | `PortSemantic` enum, `semantic` field on Port, compatibility check |
| `src/reactor/mod.rs` | Semantic check in organism edge discovery |
| `src/reactor/infrastructure.rs` | Semantic check in infra edge discovery |
| `src/modules/quantizer.rs` | Tag ports |
| `src/modules/voice_module.rs` | Tag ports |
| `src/modules/tala_module.rs` | Tag ports |
| `src/modules/raga_module.rs` | Tag ports |
| `src/modules/keyboard_input.rs` | Tag ports |
| `src/modules/audio_analysis.rs` | Tag ports |
| `src/organism/module.rs` | Tag ports |

## Verification

- [ ] Frequency→Level edge is NOT created during organism exploration
- [ ] Level→Phase edge IS created (compatible)
- [ ] Untagged ports connect freely (backward compatible)
- [ ] Infrastructure edge discovery respects semantics
- [ ] Organism edges with semantic mismatch start at weight 0.2
- [ ] Exploration candidate count decreases (fewer spurious candidates)
- [ ] All existing tests pass (tags are optional, default None)
