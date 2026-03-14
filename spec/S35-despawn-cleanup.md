# S35 — Despawn Cleanup Sweep

**Status**: Complete (Mar 2026)
**Depends on**: S32 (continuous attachment), S34 (audio polish)
**Blocks**: Organism union/merge (must clean up parents after fusion)

---

## Goal

Ensure that killing/despawning an organism fully cleans up its audio path, effect bus sends, and GPU-side resources. Currently despawning only removes the organism from the visual registry and reactor — the OrganismDsp, VoiceBus strip, ReverbBus send, and TapeDelayBus send all leak permanently.

---

## Problem Statement

### Current despawn chain (`kill_organism`)

```
1. mixer_mute.set(1.0)              — silences DRY path only
2. panel.organisms.remove(idx)       — UI cleanup
3. reactor.unregister(mod_id)        — sends DspCommand::Panic, removes graph edges
4. organism_registry.despawn(org_id) — removes from visual sim
```

### What's missing

| Resource | Leaked? | Impact |
|----------|---------|--------|
| `OrganismDsp` in audio callback | Yes — ticks every sample forever | CPU waste: oscillators, filters, sequencers still running |
| `VoiceBus` channel strip | Yes — muted but still iterated | Minor CPU (atomic reads per frame) |
| `ReverbBus` send level (Shared) | Yes — frozen at last non-zero value | **Reverb receives raw pre-VoiceBus signal forever** — audible infinite reverb tail |
| `TapeDelayBus` send level (Shared) | Yes — frozen at last value | **Tape delay receives signal forever** — audible infinite echo |
| Control-thread handle Vecs | Yes — dead entries accumulate | Index mismatch risk on repeated spawn/kill cycles |

### Root cause of "reverb runs forever"

The reverb bus `tick()` reads from the `sources[]` array which contains **raw OrganismDsp output** (pre-VoiceBus). The VoiceBus mute only affects the dry mix. Since the OrganismDsp keeps ticking and the send level is frozen at its last value, the reverb bus receives full-strength input from the killed organism indefinitely.

### GPU impact

The visual registry correctly removes the organism from `build_gpu_payload()`. However, repeated spawn/kill cycles cause the audio thread's organism Vec to grow unboundedly (organisms are never removed), increasing per-callback CPU time. Frame drops occur when the audio callback overruns, causing the GPU render loop to stall waiting for vsync while the audio thread starves.

---

## Architecture

### Two-phase fix

**Phase A — Immediate send zeroing (quick fix, no new channels)**

Zero the reverb and tape delay send Shared handles before removing the organism from the panel. This stops new audio from entering the effect buses. Existing reverb/delay tails decay naturally (RT60 ~2.5s for reverb, feedback-dependent for tape delay).

**Phase B — Audio thread tombstone (full fix, new channel)**

Add a despawn channel from control thread to audio callback. The audio thread marks organism slots as dead (tombstone pattern) and skips them during tick/source assembly. This reclaims CPU but requires careful RT-safe implementation.

---

## Phase A — Immediate Send Zeroing

### File: `src/app.rs` — `kill_organism()`

```rust
fn kill_organism(&mut self, ka: KillAction) {
    // 1. Mute dry path + zero effect sends BEFORE removing from panel
    if let Some(ref panel) = self.organism_panel {
        if let Some(org_ui) = panel.organisms.get(ka.panel_idx) {
            org_ui.mixer_mute.set(1.0);
            // Zero reverb send — stops feeding the reverb bus
            if let Some(ref rs) = org_ui.reverb_send {
                rs.set(0.0);
            }
            // Zero tape delay send — stops feeding the delay bus
            if let Some(ref ts) = org_ui.tape_delay_send {
                ts.set(0.0);
            }
        }
    }

    // 2. Remove from organism panel (UI)
    if let Some(ref mut panel) = self.organism_panel {
        panel.organisms.remove(ka.panel_idx);
    }

    // 3. Unregister from reactor
    self.reactor.unregister(ka.mod_id);

    // 4. Despawn from visual registry
    self.organism_registry.despawn(ka.org_id);
}
```

**RT safety**: `Shared::set()` is `AtomicU32::store(Relaxed)` — always safe.

**Limitation**: The OrganismDsp still ticks every sample (CPU waste), but produces no audible output since both dry (muted) and wet (sends zeroed) paths are silenced.

---

## Phase B — Audio Thread Tombstone

### New channel: `despawn_tx` / `despawn_rx`

**File: `src/substrate/audio.rs`**

Add a SPSC channel for despawn indices alongside the existing `spawn_tx`:

```rust
pub struct AudioSubstrate {
    pub spawn_tx: Sender<SpawnPayload>,
    pub despawn_tx: Sender<usize>,  // NEW: organism index to tombstone
    // ...
}
```

### Audio callback: tombstone pattern

Inside the CPAL callback, drain the despawn channel and mark slots:

