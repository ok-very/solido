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
}

/// Per-block analysis returned from DSP processing.
#[derive(Debug, Clone, Copy)]
pub struct DspAnalysis {
    pub rms: f32,
    pub peak: f32,
}
