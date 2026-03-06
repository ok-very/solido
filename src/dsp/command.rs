/// Discrete commands sent to DSP atoms/molecules via ring buffer.
///
/// Only for events (note on/off, reset). Continuous parameters use
/// `fundsp::shared::Shared` lock-free atomics instead.
#[derive(Debug, Clone, Copy)]
pub enum DspCommand {
    NoteOn { freq: f32, velocity: f32 },
    NoteOff,
    Reset,
    Panic,
    /// Update global BPM — seq_cell applies tempo_ratio × this value.
    SetGlobalBpm(f32),
}

/// Per-block analysis returned from DSP processing.
///
/// Flows audio→control at ~43Hz via ring buffer. Stack-allocated (~28 bytes).
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
}

impl DspAnalysis {
    /// Convenience constructor for cell-level analysis (rms + peak only).
    /// Cell-level bridge fields default to zero/false.
    pub fn new(rms: f32, peak: f32) -> Self {
        Self {
            rms,
            peak,
            seq_pitch_hz: 0.0,
            seq_gate: false,
            env_level: 0.0,
            spectral_centroid: 0.0,
        }
    }
}
