//! PCM sample playback cell.
//!
//! Loads a WAV file at construction time (control thread — safe). Triggered
//! samples play back as a pre-allocated `Vec<f32>` buffer — RT-safe, lock-free.
//!
//! When no file is provided or the file cannot be read, the cell is valid but
//! remains silent. This allows DNA presets with `sample_path: ""` to compile
//! and be tested without requiring actual audio files.
//!
//! # Parameters
//! - `tune`: Playback rate offset in semitones (-12–12). Positive = faster/higher.
//! - `decay`: Amplitude decay time in seconds (0.01–5.0).
//! - `level`: Output level (0–1).
//!
//! # String Parameters
//! - `sample_path`: Relative path to a WAV file. Empty string = silent.
//! - `sample_uri`: Registry URI (e.g., `"uiowa:marimba:c4:mf"`). Takes priority over `sample_path`.
//!
//! # Trigger
//! Activated by `DspCommand::NoteOn` via a Trigger wire.
//! Retriggering restarts playback from the beginning.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::dsp::cell::{param_or, string_param_or, DspCell};

/// Fade-in/out ramp length in samples (~1.5 ms at 44.1 kHz).
/// Eliminates clicks on note start (zero-crossing jump) and gate-off.
const FADE_SAMPLES: u32 = 64;

/// Valid parameter ranges for sample_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("tune", -24.0, 24.0),
    ("decay", 0.001, 30.0),
    ("level", 0.0, 1.0),
    ("base_freq", 20.0, 20000.0),
];
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;
use crate::samples::SampleRegistry;

/// Sample data storage — either owned or shared via Arc.
/// Both variants deref to `[f32]` — tick code is identical.
enum SampleData {
    /// Loaded from file (legacy path).
    Owned(Vec<f32>),
    /// From SampleRegistry (shared across organisms).
    Shared(Arc<Vec<f32>>),
}

impl SampleData {
    fn as_slice(&self) -> &[f32] {
        match self {
            SampleData::Owned(v) => v,
            SampleData::Shared(a) => a,
        }
    }

    fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }
}

impl std::ops::Index<usize> for SampleData {
    type Output = f32;
    fn index(&self, idx: usize) -> &f32 {
        &self.as_slice()[idx]
    }
}

/// Multi-sample bank — stores multiple samples at different root pitches.
/// On NoteOn, selects nearest sample and computes minimal semitone offset.
/// RT-safe: selection is O(log N) binary search, no allocation.
struct SampleBank {
    /// (root_hz, sample_data, start_offset) sorted by root_hz.
    entries: Vec<(f32, SampleData, f64)>,
}

impl SampleBank {
    /// Build from registry data: vec of (root_hz, arc_sample).
    fn from_entries(raw: Vec<(f32, Arc<Vec<f32>>)>) -> Self {
        let entries = raw
            .into_iter()
            .map(|(hz, arc)| {
                let offset = find_zero_crossing(&arc);
                (hz, SampleData::Shared(arc), offset)
            })
            .collect();
        SampleBank { entries }
    }

    /// Select nearest sample for target Hz. Returns (index, semitone_offset).
    /// Binary search by log-frequency for perceptually correct nearest.
    fn select(&self, target_hz: f32) -> (usize, f32) {
        if self.entries.is_empty() {
            return (0, 0.0);
        }
        if self.entries.len() == 1 {
            let offset = 12.0 * (target_hz / self.entries[0].0).log2();
            return (0, offset);
        }

        let target_log = target_hz.log2();

        // Binary search for insertion point
        let mut lo = 0usize;
        let mut hi = self.entries.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.entries[mid].0.log2() < target_log {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        // Compare with neighbors
        let idx = if lo == 0 {
            0
        } else if lo >= self.entries.len() {
            self.entries.len() - 1
        } else {
            let dist_lo = (self.entries[lo - 1].0.log2() - target_log).abs();
            let dist_hi = (self.entries[lo].0.log2() - target_log).abs();
            if dist_lo <= dist_hi { lo - 1 } else { lo }
        };

        let offset = 12.0 * (target_hz / self.entries[idx].0).log2();
        (idx, offset)
    }
}

/// PCM sample playback cell with pitch shifting and amplitude decay.
///
/// Sample data is loaded at construction time. tick() only reads from the
/// pre-allocated buffer — no heap allocation on the audio thread.
pub struct SampleCell {
    /// Pre-loaded sample data (normalized f32, mono).
    /// Empty = no sample loaded (silent).
    sample: SampleData,

