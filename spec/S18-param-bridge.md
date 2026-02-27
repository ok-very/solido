# S18: Parameter Bridge Architecture

## Overview

Every tunable parameter in an organism's audio chain flows through a **Shared handle bridge** — a lock-free `Arc<AtomicU32>` that connects the 60Hz control thread to the 44.1kHz audio thread. This is the single architecture for all parameter control: UI sliders, affinity graph modulation, preset recall, and mutation all write to the same Shared handles that the audio graph reads.

## Data Flow

```
                    Control Thread (60Hz)                    Audio Thread (44.1kHz)
                    ─────────────────────                    ─────────────────────
 UI Slider ──┐
              ├──► Shared handle ──────────────────────────► FunDSP graph reads
 AffinityGraph┤    (Arc<AtomicU32>)                          via var(&fundsp_shared)
              ├──► cell0.osc_mix ──────────────────────────► crossfade in tick()
 Preset Load ─┤
              ├──► cell0.root_hz ──────────────────────────► saw() frequency
 Mutation ────┘
              └──► cell0.cutoff  ──────────────────────────► moog() cutoff
```

## Layers

### Layer 0: DspCell (audio thread)
Each `DspCell` implementation (e.g. `DroneBed`) owns:
- **FunDSP shareds** (`fundsp::shared::Shared`) — internal to the audio graph
- **Our Shared handles** (`crate::dsp::shared::Shared`) — exposed to the control thread

In `tick()`, the cell bridges: reads our Shared → writes FunDSP shared → processes one sample.

```rust
fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
    // Bridge control → audio
    self.freq_shared.set_value(self.root_hz_handle.value());
    let sample = self.graph.get_mono();
    output[0] = sample;
}
```

### Layer 1: CellRegistry
Factory returns `(Box<dyn DspCell>, Vec<(String, Shared)>)`.
The `Vec<(String, Shared)>` is the cell's parameter manifest — every tunable param has a name and a Shared handle.

### Layer 2: OrganismDsp
Collects handles from all cells, prefixes with cell index:
- `cell0.root_hz`, `cell0.det`, `cell0.osc_mix`, `cell0.cutoff`, `cell0.res`
- `cell1.size`, `cell1.dcy`, `cell1.mix`
- `cell0.bypass`, `cell1.bypass` (auto-added)

Returns `SharedHandles = HashMap<String, Shared>` to the control thread.

### Layer 3: OrganismModule (reactor module)
Owns `SharedHandles`. Maps reactor signals to handles:
- `pitch_hz` signal → writes `cell0.root_hz`
- Future: `brightness` signal → writes `cell0.cutoff`
- Future: `detune` signal → writes `cell0.det`

Any Shared handle can be driven by the affinity graph through learned edge weights.

### Layer 4: UI (OrganismPanelState)
Gets cloned Shared handles at construction. Sliders read/write directly.

## Rules

1. **Every tunable param gets a Shared handle.** If it can be changed at runtime, it must be in the cell's handle manifest.
2. **Naming convention:** `cell{index}.{param_name}`. Flat namespace, no nesting.
3. **No mutex, no channel for continuous params.** Shared handles are lock-free atomic reads/writes. Discrete events (NoteOn, NoteOff) use ring buffer channels.
4. **FunDSP shareds are internal.** They live inside the DspCell. The control thread never touches them. The cell's `tick()` bridges.
5. **Param ranges live in CellRegistry.** `register_ranges()` defines min/max for each param. Used by mutation clamping and future UI slider scaling.
6. **Crossfade and mixing happen in tick(), not in FunDSP graph.** When params need to combine multiple FunDSP units (e.g. osc_mix between two saws), do the math manually. FunDSP graphs handle the DSP primitives; Rust code handles the routing.

## Drone Bed Parameter Manifest

| Param      | Range         | Default | Description                          |
|------------|---------------|---------|--------------------------------------|
| root_hz    | 20 – 2000     | 110.0   | Fundamental frequency (Hz)           |
| det        | 0 – 50        | 7.0     | Detune between osc1/osc2 (cents)     |
| osc_mix    | 0 – 1         | 0.7     | Crossfade: 1=osc1, 0=osc2           |
| cutoff     | 20 – 20000    | 1200.0  | Moog filter cutoff (Hz)             |
| res        | 0 – 1         | 0.25    | Moog filter resonance                |
| lfo_rate   | 0.01 – 10     | 0.07    | LFO frequency (Hz)                  |
| lfo_depth  | 0 – 1         | 0.3     | LFO modulation depth on cutoff      |

## Reverb Cell Parameter Manifest

| Param | Range   | Default | Description             |
|-------|---------|---------|-------------------------|
| size  | 0 – 1   | 0.7     | Room size               |
| dcy   | 0 – 1   | 0.7     | Decay time              |
| damp  | 0 – 1   | 0.4     | High-frequency damping  |
| mix   | 0 – 1   | 0.4     | Dry/wet mix             |