```rust
// Drain despawn commands (RT-safe: try_recv is lock-free)
while let Ok(idx) = despawn_rx.try_recv() {
    if idx < alive.len() {
        alive[idx] = false;  // tombstone flag
    }
}

// Per-frame loop: skip dead organisms
for (org_idx, org) in organisms.iter_mut().enumerate() {
    if !alive[org_idx] {
        sources[org_idx] = [0.0, 0.0];  // zero contribution
        continue;
    }
    // ... normal tick ...
}
```

`alive` is a `[bool; MAX_CHANNELS]` array — preallocated, stack-sized, no heap.

### VoiceBus: skip dead strips

Add a `dead` flag to `ChannelStrip`:

```rust
pub struct ChannelStrip {
    pub gain: Shared,
    pub mute: Shared,
    pub solo: Shared,
    pub dead: bool,  // NEW: set by tombstone, skipped in process_frame
    // ...
}
```

In `process_frame()`, skip dead strips entirely (saves atomic reads).

### ReverbBus / TapeDelayBus: zero dead sends

When an organism is tombstoned, its index in `send_levels` is zeroed by the control thread (Phase A). The audio thread additionally skips the dead slot's contribution in the send sum loop.

### Control thread: `kill_organism()` updated

```rust
fn kill_organism(&mut self, ka: KillAction) {
    // 1. Zero sends + mute (Phase A)
    // ... same as above ...

    // 2. Send tombstone index to audio thread
    if let Some(ref mut audio) = self.audio {
        let _ = audio.despawn_tx.try_send(ka.audio_idx);
    }

    // 3-4. UI, reactor, registry cleanup (unchanged)
}
```

### Tracking the audio index

`KillAction` needs the organism's audio-thread index. This is the position in the audio callback's `organisms` Vec, which matches the spawn order. Track it:

```rust
pub struct KillAction {
    pub panel_idx: usize,
    pub mod_id: ModuleId,
    pub org_id: OrganismId,
    pub audio_idx: usize,  // NEW: index into audio thread organisms Vec
}
```

Store this at spawn time (it's the count of organisms at the time of spawning).

---

## Phase C — Slot Recycling (Future, Optional)

After enough organisms are tombstoned, recycle their slots for new spawns instead of appending. This caps the audio-thread organism Vec at MAX_CHANNELS and prevents unbounded growth.

```rust
// In spawn integration:
if let Some(free_idx) = first_dead_slot(&alive) {
    organisms[free_idx] = new_org;
    alive[free_idx] = true;
    // update VoiceBus strip, send levels at this index
} else if organisms.len() < MAX_CHANNELS {
    organisms.push(new_org);
    alive[organisms.len() - 1] = true;
}
```

Deferred because it requires updating all index-based handles (VoiceBus, sends, panel) to support reuse.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/app.rs` | `kill_organism()`: zero sends (Phase A), send tombstone (Phase B), track audio_idx |
| `src/substrate/audio.rs` | Add `despawn_tx`/`despawn_rx`, `alive` array, skip dead in tick loop |
| `src/audio/voice_bus.rs` | `ChannelStrip::dead` flag, skip in `process_frame()` |
| `src/audio/reverb_bus.rs` | Skip dead organism sends in `tick()` |
| `src/audio/tape_delay_bus.rs` | Skip dead organism sends in `tick()` |
| `src/ui/panels/organism_panel.rs` | `KillAction` gets `audio_idx` field |

---

## Implementation Order

1. **Phase A first** — 10-line change in `kill_organism()`. Fixes the audible reverb/delay leak immediately. Ship and verify.
2. **Phase B second** — Adds despawn channel + tombstone. Fixes CPU waste from dead OrganismDsp instances. More complex, requires audio_idx tracking.
3. **Phase C later** — Slot recycling. Only needed if spawn/kill churn exceeds 16 organisms per session.

---

## Verification

### Phase A
1. Spawn 2 organisms, let reverb build up
2. Kill one organism
3. Reverb tail should decay naturally over ~2.5s then silence
4. Tape delay should decay within feedback loop time
5. No infinite reverb/delay from killed organism

### Phase B
1. Spawn 4 organisms, kill 2
2. CPU usage should drop measurably (2 fewer DSP graphs ticking)
3. VoiceBus meter should show 0 for dead strips
4. Spawn 2 more — should succeed (total audio slots = 6, not 4)

### Phase C
1. Rapid spawn/kill 20+ organisms
2. Audio callback organism count should stay bounded at MAX_CHANNELS
3. New spawns reuse dead slots

---

## RT Safety Constraints

All Phase B changes on the audio thread must be allocation-free:
- `alive` array: `[bool; MAX_CHANNELS]` — stack, preallocated
- `despawn_rx.try_recv()`: SPSC ringbuf, lock-free
- Skip logic: branch on `alive[idx]`, zero `sources[idx]` — pure arithmetic
- No Vec removal, no reallocation, no locks
