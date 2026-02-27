# S13 — First Organisms: Three Creatures for the Petri Dish

**Layer**: L5 (Organisms)
**Depends on**: S05c (two-tier architecture), S11 (atom primitives), S12 (cell composition + DNA), S09 (visual outputs + organism sim)
**Status**: Ready (prerequisites complete)
**FunDSP**: 0.23.0

## Sub-Specs

| Spec | Organism | Personality |
|------|----------|-------------|
| [S13a](S13a-tblk-tabla-machine.md) | **TBLK** (Tabla Machine) | Aggressive loner, low stamina, liked by others |
| [S13b](S13b-dron-the-droner.md) | **DRON** (The Droner) | Warm presence, infinite stamina, others get bored |
| [S13c](S13c-melo-the-melodizer.md) | **MELO** (The Melodizer) | Novelty-hungry arpeggiator, chases filter/envelope organisms |

## Why These Three

These organisms stress-test the full composition hierarchy and give maximum coverage
across different infrastructure consumption patterns, temporal behaviors, social
dynamics, and DSP requirements.

| Dimension | TBLK | DRON | MELO |
|-----------|------|------|------|
| Temporal | Burst / silence | Continuous | Rhythmic arpeggiation |
| Pitch | Pitched percussion (fixed) | Slowly drifting | Fast discrete steps |
| Social | Aggressive loner, liked by others | Passive, boring long-term | Filter/envelope chaser |
| Energy | Low stamina, regenerates near others | Infinite stamina | Medium, feeds on movement |
| Infra needs | Keyboard triggers, cursor intensity | Audio analysis (rms), cursor position | Quantizer pitch, keyboard triggers |
| Visual | Sharp transient blob, red/orange | Large diffuse blob, cool blue/cyan | Darting small blob, green/yellow |
| DSP | noise >> resonator, impulse + comb | detuned saws, allpass diffusion | pulse/square, filter envelopes |

---

## Organism Scaffold: `OrganismModule`

Each organism is an `OrganismModule` that implements `ModuleCore` and bridges the
control thread (SeedReactor at 60Hz) to the audio thread (OrganismDsp at 44.1kHz).

```
OrganismModule (control thread, 60Hz)
├── dna: OrganismDna                    — blueprint (S12)
├── shared_handles: SharedHandles       — HashMap<String, Shared> for param control
├── cmd_tx: Sender<DspCommand>          — ring buffer to audio thread
├── analysis_rx: Receiver<DspAnalysis>  — ring buffer from audio thread
├── emotion: ModuleEmotion              — valence + arousal (drives Hebbian learning)
├── state: OrganismState                — position, velocity, lobes (S09)
└── ports: Vec<PortId>                  — registered with PortRegistry
```

The `OrganismDsp` (audio thread) is **not** owned by `OrganismModule`. It lives
inside the audio callback closure alongside the MasterBus. See "Audio Integration"
below.

### OrganismModule implements ModuleCore

```rust
impl ModuleCore for OrganismModule {
    fn schema(&self) -> ModuleSchema { /* ports from DNA */ }
    fn emit_signals(&self, buffer: &mut Vec<(PortId, Signal)>) {
        // Emit organism outputs: analysis, emotion state, position
    }
    fn receive_signal(&mut self, port: PortId, signal: &Signal) {
        // Map infra signals to shared_handles or cmd_tx:
        //   quantizer.pitch_hz → shared_handles["cell0.freq"].set(v)
        //   keyboard.note_on   → cmd_tx.try_send(DspCommand::NoteOn{...})
        //   cursor.x/y         → shared_handles["cursor_x"].set(v)
    }
    fn tick(&mut self) {
        // 1. Drain analysis_rx for DspAnalysis
        // 2. Update emotion from analysis (rms → arousal, productive signal → valence)
        // 3. Update OrganismState (position, lobes, interactions via S09)
    }
}
```

---

## Audio Integration: Replacing VoicePool

### Current State (S05 scaffolding)

```
AudioSubstrate::new() creates:
  cpal callback closure owns:
    VoicePool (8 fixed voices, AudioCommand-driven)
    MasterBus (crossover + limiters + DC block)
  Control thread owns:
    cmd_tx: Sender<AudioCommand>   (SpawnVoice, KillVoice, SetParam)
    analysis_rx: Receiver<AudioAnalysis>
```