    /// Multi-sample bank — when present, NoteOn selects nearest sample.
    bank: Option<SampleBank>,
    /// Currently active bank entry index.
    active_bank_idx: usize,

    /// Current read position (fractional sample index).
    read_pos: f64,

    /// Amplitude decay envelope.
    env_level: f32,
    /// Per-sample exponential decay coefficient.
    env_coeff: f32,

    /// Whether a sample is currently playing.
    playing: bool,

    tune_handle: Shared,
    decay_handle: Shared,
    level_handle: Shared,

    /// Root pitch of the loaded sample (Hz). Used to convert NoteOn freq to semitone offset.
    base_freq: f32,
    /// Semitone offset computed from last NoteOn freq relative to base_freq.
    note_tune: f32,

    /// Sample offset of first zero crossing — avoids DC-offset click at attack.
    start_offset: f64,
    /// Fade-in ramp counter: counts down from FADE_SAMPLES on NoteOn.
    fade_in_remaining: u32,
    /// Fade-out ramp counter: counts down from FADE_SAMPLES on NoteOff.
    fade_out_remaining: u32,

    base_values: HashMap<String, f32>,
    sample_rate: f32,

    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

/// Find the first zero crossing in the sample buffer (within first 512 samples).
/// Returns fractional sample offset via linear interpolation.
/// If no crossing found, returns 0.0 (start at beginning).
fn find_zero_crossing(samples: &[f32]) -> f64 {
    let search_len = samples.len().min(512);
    for i in 1..search_len {
        let s0 = samples[i - 1];
        let s1 = samples[i];
        // Sign change: positive→negative or negative→positive
        if (s0 > 0.0 && s1 <= 0.0) || (s0 < 0.0 && s1 >= 0.0) {
            let denom = s0 - s1;
            if denom.abs() > 1e-8 {
                let t = s0 / denom;
                return (i - 1) as f64 + t as f64;
            }
            return i as f64;
        }
    }
    0.0
}

impl SampleCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        Self::new_inner(dna, sr, None)
    }

    /// Construct with optional SampleRegistry for URI-based sample loading.
    pub fn new_with_registry(
        dna: &CellDna,
        sr: f32,
        registry: &mut SampleRegistry,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        Self::new_inner(dna, sr, Some(registry))
    }

