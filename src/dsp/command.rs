/// Discrete commands sent to DSP atoms/molecules via ring buffer.
///
/// Only for events (note on/off, reset). Continuous parameters use
/// `fundsp::shared::Shared` lock-free atomics instead.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum DspCommand {
    NoteOn { freq: f32, velocity: f32 },
    NoteOff,
    Reset,
    Panic,
    /// Update global BPM — seq_cell applies tempo_ratio × this value.
    SetGlobalBpm(f32),
    /// Phase correction for seq_cell. Additive: phase += nudge, then rem_euclid(1.0).
    /// Used by S33 rhythm bridge: hard sync sends 0.0 (reset), soft sync sends small corrections.
    NudgePhase(f32),
    /// Update slew_cell rise/fall times (seconds). From gamaka slide configuration.
    SetSlewRate(f32, f32),
    /// Update LFO rate (Hz) and depth [0,1]. From gamaka vibrato configuration.
    SetLfoParams(f32, f32),
    /// Scale gravity weights (12 pitch classes) + blend factor for audio-thread quantization.
    /// Sent from OrganismModule when RagaModule delivers gravity_weights.
    SetScaleWeights([f32; 12], f32),
    /// Microtonal tuning overlay (raga, maqam, etc.) layered on top of 12-TET base.
    /// cents: position per degree [0, 1200), weights: gravity weight per degree,
    /// count: active degree count, blend: overlay strength [0, 1].
    SetMicroTuning {
        cents: [f32; MAX_MICRO_DEGREES],
        weights: [f32; MAX_MICRO_DEGREES],
        count: u8,
        blend: f32,
    },
    /// Set Turing Machine chaos level [0, 1] on seq_cell.
    /// 0 = DNA melody plays exactly, 1 = fully random (but scale-quantized).
    SetChaos(f32),
    /// Set pattern rotation offset [0, 15] on logic_seq_cell.
    SetRotation(f32),
    /// Set mutation rate [0, 1] on logic_seq_cell (random bit flip probability per cycle).
    SetMutationRate(f32),
    /// Toggle call-response listening mode.
    /// true = capture MIDI for call-response, false = pass-through only (noodling).
    SetListening(bool),
    /// Grid division as beat-fraction: 1.0=1/4, 0.5=1/8, 0.333=1/8T, 0.25=1/16, 0.0=free.
    SetGridDivision(f32),
    /// Global swing [0.0, 1.0]. 0.5=straight. Effective swing = max(global, local).
    SetGlobalSwing(f32),
    /// Rhythm sync mode: 0=none, 1=soft, 2=hard. Stored on OrganismModule.
    SetRhythmSync(u8),
    /// Tempo ratio [0.5, 2.0] for seq_cell + melodic_cell. Updates Shared handle.
    SetTempoRatio(f32),
    /// Trigger rate (Hz) override for logic_seq_cell's interval-based clock.
    SetTriggerRate(f32),
}

/// Max degrees in a microtonal overlay (7 for most ragas, up to 12).
pub const MAX_MICRO_DEGREES: usize = 12;

/// Per-block analysis returned from DSP processing.
///
/// Flows audio→control at ~43Hz via ring buffer. Stack-allocated (~36 bytes).
#[derive(Debug, Clone, Copy)]
pub struct DspAnalysis {
    pub rms: f32,
    pub peak: f32,
    /// Cell-level bridge: sequencer current pitch in Hz (0.0 if no seq_cell)
    pub seq_pitch_hz: f32,
    /// Cell-level bridge: sequencer gate state
    pub seq_gate: bool,
    /// Cell-level bridge: envelope level [0,1]
    pub env_level: f32,
    /// Cell-level bridge: energy-weighted oscillator frequency
    pub spectral_centroid: f32,
    /// Current effective chaos level [0,1] from seq_cell Turing Machine
    pub seq_chaos: f32,
    /// Current effective density [0,1] from logic_seq_cell
    pub logic_density: f32,
    /// Call-response cell state: 0=Idle, 1=Listen, 2=Respond
    pub cr_state: u8,
    /// Number of notes in captured call-response phrase
    pub cr_phrase_len: u8,
    /// Whether call-response cell is in capture-armed mode
    pub cr_listening: bool,
    /// Sequencer beat phase [0,1] for groove panel visualizer.
    pub beat_phase: f32,
    /// Current effective tempo ratio (1.0 = match global BPM).
    pub tempo_ratio: f32,
    /// Current grid division setting (f32 beat-fraction, 0.0 = free).
    pub grid_division: f32,
}

impl DspAnalysis {
    /// Convenience constructor for cell-level analysis (rms + peak only).
    /// Cell-level bridge fields default to zero/false.
    #[allow(dead_code)]
    pub fn new(rms: f32, peak: f32) -> Self {
        Self {
            rms,
            peak,
            seq_pitch_hz: 0.0,
            seq_gate: false,
            env_level: 0.0,
            spectral_centroid: 0.0,
            seq_chaos: 0.0,
            logic_density: 0.0,
            cr_state: 0,
            cr_phrase_len: 0,
            cr_listening: false,
            beat_phase: 0.0,
            tempo_ratio: 1.0,
            grid_division: 0.0,
        }
    }
}