### Target State (S13)

```
AudioSubstrate::new() creates:
  cpal callback closure owns:
    Vec<OrganismDsp>                    — one per organism
    Vec<Receiver<DspCommand>>           — one cmd channel per organism
    MasterBus                           — unchanged
    mix_buffer: [f32; 2]                — scratch for mixing
  Control thread owns (per organism):
    cmd_tx: Sender<DspCommand>          — organism-specific commands
    analysis_rx: Receiver<DspAnalysis>  — organism-specific analysis
  Shared handles:                       — lock-free, no channel needed
    HashMap<String, Shared>             — set from control, read by audio
```

### Migration Path

1. **Add `OrganismDsp` slots to AudioSubstrate** — `Vec<OrganismDsp>` alongside
   existing VoicePool. Both coexist during transition.
2. **Per-organism channels** — Each organism gets its own `(Sender<DspCommand>,
   Receiver<DspCommand>)` pair + `(Sender<DspAnalysis>, Receiver<DspAnalysis>)`.
3. **Audio callback tick loop** — For each organism: drain commands, tick per-sample,
   accumulate stereo output into mix buffer. Then sum and pass through MasterBus.
4. **Retire VoicePool** — Once all three organisms are functional, remove VoicePool
   and its AudioCommand channel. VoiceModule becomes a no-op or is removed.

### Audio Callback (target)

```rust
move |data: &mut [f32], _info| {
    let ch = channels as usize;
    let frames = data.len() / ch;

    for frame in 0..frames {
        let base = frame * ch;
        let mut mix = [0.0f32; 2];

        for (org, cmd_rx) in organisms.iter_mut().zip(cmd_channels.iter_mut()) {
            // Drain commands (NoteOn, NoteOff, Reset, Panic)
            while let Some(cmd) = cmd_rx.try_recv() {
                org.handle_command(cmd);
            }
            // Tick one sample
            let mut out = [0.0f32; 2];
            org.tick(&mut out);
            mix[0] += out[0];
            mix[1] += out[1];
        }

        data[base] = mix[0];
        if ch > 1 { data[base + 1] = mix[1]; }
    }

    // Post-process through MasterBus
    master_bus.process(data, channels);

    // Periodic analysis (unchanged)
    // ...
}
```

### Why Per-Sample in the Callback

OrganismDsp.tick() processes one sample at a time because:
- DspAtoms wrap FunDSP AudioUnit::tick() which is per-sample
- Inter-cell trigger wiring (output > 0.5 → NoteOn) must fire within the sample
- Shared handles update atomically — no batching needed

The MasterBus already processes per-sample internally (its `process()` iterates
frames). No architecture change needed.

### Shared Handles (Lock-Free Parameter Control)

```
Control thread:                          Audio thread:
  shared_handles["cell0.freq"].set(440)  →  var(&freq) inside FunDSP graph reads 440
  shared_handles["cell1.cutoff"].set(2k) →  var(&cutoff) inside filter reads 2000
```

SharedHandles naming convention: `cell{index}.{param_name}`
- `cell0.bpm` — PatternGen BPM
- `cell1.membrane_freq` — StrikeVoice membrane frequency
- Organism-level params (future): `org.gain`, `org.pan`

---

## Inter-Cell Communication

### Three Wire Types (from S12)

```rust
enum WireType {
    Audio,                              // audio routing (reserved for future)
    Trigger,                            // gate/trigger dispatch
    Modulation { target_param: String }, // param modulation (reserved for future)
}
```

### Trigger Wiring (implemented)

The primary inter-cell mechanism. Works via cell output values in the scratch buffer:

```
OrganismDsp.tick():
  1. Tick all cells, store outputs in scratch[cell_idx]
  2. For each Trigger wire (src → dst):
     if scratch[src][0] > 0.5:
       dst.handle_command(DspCommand::NoteOn { freq: 0.0, velocity: scratch[src][0] })
```

This is how:
- **PatternGen → StrikeVoice**: Clock fires → pattern step is a hit → output = velocity
  → StrikeVoice receives NoteOn → triggers percussion hit
