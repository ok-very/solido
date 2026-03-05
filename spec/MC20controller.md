<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# return it as an md

```md
# Meaning Field Patch System (Exploratory Spec v0.3)

A live, spatial, closed-loop “meaning bus” where **organisms** sense and emit into Fields, and Links (patch cables) route and shape those influences in real time.

## 1) Goals

- Enable expressive, always-live re-wiring of “meaning” (priority, emotion, attraction, motion bias) without issuing direct organism-to-organism commands.
- Keep emergent behavior **debuggable**: the system must explain what is influencing an organism *right now*.
- Keep mappings survivable across multiple viewports / view-nodes / interaction schemes by making state continuously visible and consistent with a small canonical vocabulary (visibility of system status, consistency, error prevention). [web:116]
- Support a top-down influence mode that can be instantly bypassed (global and per-context).

Non-goals:
- Faithfulness to MS-20 control-voltage metaphors or electrical semantics.
- Unlimited port growth; v0 is intentionally small.

## 2) Core model

### Vocabulary (canonical)
- **Field**: a set of spatial layers defined over world space `x` (and time), e.g., `Vitality(x)`, `Valence(x)`, `Attention(x)`.
- **Sensation**: sampling a Field at a position/neighborhood (value and optionally gradient).
- **Emission**: writing into a Field around a position (splat/trail), with falloff and radius.
- **Nerves**: internal “wiring harness” that maps Sensations into organism priorities and maps Emissions out of organism state/behavior.
- **Link**: an always-live connection (Output → Input) that routes meaning with mandatory transforms (strength, smoothing, limits).

### Field set (v0)
Start with 6 layers (resist adding more until shipping multiple schemes):
- `Vitality(x)` — activation/energy
- `Valence(x)` — pleasant ↔ aversive (bipolar)
- `Attention(x)` — salience/priority
- `Affinity(x)` — bias to cohere/link
- `Avoidance(x)` — bias to separate/repel
- `Flow(x)` — vector-like current/drift (may be represented as `FlowX(x)`, `FlowY(x)`)

Each Field MUST have:
- Decay (returns toward baseline over time)
- Diffusion/smoothing (spatial blur / laplacian-like)
- Clamp (hard bounds)

### Port taxonomy (what the patch surface exposes)
Keep physical ports in 3 families (plus global/events):

**Sensation ports (outputs)**
- `Sense.Vitality`
- `Sense.Valence`
- `Sense.Attention`
- `Sense.Affinity`
- `Sense.Avoidance`
- `Sense.Flow` (or `Sense.FlowX`, `Sense.FlowY`)

**Emission ports (inputs)**
- `Emit.Vitality`
- `Emit.Valence`
- `Emit.Attention`
- `Emit.Affinity`
- `Emit.Avoidance`
- `Emit.Flow`

**Nerve ports (inputs; organism biases)**
- `Nerve.Drive`
- `Nerve.Calm`
- `Nerve.LinkBias`
- `Nerve.AvoidBias`

**Globals / events**
- `Global.FieldInfluence` (0–100%)
- `Global.Damping`
- `Event.Panic`
- `Event.Snapshot` (capture state / bookmark)

Schemes may alias these names for narrative flavor, but the canonical IDs stay stable.

## 3) Runtime semantics (closed loop, spatial)

### Update loop (conceptual)
At each simulation tick:
1) **Sensation sampling**: each organism samples relevant Fields at its position (and optionally neighborhood).
2) **Nerve evaluation**: Sensations update organism priorities (Drive/Calm/LinkBias/AvoidBias) via the current Nerve wiring.
3) **Behavior**: organisms move, signal, bond, avoid, etc.
4) **Emission accumulation**: organism actions and state generate Emissions (local splats/trails) into Fields via routed Links.
5) **Field solve**: apply diffusion + decay + clamp; optionally advect with Flow.

### Top-down influence + bypass
- `Global.FieldInfluence` mixes “Field-guided behavior” into organism decisions.
- Bypass sets `FieldInfluence = 0` immediately (global and per-viewport if needed).
- Always-live changes are allowed, but must be bounded by clamps and smoothing.

### Link semantics (always-live)
Every Link has a mandatory transform block:
- `Strength` (gain)
- `Smoothing` (lag / slew in ms)
- `Limits` (clamp min/max)
Optional:
- `Invert` (for bipolar signals)
- `Curve` (response shaping)
- `Quantize` (if you want stepped regimes)

**Soft-start rule**: when a new Link is created, it ramps from 0 → target Strength over ~100–300 ms to prevent instantaneous spikes.

**Feedback rule**: closed-loop Links are allowed, but every loop must include at least one slow element (Field decay/diffusion and/or Link smoothing) to keep dynamics ecological rather than explosive.

## 4) Interaction + UI contract (multi-scheme survivability)

### Always-on status (non-negotiable)
A persistent overlay must show:
- Active **Scheme** name (current mapping set)
- Armed viewport/view-node
- `FieldInfluence %` and bypass state
- Live list of Links (source → dest) with Strength and Smoothing at minimum

This is “visibility of system status,” which is critical in always-live complex tools. [web:116]

### Tape-friendly labeling strategy
- Every physical jack gets a stable ID: `J01…Jn`.
- Physical label (short): `SENSE:VIT J12` / `EMIT:VAL J31` / `NERVE:DRIVE J07`.
- On-screen label (full): `Sense.Vitality @ FocusOrganism`, current alias, plus the Link’s transform values.

### Editing & recovery (always-live safety)
- Global `Event.Panic` must be reachable without hunting; it should immediately mute or bypass Field influence and/or cut Emissions. [web:116]
- Undo/redo for patch operations is first-class (connect/disconnect, Strength changes, etc.). [web:116]
- Per-Link bypass toggle and per-Link “safe mode” (caps Strength and increases Smoothing) for live tuning.

### Debuggability for emergence
Provide at least three views:
1) Field heatmaps with legend and clamp indicators.
2) Organism probe: top-N influences currently affecting that organism’s Nerves.
3) Link contribution inspector: shows which Links are injecting into which Fields and at what effective strength after transforms.

Use an MDA lens when evaluating changes: Links/Fields are **mechanics**, the observed ecological behaviors are **dynamics**, and the felt “dialog” is the **aesthetic** outcome. [web:141]

## 5) MVP slice + constraints

### MVP (1–2 weeks)
Implement:
- Fields: Vitality, Valence, Attention (diffuse + decay + clamp).
- Sensation: organisms sample Vitality/Valence at position.
- Nerves: `Nerve.Drive` and `Nerve.Calm` influence movement + linking.
- Emission: organisms emit Vitality and Valence based on “signal” behavior.
- Patch: Links route `Sense.*` into `Nerve.*` and route organism emission into `Emit.*`.
- UI: always-on overlay + heatmap + organism probe + Panic + undo.

### Hard constraints (keep it from melting)
- Fixed canonical vocabulary (Field/Link/Nerves/Emission/Sensation) across all schemes.
- New Links soft-start; Links always have clamps and smoothing defaults.
- Field solver enforces decay/diffusion/clamps.
- FieldInfluence always visible and bypassable.

### Open questions (for v0.4)
- Do you want Flow as a first-class vector field (two channels) or derived from other fields?
- Should Links be “global” (affect all organisms) or scoped (only affect focused organism / selected group / region)?
- Do you want region probes (fixed points in space) as first-class entities alongside organisms?
```