    fn new_inner(
        dna: &CellDna,
        sr: f32,
        registry: Option<&mut SampleRegistry>,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let sample_uri = string_param_or(dna, "sample_uri", "");
        let sample_path = string_param_or(dna, "sample_path", "");
        let tune = param_or(dna, "tune", 0.0).clamp(-12.0, 12.0);
        let decay = param_or(dna, "decay", 1.0).clamp(0.01, 5.0);
        let level = param_or(dna, "level", 0.8).clamp(0.0, 1.0);
        let base_freq = param_or(dna, "base_freq", 261.6).clamp(20.0, 20000.0);

        // Load sample: URI (via registry) takes priority over file path.
        // Instrument-level URIs (2-3 parts) build a multi-sample bank.
        let mut bank: Option<SampleBank> = None;

        let sample = if !sample_uri.is_empty() {
            if let Some(reg) = registry {
                if SampleRegistry::is_instrument_uri(sample_uri) {
                    // Instrument-level URI: load all available notes
                    let parts: Vec<&str> = sample_uri.split(':').collect();
                    let collection = parts[0];
                    let instrument = parts[1];
                    let dynamic = if parts.len() >= 3 { parts[2] } else { "mf" };

                    let entries = reg.load_instrument(collection, instrument, dynamic);
                    if entries.is_empty() {
                        log::warn!("sample_cell: instrument URI '{sample_uri}' found 0 samples — silent");
                        SampleData::Owned(Vec::new())
                    } else {
                        // Use first sample as default
                        let first = SampleData::Shared(Arc::clone(&entries[0].1));
                        bank = Some(SampleBank::from_entries(entries));
                        first
                    }
                } else {
                    // Single-note URI (4 parts) — legacy path
                    match reg.load(sample_uri) {
                        Some(arc) => SampleData::Shared(arc),
                        None => {
                            log::warn!("sample_cell: URI '{sample_uri}' not resolved — silent");
                            SampleData::Owned(Vec::new())
                        }
                    }
                }
            } else {
                log::warn!("sample_cell: URI '{sample_uri}' but no registry — silent");
                SampleData::Owned(Vec::new())
            }
        } else if !sample_path.is_empty() {
            match load_wav_mono(sample_path) {
                Ok(data) => {
                    log::info!("sample_cell: loaded {} samples from '{sample_path}'", data.len());
                    SampleData::Owned(data)
                }
                Err(e) => {
                    log::warn!("sample_cell: failed to load '{sample_path}': {e} — silent");
                    SampleData::Owned(Vec::new())
                }
            }
        } else {
            SampleData::Owned(Vec::new())
        };

        let tune_handle = shared::shared(tune);
        let decay_handle = shared::shared(decay);
        let level_handle = shared::shared(level);

        let mut base_values = HashMap::new();
        base_values.insert("tune".into(), tune);
        base_values.insert("decay".into(), decay);
        base_values.insert("level".into(), level);

        let handles = vec![
            ("tune".into(), tune_handle.clone()),
            ("decay".into(), decay_handle.clone()),
            ("level".into(), level_handle.clone()),
        ];

        let start_offset = find_zero_crossing(sample.as_slice());

        let cell = Box::new(SampleCell {
            sample,
            bank,
            active_bank_idx: 0,
            read_pos: 0.0,
            env_level: 0.0,
            env_coeff: 0.0,
            playing: false,
            tune_handle,
            decay_handle,
            level_handle,
            base_freq,
            note_tune: 0.0,
            start_offset,
            fade_in_remaining: 0,
            fade_out_remaining: 0,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        });

        Some((cell, handles))
    }

    /// Create from a pre-loaded sample buffer (used in tests).
    #[cfg(test)]
    pub fn from_buffer(
        sample: Vec<f32>,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let tune_handle = shared::shared(0.0);
        let decay_handle = shared::shared(1.0);
        let level_handle = shared::shared(0.8);

        let mut base_values = HashMap::new();
        base_values.insert("tune".into(), 0.0);
        base_values.insert("decay".into(), 1.0);
        base_values.insert("level".into(), 0.8);

        let handles = vec![
            ("tune".into(), tune_handle.clone()),
            ("decay".into(), decay_handle.clone()),
            ("level".into(), level_handle.clone()),
        ];

        let start_offset = find_zero_crossing(&sample);

        let cell = Box::new(SampleCell {
            sample: SampleData::Owned(sample),
            bank: None,
            active_bank_idx: 0,
            read_pos: 0.0,
            env_level: 0.0,
            env_coeff: 0.0,
            playing: false,
            tune_handle,
            decay_handle,
            level_handle,
            base_freq: 261.6,
            note_tune: 0.0,
            start_offset,
            fade_in_remaining: 0,
            fade_out_remaining: 0,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        });

        Some((cell, handles))
    }

    /// Create from a multi-sample bank (used in tests).
    #[cfg(test)]
    pub fn from_bank(
        entries: Vec<(f32, Vec<f32>)>,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let tune_handle = shared::shared(0.0);
        let decay_handle = shared::shared(1.0);
        let level_handle = shared::shared(0.8);

        let mut base_values = HashMap::new();
        base_values.insert("tune".into(), 0.0);
        base_values.insert("decay".into(), 1.0);
        base_values.insert("level".into(), 0.8);

        let handles = vec![
            ("tune".into(), tune_handle.clone()),
            ("decay".into(), decay_handle.clone()),
            ("level".into(), level_handle.clone()),
        ];

        let bank_entries: Vec<(f32, SampleData, f64)> = entries
            .into_iter()
            .map(|(hz, data)| {
                let offset = find_zero_crossing(&data);
                (hz, SampleData::Owned(data), offset)
            })
            .collect();

        let first_sample = if !bank_entries.is_empty() {
            // Clone first entry's data for the default sample field
            match &bank_entries[0].1 {
                SampleData::Owned(v) => SampleData::Owned(v.clone()),
                SampleData::Shared(a) => SampleData::Shared(Arc::clone(a)),
            }
        } else {
            SampleData::Owned(Vec::new())
        };

        let start_offset = find_zero_crossing(first_sample.as_slice());

        let cell = Box::new(SampleCell {
            sample: first_sample,
            bank: Some(SampleBank { entries: bank_entries }),
            active_bank_idx: 0,
            read_pos: 0.0,
            env_level: 0.0,
            env_coeff: 0.0,
            playing: false,
            tune_handle,
            decay_handle,
            level_handle,
            base_freq: 261.6,
            note_tune: 0.0,
            start_offset,
            fade_in_remaining: 0,
            fade_out_remaining: 0,
            base_values,
            sample_rate: sr,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        });

        Some((cell, handles))
    }
}