- **Arpeggiator → TimbreVoice**: Clock fires → next arp note → output = 1.0 →
  TimbreVoice receives NoteOn → triggers synth voice with ADSR

Note: PatternGen.drain_triggers() and Arpeggiator.drain_events() exist as alternate
APIs but are **not used** by OrganismDsp — the scratch-buffer gate approach is simpler
and avoids downcasting. These methods may be useful for testing or future custom
organisms.

### Audio Wiring (reserved)

Currently all cells' audio outputs are summed at the organism level (mono center-panned
or stereo direct). No cell-to-cell audio routing exists yet. When needed:
- Audio wire would copy scratch[src] into the input buffer for dst cell's tick()
- Requires DspCell::tick() to accept input: `tick(&[f32], &mut [f32])` (2-arg form)
- Not needed for the three initial organisms

### Modulation Wiring (reserved)

ModMatrix currently runs internal LFOs and doesn't route to other cells' Shared handles.
When needed:
- Modulation wire would read scratch[src][0] and call dst's set_param()
- Or ModMatrix would be given references to target Shared handles at construction
- Not needed for the three initial organisms (ModMatrix modulates its own atoms)

---

## Implemented Prerequisites

| Prereq | Session | Status |
|--------|---------|--------|
| Atom primitives | S11 | **Complete** — 17 atoms, DspAtom trait, Shared/var |
| Molecule wiring | S11 | **Complete** — 9 molecules, Fused + Wired variants |
| Cell composition | S12 | **Complete** — DspCell trait, 7 cells, CellRegistry |
| DNA serialization | S12 | **Complete** — OrganismDna, JSON save/load, mutation |
| OrganismDsp | S12 | **Complete** — from_dna(), tick(), handle_command() |
| Visual simulation | S09 | **Complete** — OrganismState, lobe sim, interactions, blob renderer |
| Organism scaffold | S13 | **This session** — OrganismModule, audio integration |

### S12 Cell Inventory (implemented)

| Cell | Molecules Used | Organism |
|------|---------------|----------|
| `PatternGen` | ClockAtom + Bjorklund euclidean | TBLK |
| `StrikeVoice` | membrane_sim + snap_transient + body_resonance | TBLK |
| `HarmonicBed` | detuned_stack + slow_filter + stereo_spread | DRON |
| `ShimmerLayer` | SineAtom + 3x AllpassAtom + feedback delay | DRON |
| `Arpeggiator` | ClockAtom + 5 pattern modes + gate timer | MELO |
| `TimbreVoice` | osc_pair + filter_envelope + amp_envelope | MELO |
| `ModMatrix` | 3 LFOs (pwm, filter, vibrato) + EnvFollowAtom | MELO |

---

## Social Dynamics Matrix

```
         TBLK          DRON          MELO
TBLK   [self:-0.3]    weak→DRON     weak→MELO
DRON   strong→TBLK    [self:+0.3]   medium→MELO
MELO   strong→TBLK    strong→DRON   [self:0.0]
```

### Emergent Behaviors

1. **TBLK isolation cycles**: TBLK burns through stamina, goes quiet, edges weaken.
   DRON or MELO signals regen TBLK, it explodes back. (~10-30s macro rhythm)

2. **DRON background fade**: Always present but edges to it weaken unless it shifts
   harmonics. Warm substrate that others orbit.

3. **MELO synchronization**: Arp_gate output syncs with TBLK's hit_trigger, creating
   accidental polyrhythms. If combined pattern pleases both, sync edge strengthens.

4. **Timbral sympathy**: MELO's filter tracks DRON's harmonic field. When DRON shifts,
   MELO's filter follows — a learned harmonic agreement.

---

## FunDSP 0.23 Verified API Surface

Test: `src/audio/master_bus.rs::fundsp_api_surface_check()`

