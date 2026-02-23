# Audit Report: L1-S02 Routing Backbone

**Date:** 2026-02-22
**Status:** Pre-Implementation Review
**Component:** Affinity Graph & Seed Reactor

## Executive Summary

The L1-S02 specification outlines a robust, biologically-inspired routing mechanism that fulfills the goal of an emergent, learning system. The separation of the `AffinityGraph` (state/learning) from the `RoutingTable` (execution) is a strong architectural choice. However, there are a few areas concerning data structures, borrowing patterns, and semantic clarity that should be addressed before or during implementation to prevent friction.

---

## 1. Data Structures & Types

### 1.1 `PortId` Representation
- **Finding**: `EdgeId` is defined as `(ModuleId, PortId, ModuleId, PortId)`. However, in L0-S01, ports are identified by `PortName` (which is an alias for `String`).
- **Risk**: Using `String` components in a tuple that acts as a `HashMap` key for frequent lookups (potentially multiple times per tick per active edge) introduces unnecessary hashing overhead and memory allocation.
- **Recommendation**: Define `PortId` as a lightweight numeric identifier (e.g., `u32` representing an index, or a hash of the name if uniqueness is guaranteed) rather than a raw `String`. Alternatively, use a string interning strategy, but numeric indices assigned by the `SeedReactor` or `ModuleSchema` upon registration are typically fastest.

### 1.2 `ModuleCore` vs `Module` Trait Name
- **Finding**: The `SeedReactor` struct definition uses `Box<dyn ModuleCore>`, but the trait defined in L0-S01 is named `Module`.
- **Recommendation**: Standardize the naming to `Box<dyn Module>` to match the L0-S01 contract, assuming `ModuleCore` is a typo in the specification.

## 2. Borrowing & Execution Flow

### 2.1 The Routing Tick Cycle Borrowing
- **Finding**: The `SeedReactor` owns all modules in a `HashMap`. The cycle describes modules emitting signals, and the reactor routing them to receiving modules.
- **Risk**: Rust's borrow checker will prevent simultaneous mutable borrows of the module map. You cannot mutably iterate over the map to call `emit_signals` while simultaneously mutably borrowing specific receiving modules to call `receive_signal`.
- **Recommendation**: Ensure the implementation explicitly separates the phases:
  1. **Phase 1 (Collection)**: Iterate over all modules (can be mutable if `emit_signals` requires `&mut self` as updated in L0-S01), collect all generated signals into a centralized temporary buffer (e.g., `Vec<(ModuleId, PortName, Signal)>`).
  2. **Phase 2 (Routing & Delivery)**: Iterate through the collected buffer, consult the `RoutingTable`, and then mutably borrow the specific target modules one-by-one to call `receive_signal`.

## 3. Semantic Clarity & Logic

### 3.1 Softmax Routing vs. Multi-cast Scaling
- **Finding**: The spec mentions "Softmax normalize per output port". Softmax yields a probability distribution summing to 1.0. It is ambiguous whether a signal emitted from an output port is:
  a) **Probabilistically Routed**: Sent to exactly *one* destination chosen based on the softmax probabilities.
  b) **Multi-cast and Scaled**: Sent to *all* connected destinations, but the receiving module or the edge logic uses the softmax weight to scale the signal's impact or magnitude.
- **Recommendation**: Clarify this behavior in the implementation plan. Given the context of continuous signal flow (audio, video), multi-cast (where the signal is cloned via `Arc` and delivered to all valid targets) is more common. If it's strictly stochastic routing, specify that explicitly.

### 3.2 Routing Table Rebuild Frequency
- **Finding**: The `RoutingTable::rebuild_from_graph` method exists.
- **Risk**: If the `AffinityGraph` weights are updated continuously (Hebbian learning on every tick), rebuilding the optimized `RoutingTable` every tick might negate its performance benefits.
- **Recommendation**: The `RoutingTable` should primarily cache *topology* (which ports are connected to which). The actual *weights* might need to be looked up dynamically from the `AffinityGraph` during routing, or the `RoutingTable` should only be rebuilt when the topology changes (edges added via exploration or removed via pruning), not necessarily on every continuous weight adjustment.

---

## Conclusion

The specification is solid and ready for implementation, provided the developer keeps the Rust borrow checker constraints in mind during the routing cycle and clarifies the data type for `PortId` to avoid performance pitfalls.
