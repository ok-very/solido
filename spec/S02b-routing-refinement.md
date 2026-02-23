# S02b — Routing Refinement: Range-Aware Edge Discovery

> Not every Float is the same Float.

## Problem

S02 auto-discovery creates edges between ALL type-compatible port pairs.
Every Float output connects to every Float input. With 5+ modules this
means raw_pitch (0.0-1.0), cursor_x (0.0-1.0), rms (0.0-1.0), and
pitch_hz (20-20000) all arrive at the same input. Multi-cast delivers
all of them. The last one to arrive wins, which is non-deterministic.

Result: VoiceModule's pitch_hz input receives garbage values. Voices
spawn at 20 Hz (the clamp floor). Users hear clicks instead of tones.

The Hebbian learning was designed to solve this over time, but:
1. It takes hundreds of ticks to differentiate good vs bad edges
2. On first launch, everything is garbage
3. The valence signal is too weak to quickly prune bad Float→Float edges
   because the module has no way to know *which* edge caused a bad result

## Goal

Make auto-discovery smarter so obviously incompatible ports don't connect
in the first place, while preserving the self-organizing philosophy.

## Depends On

- S02 (AffinityGraph, RoutingTable, auto-discovery)
- S05 (concrete evidence of the problem — Float cross-contamination)

## Approach: Range-Compatible Edge Discovery

Ports already carry optional range metadata:
```rust
pub struct Port {
    pub range: Option<(f32, f32)>,  // already exists
    ...
}
```

### 2b.1 Range overlap check in auto-discovery

When creating edges, check if the output port's range overlaps the
input port's range. If both ports have ranges and they DON'T overlap,
skip the edge.

```rust
fn ranges_compatible(out: &Port, inp: &Port) -> bool {
    match (out.range, inp.range) {
        (Some((o_min, o_max)), Some((i_min, i_max))) => {
            // Overlap exists if one range's min is within the other's span
            o_max >= i_min && i_max >= o_min
        }
        _ => true,  // if either lacks a range, allow (preserve current behavior)
    }
}
```

This is the minimum viable fix:
- `raw_pitch` [0,1] vs `pitch_hz` [20,20000] → no overlap → no edge
- `pitch_hz` [20,20000] vs `pitch_hz` [20,20000] → overlap → edge created
- `gravity_delta` [-1,1] vs `amplitude` [0,1] → overlap → edge created (reasonable)
- Ports without ranges → always allowed (backward compat)

### 2b.2 Initial weight seeding (optional)

Edges between ports with matching names could start at higher weight:
```rust
let initial_weight = if out.name == inp.name { 0.7 } else { 0.5 };
```

This gives the Hebbian system a head start without removing flexibility.

### 2b.3 Exploration respects ranges too

`AffinityGraph::maybe_explore()` should use the same range check when
proposing new edges. No point exploring connections that are known
incompatible.

## Files Modified

```
src/reactor/mod.rs         — auto-discovery uses ranges_compatible()
src/affinity/graph.rs      — maybe_explore() uses ranges_compatible()
src/module/port.rs         — add ranges_compatible() helper
```

## Verification

1. Unit test: ranges_compatible returns false for [0,1] vs [20,20000]
2. Unit test: ranges_compatible returns true for [0,1] vs [0,1]
3. Unit test: ranges_compatible returns true when either range is None
4. Integration: register KeyboardInput + Quantizer + VoiceModule →
   no edge from raw_pitch to pitch_hz, but edge from quantizer pitch_hz
   to voice pitch_hz
5. Manual: press number keys → hear Bhairav tones, not clicks
