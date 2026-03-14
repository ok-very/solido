pub mod osc_cell;
pub mod lfo_cell;
pub mod filter_cell;
pub mod mixer_cell;
pub mod seq_cell;
pub mod env_cell;
pub mod slew_cell;
pub mod accent_env_cell;
pub mod func_gen_cell;
pub mod saw_bank_cell;
pub mod logic_seq_cell;
pub mod diode_filter_cell;
pub mod strike_voice_cell;
pub mod noise_burst_cell;
pub mod drum_voice_cell;
pub mod sample_cell;
pub mod walk_cell;
// tape_delay_cell is superseded by audio::tape_delay_bus (send bus infrastructure).
// Kept here so its unit tests still run; not registered as a buildable cell type.
pub mod tape_delay_cell;

use std::any::Any;

use fundsp::hacker32::*;
use fundsp::shared::Shared as FundspShared;

use crate::dsp::shared::Shared;

use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// A functional DSP unit with parameter interface. Runs on audio thread.
/// Owns molecules, exposes musical parameters via Shared handles.
pub trait DspCell: Send {
    /// Process one sample. Cells call their molecules' tick() internally.
    /// `input` carries audio from upstream cells via audio wires (empty if none).
    /// Output is interleaved stereo (2 channels) or mono (1 channel).
    fn tick(&mut self, input: &[f32], output: &mut [f32]);

    /// Handle a discrete command from the control thread.
    fn handle_command(&mut self, cmd: &DspCommand);

    /// Return current analysis (RMS, peak) for the control thread.
    #[allow(dead_code)]
    fn analysis(&self) -> DspAnalysis;

    /// Number of output channels (1 for mono cells, 2 for stereo).
    fn output_channels(&self) -> usize;

    /// Reset all internal state (molecules, envelopes, accumulators).
    fn reset(&mut self);

    /// Cell type name (matches CellDna.cell_type).
    fn name(&self) -> &str;

    /// Get the base (unmodulated) value for a parameter.
    ///
    /// Returns the DNA default value that was set during cell construction.
    /// Used by OrganismDsp to implement additive modulation (base + mod*gain).
    ///
    /// # Returns
    /// - `Some(f32)`: Base value if parameter exists
    /// - `None`: Parameter name not recognized
    fn get_param_base(&self, name: &str) -> Option<f32>;

    /// Valid parameter ranges for this cell type.
    /// Returns `&[(name, min, max)]`. Default: empty (no modulatable params).
    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { &[] }

    /// Downcast to concrete type for inspection/debugging.
    /// Zero-cost pointer cast — no allocation, RT-safe.
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast for inspector panels that need write access.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Build a cell from its DNA type string. Returns cell + shared handles, or None if unknown.
pub fn build_cell(
    dna: &CellDna,
    sr: f32,
) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
    match dna.cell_type.as_str() {
        "osc_cell" => osc_cell::OscCell::new(dna, sr),
        "saw_bank_cell" => saw_bank_cell::SawBankCell::new(dna, sr),
        "filter_cell" => filter_cell::FilterCell::new(dna, sr),
        "diode_filter_cell" => diode_filter_cell::DiodeFilterCell::new(dna, sr),
        "lfo_cell" => lfo_cell::LfoCell::new(dna, sr),
        "seq_cell" => seq_cell::SeqCell::new(dna, sr),
        "env_cell" => env_cell::EnvCell::new(dna, sr),
        "slew_cell" => slew_cell::SlewCell::new(dna, sr),
        "accent_env_cell" => accent_env_cell::AccentEnvCell::new(dna, sr),
        "func_gen_cell" => func_gen_cell::FuncGenCell::new(dna, sr),
        "logic_seq_cell" => logic_seq_cell::LogicSeqCell::new(dna, sr),
        "mixer_cell" => mixer_cell::MixerCell::new(dna, sr),
        "strike_voice_cell" => strike_voice_cell::StrikeVoiceCell::new(dna, sr),
        "noise_burst_cell" => noise_burst_cell::NoiseBurstCell::new(dna, sr),
        "drum_voice_cell" => drum_voice_cell::DrumVoiceCell::new(dna, sr),
        "sample_cell" => sample_cell::SampleCell::new(dna, sr),
        "walk_cell" => walk_cell::WalkCell::new(dna, sr),
        _ => None,
    }
}

