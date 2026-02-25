pub mod effects;
pub mod envelopes;
pub mod filters;
pub mod oscillators;

/// The universal contract for a single-function DSP primitive.
///
/// Every atom wraps one FunDSP graph (or custom implementation) and exposes
/// lock-free parameter control via `set_param`/`get_param`. Atoms are `Send`
/// so they can live on the audio thread.
///
/// Audio I/O counts refer to *external* audio connections only — internal
/// `Shared`/`var` feeds don't count as audio inputs.
pub use effects::{DelayAtom, EnvFollowAtom, GateAtom, PanAtom};
pub use envelopes::{AdsrAtom, ClockAtom, LfoAtom};
pub use filters::{AllpassAtom, BandpassAtom, HighpassAtom, LowpassAtom, LowpoleAtom};
pub use oscillators::{NoiseAtom, PulseAtom, SawAtom, SineAtom, SquareAtom};

pub trait DspAtom: Send {
    /// Process `audio_inputs()` channels of input into `audio_outputs()` channels of output.
    /// Both slices have length equal to their respective channel count.
    fn tick(&mut self, input: &[f32], output: &mut [f32]);

    /// Set a named parameter. Returns `true` if the name was recognized.
    fn set_param(&mut self, name: &str, value: f32) -> bool;

    /// Get a named parameter's current value.
    fn get_param(&self, name: &str) -> Option<f32>;

    /// Number of external audio input channels (not counting Shared feeds).
    fn audio_inputs(&self) -> usize;

    /// Number of external audio output channels.
    fn audio_outputs(&self) -> usize;

    /// Reset internal state (clear delay lines, filters, envelopes).
    fn reset(&mut self);

    /// Human-readable name for debugging/UI.
    fn name(&self) -> &str;
}


/// Render `samples` frames from a 0-input atom into a Vec.
#[cfg(test)]
pub fn render_atom(atom: &mut dyn DspAtom, samples: usize) -> Vec<f32> {
    let outs = atom.audio_outputs();
    let ins = atom.audio_inputs();
    let input = vec![0.0f32; ins];
    let mut buf = Vec::with_capacity(samples * outs);
    let mut output = vec![0.0f32; outs];
    for _ in 0..samples {
        atom.tick(&input, &mut output);
        buf.extend_from_slice(&output);
    }
    buf
}

/// Render `samples` frames from a 1-input atom, feeding `input_signal`.
#[cfg(test)]
pub fn render_atom_with_input(
    atom: &mut dyn DspAtom,
    input_signal: &[f32],
) -> Vec<f32> {
    let outs = atom.audio_outputs();
    let mut buf = Vec::with_capacity(input_signal.len() * outs);
    let mut output = vec![0.0f32; outs];
    for &sample in input_signal {
        atom.tick(&[sample], &mut output);
        buf.extend_from_slice(&output);
    }
    buf
}

/// RMS of a buffer.
#[cfg(test)]
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f32 = buf.iter().map(|s| s * s).sum();
    (sum / buf.len() as f32).sqrt()
}

/// Count zero crossings in a buffer.
#[cfg(test)]
pub fn zero_crossings(buf: &[f32]) -> usize {
    buf.windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count()
}
