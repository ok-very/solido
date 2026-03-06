//! Short filtered noise burst cell for attack transients.
//!
//! On trigger (NoteOn), generates a burst of 1-pole-filtered white noise with
//! linear amplitude decay over `duration` seconds. Useful for tabla "chik" slaps,
//! hand drum attacks, and any percussive transient that needs broadband noise character.
//!
//! # Parameters
//! - `color`: Lowpass cutoff on noise (200–8000 Hz). Low = dark, high = bright.
//! - `duration`: Burst duration in seconds (0.001–0.1)
//! - `level`: Output level (0–1)
//!
//! # Trigger
//! Activated by `DspCommand::NoteOn` via a Trigger wire from logic_seq_cell.
//! Retriggering during decay restarts from the level parameter value at trigger time.
//!
//! # Wire Graph Patterns
//! - `logic_seq_cell --[Trigger]--> noise_burst_cell`
//! - `noise_burst_cell --[Audio]--> mixer_cell`

use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Short filtered noise burst for attack transients.
///
/// Triggered by NoteOn commands. Outputs 1-pole-filtered white noise with
/// linear decay over `duration` seconds. Output is centered stereo (L=R).
pub struct NoiseBurstCell {
    /// Current envelope amplitude. 0.0 = silent.
    env_level: f32,
    /// Linear amplitude decrement per sample. Computed on each NoteOn.
    env_decrement: f32,

    /// LCG noise seed — deterministic, stack-only, RT-safe.
    noise_seed: u32,
    /// 1-pole IIR lowpass filter state.
    filter_state: f32,

    color_handle: Shared,
    duration_handle: Shared,
    level_handle: Shared,

    /// Base (unmodulated) values for get_param_base().
    base_values: HashMap<String, f32>,

    sample_rate: f32,

    // Analysis accumulators (reset on read).
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl NoiseBurstCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let color = param_or(dna, "color", 2000.0).clamp(200.0, 8000.0);
        let duration = param_or(dna, "duration", 0.01).clamp(0.001, 0.1);
        let level = param_or(dna, "level", 0.5).clamp(0.0, 1.0);

        let color_handle = shared::shared(color);
        let duration_handle = shared::shared(duration);
        let level_handle = shared::shared(level);

        let mut base_values = HashMap::new();
        base_values.insert("color".into(), color);
        base_values.insert("duration".into(), duration);
        base_values.insert("level".into(), level);

        let handles = vec![
            ("color".into(), color_handle.clone()),
            ("duration".into(), duration_handle.clone()),
            ("level".into(), level_handle.clone()),
        ];