/// Load a WAV file and return normalized mono f32 samples.
///
/// Stereo files are downmixed to mono. Integer formats are normalized to [-1, 1].
fn load_wav_mono(path: &str) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("open failed: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // Downmix stereo → mono if needed.
    let mono = if spec.channels == 2 {
        samples
            .chunks(2)
            .map(|c| {
                let l = c[0];
                let r = c.get(1).copied().unwrap_or(0.0);
                (l + r) * 0.5
            })
            .collect()
    } else {
        samples
    };

    Ok(mono)
}

impl DspCell for SampleCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        if output.len() < 2 {
            return;
        }

        // Fade-out completed → stop
        if self.fade_out_remaining == 0 && !self.playing {
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        }

        // Resolve active sample source: bank entry or legacy single sample
        let active_sample: &[f32] = if let Some(ref bank) = self.bank {
            if self.active_bank_idx < bank.entries.len() {
                bank.entries[self.active_bank_idx].1.as_slice()
            } else {
                self.sample.as_slice()
            }
        } else {
            self.sample.as_slice()
        };

        if !self.playing || active_sample.is_empty() || self.env_level < 1e-6 {
            output[0] = 0.0;
            output[1] = 0.0;
            self.fade_out_remaining = 0;
            return;
        }

        // Linear interpolation between adjacent samples for pitch-shifted playback.
        let pos = self.read_pos;
        let idx = pos as usize;

        let out = if idx + 1 < active_sample.len() {
            let frac = (pos - idx as f64) as f32;
            let s0 = active_sample[idx];
            let s1 = active_sample[idx + 1];
            s0 + (s1 - s0) * frac
        } else if idx < active_sample.len() {
            active_sample[idx]
        } else {
            // End of sample.
            self.playing = false;
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        };

        // Advance read position by playback rate.
        // note_tune = semitone offset from NoteOn freq; tune_handle = additional modulation.
        let total_tune = self.note_tune + self.tune_handle.value();
        let playback_rate = (2.0_f32).powf(total_tune / 12.0) as f64;
        self.read_pos += playback_rate;

        // Fade-in ramp: linear 0→1 over FADE_SAMPLES
        let mut fade = 1.0f32;
        if self.fade_in_remaining > 0 {
            fade *= 1.0 - (self.fade_in_remaining as f32 / FADE_SAMPLES as f32);
            self.fade_in_remaining -= 1;
        }

        // Fade-out ramp: linear 1→0 over FADE_SAMPLES (from NoteOff)
        if self.fade_out_remaining > 0 {
            fade *= self.fade_out_remaining as f32 / FADE_SAMPLES as f32;
            self.fade_out_remaining -= 1;
            if self.fade_out_remaining == 0 {
                self.playing = false;
            }
        }

        // Apply level, decay envelope, and fade ramp.
        let level = self.level_handle.value();
        let out_scaled = out * self.env_level * level * fade;
        self.env_level *= self.env_coeff;

        output[0] = out_scaled;
        output[1] = out_scaled;

        // Analysis.
        self.rms_acc += out_scaled * out_scaled;
        self.peak = self.peak.max(out_scaled.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { freq, velocity } => {
                if let Some(ref bank) = self.bank {
                    // Multi-sample bank: select nearest recorded pitch
                    if *freq > 20.0 && !bank.entries.is_empty() {
                        let (idx, offset) = bank.select(*freq);
                        self.active_bank_idx = idx;
                        self.note_tune = offset;
                        // Point sample/start_offset at the selected bank entry
                        self.start_offset = bank.entries[idx].2;
                    }
                } else {
                    // Legacy single-sample: compute semitone offset from base_freq
                    if *freq > 20.0 && self.base_freq > 20.0 {
                        self.note_tune = 12.0 * (*freq / self.base_freq).log2();
                    }
                }
                let decay = self.decay_handle.value().clamp(0.01, 5.0);
                self.read_pos = self.start_offset;
                self.env_level = velocity.clamp(0.05, 1.0);
                self.env_coeff = (-1.0 / (decay * self.sample_rate)).exp();
                self.playing = true;
                self.fade_in_remaining = FADE_SAMPLES;
                self.fade_out_remaining = 0; // Cancel any in-progress fade-out
            }
            DspCommand::NoteOff => {
                // Gate-off: start fade-out ramp for clean release.
                if self.playing && self.fade_out_remaining == 0 {
                    self.fade_out_remaining = FADE_SAMPLES;
                }
            }
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        let rms = if self.sample_count > 0 {
            (self.rms_acc / self.sample_count as f32).sqrt()
        } else {
            0.0
        };
        DspAnalysis::new(rms, self.peak)
    }

    fn output_channels(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.read_pos = 0.0;
        self.env_level = 0.0;
        self.env_coeff = 0.0;
        self.playing = false;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "sample_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    fn trigger(cell: &mut Box<dyn DspCell>) {
        // freq: 0.0 means "no pitch change" — plays at base sample rate.
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
    }

    /// Generate a simple sine wave buffer for testing.
    fn sine_buffer(sr: f32, freq: f32, duration_secs: f32) -> Vec<f32> {
        let n = (sr * duration_secs) as usize;
        (0..n)
            .map(|i| (i as f32 / sr * freq * std::f32::consts::TAU).sin())
            .collect()
    }

    #[test]
    fn sample_silent_before_trigger() {
        let buf = sine_buffer(SR, 440.0, 0.1);
        let (mut cell, _) = SampleCell::from_buffer(buf, SR).unwrap();
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0, "should be silent before trigger");
    }

    #[test]
    fn sample_plays_on_trigger() {
        let buf = sine_buffer(SR, 440.0, 0.5);
        let (mut cell, _) = SampleCell::from_buffer(buf, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out); // first tick: sin(0) = 0, skip it
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "should produce audio after trigger");
    }

    #[test]
    fn sample_ends_at_buffer_boundary() {
        let buf = sine_buffer(SR, 440.0, 0.01); // 441 samples
        let sample_len = buf.len();
        let (mut cell, _) = SampleCell::from_buffer(buf, SR).unwrap();
        trigger(&mut cell);

        let mut out = [0.0f32; 2];
        // Advance past end of buffer.
        for _ in 0..sample_len + 10 {
            cell.tick(&[], &mut out);
        }
        assert_eq!(out[0], 0.0, "should be silent after sample ends");
    }

    #[test]
    fn sample_retrigger_restarts() {
        let buf = sine_buffer(SR, 440.0, 0.001); // very short
        let (mut cell, _) = SampleCell::from_buffer(buf.clone(), SR).unwrap();
        // First play — exhaust the sample.
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        for _ in 0..buf.len() + 20 {
            cell.tick(&[], &mut out);
        }
        assert_eq!(out[0], 0.0, "should be silent after first play");
        // Retrigger — should restart from beginning.
        trigger(&mut cell);
        cell.tick(&[], &mut out); // first tick: sin(0) = 0, skip it
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "should produce audio on retrigger");
    }

    #[test]
    fn sample_empty_path_is_silent() {
        use std::collections::BTreeMap;
        use crate::organism::dna::CellDna;
        let dna = CellDna {
            cell_type: "sample_cell".into(),
            params: BTreeMap::new(),
            string_params: {
                let mut m = BTreeMap::new();
                m.insert("sample_path".into(), "".into());
                m
            },
        };
        let (mut cell, _) = SampleCell::new(&dna, SR).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0, "empty path should produce silence");
    }

    #[test]
    fn sample_output_channels_is_two() {
        let buf = sine_buffer(SR, 440.0, 0.1);
        let (cell, _) = SampleCell::from_buffer(buf, SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn bank_selects_nearest_sample() {
        // Bank with 3 samples: C4 (261.6), E4 (329.6), G4 (392.0)
        let c4 = sine_buffer(SR, 261.6, 0.1);
        let e4 = sine_buffer(SR, 329.6, 0.1);
        let g4 = sine_buffer(SR, 392.0, 0.1);

        let (mut cell, _) = SampleCell::from_bank(
            vec![(261.6, c4), (329.6, e4), (392.0, g4)],
            SR,
        ).unwrap();

        // NoteOn at D4 (~293.7 Hz) — equidistant in log-freq, binary search picks E4 (idx 1)
        cell.handle_command(&DspCommand::NoteOn { freq: 293.7, velocity: 1.0 });
        let sc = cell.as_any().downcast_ref::<SampleCell>().unwrap();
        assert_eq!(sc.active_bank_idx, 1, "D4 should select E4 (nearest in log-freq)");
        assert!((sc.note_tune - (-2.0)).abs() < 0.2, "D4 is ~2 semitones below E4, got {}", sc.note_tune);
    }

    #[test]
    fn bank_selects_nearest_upper() {
        let c4 = sine_buffer(SR, 261.6, 0.1);
        let e4 = sine_buffer(SR, 329.6, 0.1);
        let g4 = sine_buffer(SR, 392.0, 0.1);

        let (mut cell, _) = SampleCell::from_bank(
            vec![(261.6, c4), (329.6, e4), (392.0, g4)],
            SR,
        ).unwrap();

        // NoteOn at F4 (~349.2 Hz) should select E4 (329.6, idx 1), shift +1 semitone
        cell.handle_command(&DspCommand::NoteOn { freq: 349.2, velocity: 1.0 });
        let sc = cell.as_any().downcast_ref::<SampleCell>().unwrap();
        assert_eq!(sc.active_bank_idx, 1, "F4 should select E4 (nearest)");
        assert!((sc.note_tune - 1.0).abs() < 0.2, "F4 is ~1 semitone above E4, got {}", sc.note_tune);
    }

    #[test]
    fn bank_plays_audio_on_trigger() {
        let c4 = sine_buffer(SR, 261.6, 0.5);
        let e4 = sine_buffer(SR, 329.6, 0.5);

        let (mut cell, _) = SampleCell::from_bank(
            vec![(261.6, c4), (329.6, e4)],
            SR,
        ).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 300.0, velocity: 1.0 });
        let mut out = [0.0f32; 2];
        // Skip past fade-in
        for _ in 0..100 {
            cell.tick(&[], &mut out);
        }
        assert!(out[0].abs() > 0.0, "bank should produce audio after NoteOn");
    }

    #[test]
    fn bank_empty_is_silent() {
        let (mut cell, _) = SampleCell::from_bank(vec![], SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 1.0 });
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0, "empty bank should be silent");
    }

    #[test]
    fn sample_tune_shifts_playback_rate() {
        // At tune=+12, playback is 2× faster → reaches buffer end in half the samples.
        let buf = sine_buffer(SR, 440.0, 0.1);
        let sample_len = buf.len();

        let (mut normal, _) = SampleCell::from_buffer(buf.clone(), SR).unwrap();
        let (mut shifted, shifted_handles) = SampleCell::from_buffer(buf, SR).unwrap();

        // Set shifted tune to +12 (2× speed) via the returned Shared handle.
        let tune_h = shifted_handles
            .iter()
            .find(|(name, _)| name == "tune")
            .map(|(_, h)| h.clone())
            .unwrap();
        tune_h.set(12.0);

        trigger(&mut normal);
        trigger(&mut shifted);

        let half = sample_len / 2;
        let mut out = [0.0f32; 2];

        // Advance normal through half the buffer — should still be playing.
        for _ in 0..half {
            normal.tick(&[], &mut out);
        }
        let normal_still_playing = out[0].abs() > 0.0;

        // Reset shifted and advance the same number of samples.
        // At 2× speed, shifted should have passed the end.
        // Run half+2 to ensure we're past the end-of-buffer boundary tick.
        for _ in 0..half + 2 {
            shifted.tick(&[], &mut out);
        }
        let shifted_still_playing = out[0].abs() > 0.0;

        // Normal should still be playing, shifted should be done.
        assert!(
            normal_still_playing,
            "normal playback should still be active at halfway"
        );
        assert!(
            !shifted_still_playing,
            "2× speed playback should be silent at halfway (already finished)"
        );
    }
}
