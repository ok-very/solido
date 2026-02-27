# S01b — Module Lifecycle Hooks

> Born, registered, alive, dying, dead. Every module deserves a proper lifecycle.

**Layer**: L0 (Module Contract)
**Depends on**: S01 (module contract)
**Status**: Prospect

## Goal

Add lifecycle hooks to `ModuleCore` so modules know when they're registered (receiving their ModuleId), and organisms can shut down gracefully (release envelopes, fade audio) before being removed from the reactor. Also eliminate the `as_any` downcast escape hatch by providing a typed event channel.

## Ancestry (MAKE A BABY)

In Max/MSP, objects receive `loadbang` on patch load and `closebang` on close. Solido modules currently have neither — they're constructed, tick forever, then vanish. A proper lifecycle lets organisms die musically.

## The Problem

### No registration notification

Modules don't know their own `ModuleId`. If a module needs to log events, request specific edges, or identify itself in debug output, it has to receive the ID out-of-band. The `ModuleSchema` carries the module's name, but not its runtime ID.

### Abrupt unregister

`reactor.unregister(id)` immediately removes the module, its emotions, and all its edges. If the organism is mid-note, the audio thread holds stale SharedHandles and may produce garbage until the ring buffer drains. There's no "please stop playing and tell me when you're done."

### `as_any` breaks the signal contract

`ModuleCore` requires `as_any()` solely so `app.rs` can downcast to `KeyboardInputModule` and call `handle_key_event()` directly. This is a trapdoor in the module contract — every future module that needs external input copies this pattern. The correct approach is an event channel.

## Architecture Decisions

### AD-1: Add on_register and on_unregister hooks

```rust
pub trait ModuleCore: Send {
    // ... existing methods ...

    /// Called after the module is registered with the reactor.
    /// Receives the assigned ModuleId for self-identification.
    fn on_register(&mut self, _id: ModuleId) {}

    /// Called when the module is about to be unregistered.
    /// Return `false` to request a grace period (module stays alive
    /// until it returns `true` on a subsequent call, or 5 seconds elapse).
    fn on_unregister(&mut self) -> bool { true }
}
```

Default implementations ensure backward compatibility — existing modules don't need changes.

### AD-2: Graceful shutdown with timeout

When `reactor.unregister(id)` is called:

1. Call `module.on_unregister()`. If it returns `true`, remove immediately (current behavior).
2. If it returns `false`, mark the module as `dying`. Continue ticking it, but stop delivering signals to it. Call `on_unregister()` each tick.
3. If it returns `true` on a subsequent tick, or 5 seconds (300 ticks) elapse, remove it.

OrganismModule's `on_unregister()` sends a `DspCommand::Panic` (release all notes), then waits for `current_rms < 0.001` before returning `true`. This lets envelopes ring out.

### AD-3: Replace as_any with a receive_event method

```rust
pub trait ModuleCore: Send {
    // ... existing methods ...

    /// Receive an external event (keyboard input, MIDI, OSC, etc.).
    /// Events are typed by the module — the reactor doesn't interpret them.
    /// Default implementation ignores all events.
    fn receive_event(&mut self, _event: &dyn std::any::Any) {}
}
```

`app.rs` calls `module.receive_event(&KeyEvent { ... })` instead of `module.as_any_mut().downcast_mut::<KeyboardInputModule>()`. The module checks the type internally:

```rust
fn receive_event(&mut self, event: &dyn Any) {
    if let Some(key_event) = event.downcast_ref::<KeyEvent>() {
        self.handle_key(key_event);
    }
}
```

This keeps the module contract clean — no downcasting at the call site, no `as_any` in the trait.

### AD-4: Deprecate as_any, remove after migration

Keep `as_any()` / `as_any_mut()` in the trait with `#[deprecated]` for one release cycle. Remove after all call sites migrate to `receive_event()`.

## Implementation

### 1. Extend ModuleCore trait

Add `on_register`, `on_unregister`, `receive_event` with default implementations.

### 2. Reactor tracks dying modules

New field on SeedReactor: `dying: HashMap<ModuleId, u32>` (tick count since shutdown started). Modified tick cycle skips signal delivery to dying modules but continues calling `tick()` and `on_unregister()`.

### 3. Migrate KeyboardInputModule

Replace `as_any` downcast in `app.rs` with `receive_event` call. Define `KeyEvent` struct in `modules/keyboard_input.rs`.

### 4. Migrate OrganismModule

Implement `on_register` to store ModuleId. Implement `on_unregister` to send Panic and wait for silence.

### 5. Migrate all other downcasts

Search for `as_any` usage in `app.rs` and migrate each to `receive_event`.

## Files Modified

| File | Changes |
|------|---------|
| `src/module/mod.rs` | `on_register`, `on_unregister`, `receive_event` on ModuleCore |
| `src/reactor/mod.rs` | Graceful shutdown logic, dying module tracking |
| `src/modules/keyboard_input.rs` | `receive_event` for KeyEvent, deprecate `handle_key_event` |
| `src/organism/module.rs` | `on_register` stores ID, `on_unregister` sends Panic + waits |
| `src/app.rs` | Replace all `as_any` downcasts with `receive_event` calls |

## Verification

- [ ] OrganismModule receives its ModuleId via on_register
- [ ] Unregistering an organism mid-note: envelope rings out before removal
- [ ] Timeout: dying module removed after 5 seconds even if still noisy
- [ ] KeyboardInputModule receives key events via receive_event (no downcast)
- [ ] All as_any call sites in app.rs migrated
- [ ] Existing modules with default hooks compile and work unchanged
- [ ] No audio glitches during organism shutdown
