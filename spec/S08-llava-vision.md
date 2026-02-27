# L2-S08 — LLaVA Vision Module

> The system thinks about what it sees.

## Goal

Build a multimodal vision LLM as a Module. LLaVAModule receives FrameRef
signals, runs inference on a dedicated thread, and emits analysis signals
(confidence, perplexity, embedding, description, detected objects) back
through the affinity graph. This is the slowest timescale (~2-10Hz) but
produces the richest semantic information.

## Ancestry

The roadmap.md envisions candle-based Phi-2 Q4 inference producing
LlmSignals (confidence, perplexity, embedding, token_rate). The
addendum specified: GPU split — wgpu on discrete NVIDIA, candle on CPU.
LLM signal extraction feeds directly into the gravity system.

The user's vision: "long analysis through LLaVAs" — LLaVA processes
frames slowly but deeply, and the affinity system naturally handles
the throughput mismatch.

## Depends On

- L0-S01 (Module trait, Signal types — FrameRef, Embedding, Text)
- L1-S02 (SeedReactor)
- L2-S07 (CameraModule — source of FrameRef signals)

Can run in parallel with S04-S06 (tuning/gravity).

## Tasks

### 8.1 Create `src/substrate/llm.rs`

LLM inference on a dedicated thread:

```rust
pub struct LlmThread {
    tx_request: ringbuf::Producer<LlmRequest>,
    rx_response: ringbuf::Consumer<LlmResponse>,
    running: Arc<AtomicBool>,
}

pub struct LlmRequest {
    pub frame: Arc<FrameBuffer>,
    pub prompt: String,
    pub request_id: u64,
}

pub struct LlmResponse {
    pub request_id: u64,
    pub confidence: f32,
    pub perplexity: f32,
    pub embedding: [f32; 4],
    pub description: String,
    pub objects: Vec<f32>,  // detected object pattern
    pub inference_ms: u32,
}
```

**Backend options** (feature-gated):
- `candle` — pure Rust, runs on CPU or CUDA
- `llama-cpp` — via bindings, wider model support
- `stub` — returns random values for testing without a model

The default build uses the stub backend. Real inference requires
a feature flag: `cargo run --features llm-candle`.

### 8.2 Create `src/modules/llava_module.rs`

```rust
pub struct LLaVAModule {
    schema: ModuleSchema,
    llm_thread: Option<LlmThread>,
    last_response: Option<LlmResponse>,
    frames_received: u64,
    frames_processed: u64,
    current_prompt: String,
}
```

**Schema**:
- Inputs:
  - `frame` (FrameRef, Block) — from CameraModule or VideoFileModule
  - `prompt` (Text, Event) — analysis prompt to send with frame
- Outputs:
  - `confidence` (Float, Block) — model confidence [0, 1]
  - `perplexity` (Float, Block) — token perplexity (surprise measure)
  - `embedding` (Embedding, Block) — 4D projected vector
  - `description` (Text, Block) — natural language description
  - `objects` (Pattern, Block) — detected object pattern

**Throughput handling**: LLaVA receives frames every tick but processes
them slowly (~100-500ms per frame). The module:
1. Accepts every incoming FrameRef
2. Sends only the latest frame to the LLM thread (skip frames)
3. Emits the most recent LlmResponse on every tick
4. The affinity edge weight naturally adjusts to actual throughput

**Custom UI panel**:
- Model selector (if multiple models available)
- Prompt text input
- Inference time display
- Last description text
- Confidence/perplexity gauges
- Enable/disable toggle (LLM is expensive)

### 8.3 LLM signals → gravity system

Key routing paths through the affinity graph:
- `perplexity` → QuantizerModule.gravity_override — high perplexity
  (visual surprise) → lower pitch gravity → more microtonal drift
- `confidence` → RagaModule.arousal — high confidence → stable raga
- `embedding` → visual output modules → drives thermal palette
- `description` → tool glyph module → text overlay on blobs

These connections form through Hebbian learning, not hardwiring.
The affinity graph discovers which LLM outputs usefully drive
which parameters.

### 8.4 Add dependency (feature-gated)

```toml
[features]
llm-candle = ["candle-core", "candle-transformers"]
llm-stub = []
default = ["llm-stub"]

[dependencies]
candle-core = { version = "0.8", optional = true }
candle-transformers = { version = "0.8", optional = true }
```

## Files Created

```
src/substrate/llm.rs              — LlmThread, LlmRequest, LlmResponse
src/modules/llava_module.rs       — LLaVAModule (Module impl)
```

## Files Modified

```
src/substrate/mod.rs              — add pub mod llm;
src/modules/mod.rs                — add pub mod llava_module;
src/app.rs                        — register LLaVAModule with SeedReactor
Cargo.toml                        — add candle (optional), feature flags
```

## Verification

1. `cargo run` (stub mode) — LLaVA module registers, emits random values
2. Camera frame → LLaVA → description text appears in debug log
3. Confidence/perplexity values change each inference cycle
4. Embedding values route through reactor to visual modules
5. With `--features llm-candle`: real model loads, real descriptions generated
6. Inference time displayed in UI: ~200-500ms per frame typical
7. Frame skipping works: only latest frame sent to LLM thread
8. Affinity edges form: camera→llava strengthens, llava→quantizer forms
9. High perplexity → audible pitch drift (if wired to gravity)
10. No memory leaks from frame accumulation in LLM pipeline

## Design Notes

The LLM thread is the slowest component in the system. The three
timescales from the roadmap:
- Audio rate: 44.1kHz (audio thread)
- Block rate: ~60Hz (UI/control thread, SeedReactor tick)
- LLM rate: ~2-10Hz (LLM inference thread)

The affinity system handles this naturally — the edge from camera to
LLaVA has low throughput (goodput reflects actual delivery rate), and
the LLaVA module's emotion reflects whether it's getting enough frames
(homeostatic activity tracking).
