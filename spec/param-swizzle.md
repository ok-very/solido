# Runtime Parameter Swizzling

> The best sounds in electronic music come from "happy accidents" -- patching
> things that were never designed to connect. But you need guardrails so it
> doesn't become noise.

**Status**: Prospect
**Depends on**: interaction_params DNA extension, edge pinning (S02c)

## Goal

Organisms discover novel parameter mappings NOT defined in DNA. DNA defines
safe starting points ("normalled" routing); runtime swizzling finds unexpected
but musically useful connections. Circuit bending -- connecting an organism's
rhythm density to another's filter cutoff, even though no species whitelist
says they belong together.

## DNA Foundation: interaction_params

```json
"interaction_params": {
  "exports": [
    { "name": "rhythm_density", "source": "rhythm_density", "signal_type": "Float" },
    { "name": "rms", "source": "rms", "signal_type": "Float" }
  ],
  "imports": [
    { "name": "cutoff_mod", "target": "cell2.cutoff", "range": [20, 5000],
      "gain": 1500.0, "mode": "Add", "accepts_from": ["*"] }
  ]
}
```

Exports become output ports. Imports become input ports that bridge to Shared
handles. Normal exploration connects matching exports to imports by type.
Swizzling goes further.

## Mechanism: Relaxed Exploration

Standard exploration (`graph.rs:maybe_explore`) requires signal_type match AND
ranges_compatible AND rates_compatible. Swizzling adds a second path:

1. **Type match still required** -- Float to Float only. No Trigger-to-Float.
2. **Range check skipped** -- [0, 10] export can target [0, 1] import.
   Gain clamping prevents exceeding import's declared range.
3. **Species whitelist skipped** -- KKIT rhythm can modulate ACID cutoff.
4. **Lower initial weight** -- 0.2 (vs 0.5 normal). Must earn its way up.
5. **Faster pruning** -- min_age=500 ticks (vs 1000), threshold=0.15 (vs 0.1).
6. **Higher arousal gate** -- triggers at arousal > 0.5 (vs 0.3 normal).

New `LedgerReason::Swizzle` tags these edges for explainability.

## Safety Rails

### Gain Clamping

Source value normalized to [0, 1] using export's range, then scaled to import's
range. Clamped to `[min, min + 2*(max-min)]`. The "2x rule" allows pushing
parameters beyond DNA range for expression, but never beyond double. A cutoff
with range [200, 8000] Hz can reach 16000 Hz but never 40000.

### Emergency Bypass

If receiving organism's valence drops below -0.8 for 100 consecutive ticks
while a swizzle edge is active, that edge is immediately removed. Ledger records
`EmergencyDisconnect`. Prevents pathological connections from persisting through
Hebbian inertia.

### Gradual Ramp-Up

New swizzle connections start at 10% effectiveness. Ramps linearly from 0.1 to
1.0 over 1800 ticks (~30 seconds at 60Hz). Delivered signal multiplied by
effectiveness. Prevents sudden parameter jumps.

## The Overseer

A `SwizzleOverseer` meta-module monitors aggregate system health:

- **Reads**: mean valence and arousal across all organism modules each tick
- **Global inhibit**: mean arousal > 0.7 disables swizzle exploration until
  arousal drops below 0.5. Existing edges continue operating.
- **Cooldown**: 300-tick holdoff after inhibiting, prevents rapid cycling
- **No edges of its own** -- Infrastructure tier, reads `graph.emotions` directly

## Musical Rationale

Normalled routing is a mixing desk. Swizzling is a patch bay with no labels.
TBLK's polyrhythmic density modulating ACID's filter cutoff creates sidechained
acid lines nobody designed. SPGL's glacial pitch drift warping DRON's reverb
decay produces evolving ambient textures. KKIT's mechanical grid driving HOSO's
PWM width turns a clinical bass into a rhythmic growl.

The guardrails -- gain clamping, emergency bypass, ramp-up, overseer -- keep the
system between "interesting" and "broken." Hebbian does the rest: connections
that make the receiving organism happy survive; connections that cause distress
are pruned. Natural selection for patch cables.

## Critical Files

| File | Change |
|------|--------|
| `src/affinity/graph.rs` | `maybe_explore_swizzle()`, emergency bypass |
| `src/affinity/edge.rs` | `swizzle: bool`, `effectiveness: f32` fields |
| `src/organism/dna.rs` | `InteractionParams` with export/import defs |
| `src/organism/module.rs` | Dynamic ports, import-to-Shared bridging |
| `src/affinity/ledger.rs` | `Swizzle` reason, `EmergencyDisconnect` event |
| `src/reactor/mod.rs` | Overseer registration, swizzle pass in tick |
