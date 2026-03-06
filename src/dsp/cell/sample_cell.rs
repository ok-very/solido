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
//!
//! # Trigger
//! Activated by `DspCommand::NoteOn` via a Trigger wire.
//! Retriggering restarts playback from the beginning.

use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// PCM sample playback cell with pitch shifting and amplitude decay.
///
/// Sample data is loaded at construction time. tick() only reads from the
/// pre-allocated buffer — no heap allocation on the audio thread.
pub struct SampleCell {
    /// Pre-loaded sample data (normalized f32, mono).
    /// Empty vec = no sample loaded (silent).
    sample: Vec<f32>,

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

    base_values: HashMap<String, f32>,
    sample_rate: f32,

    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl SampleCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let sample_path = string_param_or(dna, "sample_path", "");
        let tune = param_or(dna, "tune", 0.0).clamp(-12.0, 12.0);
        let decay = param_or(dna, "decay", 1.0).clamp(0.01, 5.0);
        let level = param_or(dna, "level", 0.8).clamp(0.0, 1.0);

        // Load WAV at construction time (control thread — allocation is fine here).
        let sample = if sample_path.is_empty() {
            Vec::new()
        } else {
            match load_wav_mono(sample_path) {
                Ok(data) => {
                    log::info!("sample_cell: loaded {} samples from '{sample_path}'", data.len());
                    data
                }
                Err(e) => {
                    log::warn!("sample_cell: failed to load '{sample_path}': {e} — silent");
                    Vec::new()
                }
            }
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

        let cell = Box::new(SampleCell {
            sample,
            read_pos: 0.0,
            env_level: 0.0,
            env_coeff: 0.0,
            playing: false,
            tune_handle,
            decay_handle,
            level_handle,
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

        let cell = Box::new(SampleCell {
            sample,
            read_pos: 0.0,
            env_level: 0.0,
            env_coeff: 0.0,
            playing: false,
            tune_handle,
            decay_handle,
            level_handle,
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

        if !self.playing || self.sample.is_empty() || self.env_level < 1e-6 {
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        }

        // Linear interpolation between adjacent samples for pitch-shifted playback.
        let pos = self.read_pos;
        let idx = pos as usize;

        let out = if idx + 1 < self.sample.len() {
            let frac = (pos - idx as f64) as f32;
            let s0 = self.sample[idx];
            let s1 = self.sample[idx + 1];
            s0 + (s1 - s0) * frac
        } else if idx < self.sample.len() {
            self.sample[idx]
        } else {
            // End of sample.
            self.playing = false;
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        };

        // Advance read position by playback rate (tune shifts pitch by semitones).
        let tune = self.tune_handle.value();
        let playback_rate = (2.0_f32).powf(tune / 12.0) as f64;
        self.read_pos += playback_rate;

        // Apply level and decay envelope.
        let level = self.level_handle.value();
        let out_scaled = out * self.env_level * level;
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
            DspCommand::NoteOn { .. } => {
                let decay = self.decay_handle.value().clamp(0.01, 5.0);
                self.read_pos = 0.0;
                self.env_level = 1.0;
                self.env_coeff = (-1.0 / (decay * self.sample_rate)).exp();
                self.playing = true;
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

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    fn trigger(cell: &mut Box<dyn DspCell>) {
        cell.handle_command(&DspCommand::NoteOn { freq: 60.0, velocity: 1.0 });
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
