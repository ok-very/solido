# Audit Report: L0-S01 Module Contract

**Date:** 2026-02-22
**Status:** Review Required
**Component:** Core Module System & Substrates

## Executive Summary

The foundational layer for the Solido module system is structurally sound but contains several performance bottlenecks and minor design gaps that will compound as the routing backbone (L1-S02) is implemented. Addressing these now is critical to avoid expensive refactoring later.

---

## 1. Performance & Memory

### 1.1 `Module::emit_signals` Allocation
- **Finding**: The trait method `fn emit_signals(&self) -> Vec<(PortName, Signal)>` allocates a new `Vec` on every tick.
- **Risk**: High heap churn when running many modules at 60Hz.
- **Recommendation**: Shift to a visitor pattern or pre-allocated buffer:
  ```rust
  fn emit_signals(&self, signals: &mut Vec<(PortName, Signal)>);
  ```

### 1.2 Signal Data Cloning
- **Finding**: `Signal::AudioBlock(Vec<f32>)` and `Signal::Pattern(Vec<f32>)` own their data. `Signal` is `Clone`.
- **Risk**: Cloning a signal for multi-port routing copies the entire heap-allocated buffer.
- **Recommendation**: Wrap heap buffers in `Arc` (e.g., `AudioBlock(Arc<Vec<f32>>)`). This makes cloning $O(1)$ and aligns with the existing `FrameRef(Arc<FrameBuffer>)` pattern.

## 2. Type & Rate Safety

### 2.1 Port Rate Compatibility
- **Finding**: `Port::accepts()` only verifies `SignalType`.
- **Risk**: Logical errors where an `Audio` rate signal is routed to an `Llm` rate port, causing buffer overflows or processing jitter.
- **Recommendation**: Extend `accepts()` to check `PortRate` compatibility, or at least provide a warning mechanism in the routing layer.

## 3. Substrate Integrity

### 3.1 `FrameBuffer` GPU Path
- **Finding**: `gpu_texture: Option<wgpu::Texture>` was missing from the implementation.
- **Risk**: Breaks the "zero-copy" goal for video. Every frame would require a CPU readback/upload.
- **Recommendation**: Restore `pub gpu_texture: Option<Arc<wgpu::Texture>>`.

### 3.2 Channel Thread Safety
- **Finding**: Manual `unsafe impl Send` for `Sender`/`Receiver`.
- **Verification**: Redundant. `ringbuf` 0.4 `HeapProd`/`HeapCons` are `Send` if `T: Send`.
- **Recommendation**: Remove manual `unsafe` blocks to rely on compiler-verified safety.

### 3.3 Audio Hardware Support
- **Finding**: `AudioSubstrate` only supports `F32` sample format.
- **Risk**: Incompatibility with older or specialized audio hardware that defaults to `I16`.
- **Recommendation**: Implement a simple `I16` to `F32` conversion in the audio callback.

## 4. Parser Robustness

### 4.1 ISF Header Extraction
- **Finding**: `source.find("/*")` is fragile. It will fail if there are leading comments before the ISF JSON header.
- **Recommendation**: Use a more specific search pattern, such as looking for the first `/*` that contains a `{` on the next line or is followed by ISF-specific keys.

---

## Action Items for L1-S02

1. **Refactor Module Trait**: Optimize signal emission before the routing backbone starts calling it.
2. **Arc-wrap Signals**: Ensure high-bandwidth signals are shared, not copied.
3. **Restore GPU Textures**: Ensure `FrameBuffer` can hold texture handles for the upcoming renderer sessions.