/// Get the PARAM_RANGES constant for a cell type string (without building a cell instance).
/// Returns the static slice, or empty if the type is unknown.
pub fn cell_type_ranges(cell_type: &str) -> &'static [(&'static str, f32, f32)] {
    match cell_type {
        "osc_cell" => osc_cell::PARAM_RANGES,
        "saw_bank_cell" => saw_bank_cell::PARAM_RANGES,
        "filter_cell" => filter_cell::PARAM_RANGES,
        "diode_filter_cell" => diode_filter_cell::PARAM_RANGES,
        "lfo_cell" => lfo_cell::PARAM_RANGES,
        "seq_cell" => seq_cell::PARAM_RANGES,
        "env_cell" => env_cell::PARAM_RANGES,
        "slew_cell" => slew_cell::PARAM_RANGES,
        "accent_env_cell" => accent_env_cell::PARAM_RANGES,
        "func_gen_cell" => func_gen_cell::PARAM_RANGES,
        "logic_seq_cell" => logic_seq_cell::PARAM_RANGES,
        "mixer_cell" => mixer_cell::PARAM_RANGES,
        "strike_voice_cell" => strike_voice_cell::PARAM_RANGES,
        "noise_burst_cell" => noise_burst_cell::PARAM_RANGES,
        "drum_voice_cell" => drum_voice_cell::PARAM_RANGES,
        "sample_cell" => sample_cell::PARAM_RANGES,
        "walk_cell" => walk_cell::PARAM_RANGES,
        _ => &[],
    }
}

/// Look up (min, max) for a named parameter in a ranges slice. O(N) scan over ≤5 entries.
#[inline]
pub fn find_range(ranges: &[(&str, f32, f32)], name: &str) -> Option<(f32, f32)> {
    for &(n, min, max) in ranges {
        if n == name {
            return Some((min, max));
        }
    }
    None
}

/// Clamp a value to the range defined for `name`, or return it unchanged if not found.
#[inline]
pub fn clamp_param(ranges: &[(&str, f32, f32)], name: &str, value: f32) -> f32 {
    if let Some((min, max)) = find_range(ranges, name) {
        value.clamp(min, max)
    } else {
        value
    }
}

/// Helper: read a param from CellDna, returning default if missing.
pub(crate) fn param_or(dna: &CellDna, name: &str, default: f32) -> f32 {
    dna.params.get(name).copied().unwrap_or(default)
}

/// Helper: read a string param from CellDna, returning default if missing.
pub(crate) fn string_param_or<'a>(dna: &'a CellDna, name: &str, default: &'a str) -> &'a str {
    dna.string_params.get(name).map(|s| s.as_str()).unwrap_or(default)
}

/// Build an oscillator AudioUnit from wtype string and FunDSP shared frequency.
///
/// # Waveform Types
/// - `"saw"`: Maximally harmonic-rich, aggressive
/// - `"sine"`: Pure tone
/// - `"square"`: Hollow, odd harmonics
/// - `"triangle"`: Soft, odd harmonics with steep rolloff
/// - `"pulse"`: Variable pulse width (requires pw_shared)
/// - `"soft_saw"` (default): Mellow, triangle-like harmonic rolloff
///
/// The oscillator's sample rate is set to `sr` before returning.
pub(crate) fn build_osc(
    wtype: &str,
    freq_shared: &FundspShared,
    pw_shared: Option<&FundspShared>,
    sr: f32,
) -> Box<dyn AudioUnit> {
    let mut osc: Box<dyn AudioUnit> = match wtype {
        "pulse" => {
            let pw = pw_shared.expect("pulse waveform requires pw_shared parameter");
            // pulse() takes 2 inputs: frequency | pulse_width
            Box::new((var(freq_shared) | var(pw)) >> pulse())
        }
        "saw" => Box::new(var(freq_shared) >> saw()),
        "sine" => Box::new(var(freq_shared) >> sine()),
        "square" => Box::new(var(freq_shared) >> square()),
        "triangle" => Box::new(var(freq_shared) >> triangle()),
        // "soft_saw" and anything else → soft_saw (mellow default)
        _ => Box::new(var(freq_shared) >> soft_saw()),
    };
    osc.set_sample_rate(sr as f64);
    osc
}