        let cell = Box::new(NoiseBurstCell {
            env_level: 0.0,
            env_decrement: 0.0,
            noise_seed: 0x12345678,
            filter_state: 0.0,
            color_handle,
            duration_handle,
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

impl DspCell for NoiseBurstCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        if output.len() < 2 {
            return;
        }

        if self.env_level <= 0.0 {
            output[0] = 0.0;
            output[1] = 0.0;
            return;
        }

        // LCG white noise — RT-safe, no allocation.
        self.noise_seed = self.noise_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let raw_noise = self.noise_seed as i32 as f32 / i32::MAX as f32;

        // 1-pole IIR lowpass: y += (x - y) * coeff
        // coeff = 1 - exp(-2π * cutoff / sr)
        // Reading color_handle each tick is an atomic load — cheap at 44100 Hz.
        let color = self.color_handle.value();
        let coeff = 1.0 - (-2.0 * std::f32::consts::PI * color / self.sample_rate).exp();
        self.filter_state += (raw_noise - self.filter_state) * coeff;

        // Apply envelope and write centered stereo (L=R — mono source, no width needed).
        let out = self.filter_state * self.env_level;
        output[0] = out;
        output[1] = out;

        // Linear decay — decrement after output to include the attack sample.
        self.env_level = (self.env_level - self.env_decrement).max(0.0);

        // Analysis accumulation.
        self.rms_acc += out * out;
        self.peak = self.peak.max(out.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { .. } => {
                // On trigger: read current level + duration and start envelope.
                let level = self.level_handle.value().clamp(0.0, 1.0);
                let duration = self.duration_handle.value().clamp(0.001, 0.1);
                let samples = (duration * self.sample_rate).max(1.0);
                self.env_level = level;
                // env_decrement: drop from `level` to 0 over `samples` samples.
                self.env_decrement = if level > 0.0 { level / samples } else { 0.0 };
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
        self.env_level = 0.0;
        self.env_decrement = 0.0;
        self.filter_state = 0.0;
        self.noise_seed = 0x12345678;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        "noise_burst_cell"
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use crate::organism::dna::CellDna;

    fn make_dna(color: f32, duration: f32, level: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("color".into(), color);
        params.insert("duration".into(), duration);
        params.insert("level".into(), level);
        CellDna {
            cell_type: "noise_burst_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    fn trigger(cell: &mut Box<dyn DspCell>) {
        cell.handle_command(&DspCommand::NoteOn { freq: 0.0, velocity: 1.0 });
    }

    #[test]
    fn noise_burst_silent_before_trigger() {
        let dna = make_dna(2000.0, 0.01, 0.5);
        let (mut cell, _) = NoiseBurstCell::new(&dna, 44100.0).unwrap();
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], 0.0);
    }

    #[test]
    fn noise_burst_produces_audio_on_trigger() {
        let dna = make_dna(2000.0, 0.01, 0.8);
        let (mut cell, _) = NoiseBurstCell::new(&dna, 44100.0).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "should produce non-zero audio after trigger");
    }

    #[test]
    fn noise_burst_returns_to_silence_after_duration() {
        let sr = 44100.0f32;
        let duration = 0.01f32;
        let dna = make_dna(2000.0, duration, 0.5);
        let (mut cell, _) = NoiseBurstCell::new(&dna, sr).unwrap();
        trigger(&mut cell);
        // Run past the full burst duration with a small buffer
        let burst_samples = (duration * sr) as usize + 200;
        let mut out = [0.0f32; 2];
        for _ in 0..burst_samples {
            cell.tick(&[], &mut out);
        }
        assert_eq!(out[0], 0.0, "burst should decay to silence after duration");
    }

    #[test]
    fn noise_burst_centered_stereo() {
        // L and R channels should be identical (centered mono source).
        let dna = make_dna(2000.0, 0.01, 0.5);
        let (mut cell, _) = NoiseBurstCell::new(&dna, 44100.0).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], out[1], "noise_burst should output centered stereo (L=R)");
    }

    #[test]
    fn noise_burst_level_zero_is_silent() {
        let dna = make_dna(2000.0, 0.01, 0.0);
        let (mut cell, _) = NoiseBurstCell::new(&dna, 44100.0).unwrap();
        trigger(&mut cell);
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert_eq!(out[0], 0.0, "level=0 should produce silence even after trigger");
    }

    #[test]
    fn noise_burst_output_channels_is_two() {
        let dna = make_dna(2000.0, 0.01, 0.5);
        let (cell, _) = NoiseBurstCell::new(&dna, 44100.0).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn noise_burst_retrigger_restarts_envelope() {
        let sr = 44100.0f32;
        let duration = 0.001f32;
        let dna = make_dna(2000.0, duration, 0.5);
        let (mut cell, _) = NoiseBurstCell::new(&dna, sr).unwrap();

        // Trigger, let decay fully.
        trigger(&mut cell);
        let burst_samples = (duration * sr) as usize + 100;
        let mut out = [0.0f32; 2];
        for _ in 0..burst_samples {
            cell.tick(&[], &mut out);
        }
        assert_eq!(out[0], 0.0, "should be silent after burst decays");

        // Retrigger should restart from silence to non-zero.
        trigger(&mut cell);
        cell.tick(&[], &mut out);
        assert!(out[0].abs() > 0.0, "retrigger should restart the burst");
    }
}
