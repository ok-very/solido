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
// tape_delay_cell is superseded by audio::tape_delay_bus (send bus infrastructure).
// Kept here so its unit tests still run; not registered in CellRegistry.
pub mod tape_delay_cell;

use std::collections::HashMap;

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
}

/// Factory function type: takes cell DNA + sample rate, returns cell + shared handles.
type CellFactory = Box<dyn Fn(&CellDna, f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)>>;

/// Registry that maps cell type strings to factory functions and param ranges.
pub struct CellRegistry {
    factories: HashMap<String, CellFactory>,
    /// Param ranges per cell type: cell_type → { param_name → (min, max) }.
    param_ranges: HashMap<String, HashMap<String, (f32, f32)>>,
}

impl CellRegistry {
    /// Create a new registry with all known cell types registered.
    pub fn new() -> Self {
        let mut reg = Self {
            factories: HashMap::new(),
            param_ranges: HashMap::new(),
        };

        // osc_cell: dual detuned oscillators with waveform selection
        reg.register("osc_cell", Box::new(|dna, sr| {
            osc_cell::OscCell::new(dna, sr)
        }));
        reg.register_ranges("osc_cell", &[
            ("freq", 20.0, 2000.0),
            ("det", 0.0, 50.0),
            ("gain", 0.0, 1.0),
            ("pw", 0.1, 0.9),
        ]);

        // filter_cell: audio filter with type selection (moog, lowpass, highpass, bandpass)
        reg.register("filter_cell", Box::new(|dna, sr| {
            filter_cell::FilterCell::new(dna, sr)
        }));
        reg.register_ranges("filter_cell", &[
            ("cutoff", 20.0, 20000.0),
            ("res", 0.0, 1.0),
        ]);

        // lfo_cell: bipolar control signal generator
        reg.register("lfo_cell", Box::new(|dna, sr| {
            lfo_cell::LfoCell::new(dna, sr)
        }));
        reg.register_ranges("lfo_cell", &[
            ("rate", 0.01, 10.0),
            ("depth", 0.0, 1.0),
        ]);

        // mixer_cell: terminal stereo mixer with gain and pan
        reg.register("mixer_cell", Box::new(|dna, sr| {
            mixer_cell::MixerCell::new(dna, sr)
        }));
        reg.register_ranges("mixer_cell", &[
            ("gain", 0.0, 1.0),
            ("pan", -1.0, 1.0),
        ]);

        // seq_cell: internal sequencer with per-step pitch/gate/accent/slide
        reg.register("seq_cell", Box::new(|dna, sr| {
            seq_cell::SeqCell::new(dna, sr)
        }));
        reg.register_ranges("seq_cell", &[
            ("bpm", 20.0, 300.0),
            ("gate_length", 0.01, 1.0),
            ("swing", 0.0, 1.0),
        ]);

        // env_cell: ADSR envelope generator
        reg.register("env_cell", Box::new(|dna, sr| {
            env_cell::EnvCell::new(dna, sr)
        }));
        reg.register_ranges("env_cell", &[
            ("attack", 0.001, 2.0),
            ("decay", 0.001, 2.0),
            ("sustain", 0.0, 1.0),
            ("release", 0.001, 5.0),
        ]);

        // slew_cell: portamento/glide
        reg.register("slew_cell", Box::new(|dna, sr| {
            slew_cell::SlewCell::new(dna, sr)
        }));
        reg.register_ranges("slew_cell", &[
            ("rise", 0.001, 2.0),
            ("fall", 0.001, 2.0),
        ]);

        // accent_env_cell: short decay envelope for accents
        reg.register("accent_env_cell", Box::new(|dna, sr| {
            accent_env_cell::AccentEnvCell::new(dna, sr)
        }));
        reg.register_ranges("accent_env_cell", &[
            ("accent_amount", 0.0, 1.0),
            ("decay", 0.01, 1.0),
        ]);

        // func_gen_cell: multi-minute control signal generator
        reg.register("func_gen_cell", Box::new(|dna, sr| {
            func_gen_cell::FuncGenCell::new(dna, sr)
        }));
        reg.register_ranges("func_gen_cell", &[
            ("period", 1.0, 600.0),
            ("depth", 0.0, 1.0),
        ]);

        // saw_bank_cell: N-voice detuned saw oscillator bank
        reg.register("saw_bank_cell", Box::new(|dna, sr| {
            saw_bank_cell::SawBankCell::new(dna, sr)
        }));
        reg.register_ranges("saw_bank_cell", &[
            ("freq", 20.0, 2000.0),
            ("voices", 1.0, 8.0),
            ("spread", 0.0, 100.0),
            ("gain", 0.0, 1.0),
        ]);

        // logic_seq_cell: algorithmic trigger pattern generator
        reg.register("logic_seq_cell", Box::new(|dna, sr| {
            logic_seq_cell::LogicSeqCell::new(dna, sr)
        }));
        reg.register_ranges("logic_seq_cell", &[
            ("rate", 0.1, 20.0),
            ("density", 0.0, 1.0),
        ]);

        // diode_filter_cell: 3-pole diode ladder filter (303 character)
        reg.register("diode_filter_cell", Box::new(|dna, sr| {
            diode_filter_cell::DiodeFilterCell::new(dna, sr)
        }));
        reg.register_ranges("diode_filter_cell", &[
            ("cutoff", 20.0, 20000.0),
            ("res", 0.0, 2.0),
            ("drive", 0.0, 1.0),
            ("env_mod", 0.0, 1.0),
        ]);

        // strike_voice_cell: resonant membrane percussion via damped sinusoidal resonators
        reg.register("strike_voice_cell", Box::new(|dna, sr| {
            strike_voice_cell::StrikeVoiceCell::new(dna, sr)
        }));
        reg.register_ranges("strike_voice_cell", &[
            ("membrane_freq", 40.0, 800.0),
            ("tension", 0.0, 1.0),
            ("decay", 0.01, 3.0),
            ("noise_mix", 0.0, 1.0),
            ("tone", 0.0, 1.0),
        ]);

        // noise_burst_cell: short filtered noise burst for attack transients
        reg.register("noise_burst_cell", Box::new(|dna, sr| {
            noise_burst_cell::NoiseBurstCell::new(dna, sr)
        }));
        reg.register_ranges("noise_burst_cell", &[
            ("color", 200.0, 8000.0),
            ("duration", 0.001, 0.1),
            ("level", 0.0, 1.0),
        ]);

        // drum_voice_cell: parametric electronic drum synthesizer (kick, snare, hat, etc.)
        reg.register("drum_voice_cell", Box::new(|dna, sr| {
            drum_voice_cell::DrumVoiceCell::new(dna, sr)
        }));
        reg.register_ranges("drum_voice_cell", &[
            ("tune", -1.0, 1.0),
            ("decay", 0.01, 3.0),
            ("tone", 0.0, 1.0),
            ("snap", 0.0, 1.0),
            ("drive", 0.0, 1.0),
        ]);

        // sample_cell: PCM sample playback with pitch shifting and amplitude decay
        reg.register("sample_cell", Box::new(|dna, sr| {
            sample_cell::SampleCell::new(dna, sr)
        }));
        reg.register_ranges("sample_cell", &[
            ("tune", -12.0, 12.0),
            ("decay", 0.01, 5.0),
            ("level", 0.0, 1.0),
        ]);

        // tape_delay_cell is no longer registered — use sends.tape_delay in DNA instead.

        reg
    }

    fn register(&mut self, name: &str, factory: CellFactory) {
        self.factories.insert(name.into(), factory);
    }

    fn register_ranges(&mut self, cell_type: &str, ranges: &[(&str, f32, f32)]) {
        let map: HashMap<String, (f32, f32)> = ranges
            .iter()
            .map(|(name, min, max)| (name.to_string(), (*min, *max)))
            .collect();
        self.param_ranges.insert(cell_type.into(), map);
    }

    /// Get the valid range for a cell param. Returns None if unknown.
    pub fn param_range(&self, cell_type: &str, param_name: &str) -> Option<(f32, f32)> {
        self.param_ranges
            .get(cell_type)
            .and_then(|m| m.get(param_name))
            .copied()
    }

    /// Build a cell from DNA. Returns cell + shared handles, or None if type is unknown.
    pub fn build(
        &self,
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let factory = self.factories.get(&dna.cell_type)?;
        factory(dna, sr)
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