| Category | Function | Notes |
|----------|----------|-------|
| Oscillators | `noise()`, `pink()`, `sine_hz(f)`, `saw_hz(f)`, `square_hz(f)`, `pulse()` | pulse() is signal-input only |
| Filters | `lowpass_hz(f,q)`, `highpass_hz(f,q)`, `butterpass_hz(f)`, `resonator_hz(f,bw)`, `bell_hz(f,q,g)`, `allpass_hz(f,q)`, `lowpole_hz(f)` | |
| Effects | `delay(t)`, `feedback(node)`, `limiter(a,r)`, `limiter_stereo(a,r)`, `dcblock()`, `dcblock_hz(f)`, `declick_s(t)`, `pan(p)` | |
| Other | `dc(v)`, `follow(t)`, `envelope2(\|t,x\|...)`, `join::<UN>()` | |

---

## S13 Implementation Steps

### Step 1: OrganismModule scaffold
- Create `src/organism/module.rs` — OrganismModule struct implementing ModuleCore
- Owns OrganismDna, SharedHandles, emotion, OrganismState
- Port registration from DNA
- Signal receive → Shared handle mapping

### Step 2: Audio integration
- Modify `src/substrate/audio.rs` — add organism slots alongside VoicePool
- Per-organism DspCommand/DspAnalysis channels
- Audio callback ticks all organisms, sums output, passes through MasterBus
- VoicePool and organisms coexist initially

### Step 3: Reactor integration
- Register OrganismModule instances with SeedReactor
- AffinityGraph edges between organisms and infrastructure
- Hebbian learning on organism↔organism edges
- EmotionDna → initial ModuleEmotion

### Step 4: Visual integration
- Connect OrganismModule emotion to OrganismState (S09)
- GravityState from emotion (S09)
- OrganismRegistry tick updates positions
- Blob renderer draws organisms

### Step 5: Three organisms running
- Instantiate TBLK, DRON, MELO from DNA presets
- Verify trigger wiring (PatternGen → StrikeVoice, Arpeggiator → TimbreVoice)
- Verify continuous audio (HarmonicBed always sounds)
- Verify SharedHandles respond to infrastructure signals

### Step 6: Retire VoicePool
- Remove VoicePool from audio callback
- Remove AudioCommand, VoiceParam
- Remove VoiceModule from reactor
- MasterBus unchanged

---

## Verification Criteria (integration)

When all three organisms are running simultaneously:

- [ ] TBLK produces percussive hits when keyboard triggers arrive
- [ ] TBLK goes quiet when isolated, regenerates when other organisms signal it
- [ ] DRON produces continuous sound, slowly evolving timbre
- [ ] DRON's edges from other organisms weaken over time unless DRON shifts harmonics
- [ ] MELO produces arpeggiated patterns synced to quantizer pitch
- [ ] MELO's valence responds to pitch variety (tanks on repetition, spikes on novelty)
- [ ] AffinityGraph shows learned edge weight evolution between all three
- [ ] Ledger records Hebbian updates, exploration events, and pruning
- [ ] No infrastructure modules have emotions or learned weights
- [ ] Blob renderer shows three blobs with distinct thermal colors and sizes
- [ ] Organisms can be saved to DNA files and reloaded
- [ ] `cargo test` — all tests pass
- [ ] Audio callback processes all organisms per-sample through MasterBus

---

## Work Completed

### DSP Bug Fixes (Phase 1) — commit `7898715`

1. **snap_transient**: `dc(1.0)` → `noise()` source + `snap_decay` exponential envelope in StrikeVoice
2. **pulse() oscillator**: `square()` → `pulse()` with `pulse_width` Shared param in osc_pair
3. **body_feedback**: Wired as dry/wet blend, removed arbitrary 0.7 membrane scaling
4. **harmonic_bed**: Added 0.25 normalization for 4-voice detuned stack
5. **soft_clip**: Harsh exponential → `tanh()` smooth saturation
6. **gain staging**: Equal-power `1/sqrt(N)` scaling for cell mix, mono center pan at 0.707
7. **DNA presets**: DRON cutoff 800→2000, resonance 0.707→1.8, detune 5→12, pan_rate 0.05→0.3; MELO filter_q 0.707→1.5

### Organism Panel (Phase 2) — commit `7898715`

- Per-cell bypass via `Shared` handles in OrganismDsp (`cell{i}.bypass`)
- `OrganismPanelState` with `CellUiState` (bypass + all param handles for future sliders)
- Organism-level identity: hue swatch, mixer mute, shape_id (S12a scaffold)
- DNA icon toggle in header tabs, floating window panel

429 tests passing.
