#![allow(dead_code)]
//! Tape echo with high-frequency loss in the feedback path.
//!
//! Simulates the character of a tape delay unit: each echo pass loses high-frequency
//! content (tape heads have limited HF bandwidth), giving a warm, progressively darker
//! echo wash. Essential for the ACID organism's spatial depth.
//!
//! # Implementation
//! Stereo circular buffer delay with a lowpole HF filter in the feedback path.
//! Buffer is preallocated at construction (RT-safe — no per-tick allocation).
//!
//! # Parameters
//! - `time`: Delay time (0.01-2.0 seconds)
//! - `feedback`: Echo repeat level (0-0.95, capped to prevent runaway)
//! - `hf_damp`: High-frequency damping per echo (0=flat, 1=very dark)
//! - `mix`: Dry/wet balance (0=dry only, 1=wet only)
//!
//! # Wire Graph Patterns
//! - Audio: diode_filter_cell → tape_delay_cell → mixer_cell

use fundsp::hacker32::*;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Maximum supported delay time in seconds. Determines buffer allocation size.
const MAX_DELAY_SECONDS: f32 = 2.0;

/// Tape echo with progressive HF loss per echo repeat.
///
/// Circular buffer is allocated once at construction. `tick()` only performs
/// index arithmetic and FunDSP lowpole calls — no allocation on the audio thread.
pub struct TapeDelayCell {
    // Preallocated stereo delay buffers (circular)
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    buffer_size: usize,
    write_pos: usize,

    // HF damping lowpass in the feedback path (one per channel)
    damp_l: Box<dyn AudioUnit>,
    damp_r: Box<dyn AudioUnit>,

    // Param handles
    time_handle: Shared,
    feedback_handle: Shared,
    hf_damp_handle: Shared,
    mix_handle: Shared,

    sample_rate: f32,
    base_values: HashMap<String, f32>,
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

impl TapeDelayCell {
    /// Build a tape_delay_cell from DNA.
    ///
    /// Allocates the circular buffer at `MAX_DELAY_SECONDS * sr` samples.
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let time = param_or(dna, "time", 0.375);
        let feedback = param_or(dna, "feedback", 0.4);
        let hf_damp = param_or(dna, "hf_damp", 0.5);
        let mix = param_or(dna, "mix", 0.3);

        // Preallocate buffer large enough for max delay at current sample rate
        let buffer_size = (MAX_DELAY_SECONDS * sr) as usize + 1;
        let delay_l = vec![0.0f32; buffer_size];
        let delay_r = vec![0.0f32; buffer_size];

        let mut damp_l: Box<dyn AudioUnit> = Box::new(lowpole());
        let mut damp_r: Box<dyn AudioUnit> = Box::new(lowpole());
        damp_l.set_sample_rate(sr as f64);
        damp_r.set_sample_rate(sr as f64);

        let time_handle = shared::shared(time);
        let feedback_handle = shared::shared(feedback);
        let hf_damp_handle = shared::shared(hf_damp);
        let mix_handle = shared::shared(mix);

        let handles = vec![
            ("time".into(), time_handle.clone()),
            ("feedback".into(), feedback_handle.clone()),
            ("hf_damp".into(), hf_damp_handle.clone()),
            ("mix".into(), mix_handle.clone()),
        ];

        let base_values = [
            ("time", time),
            ("feedback", feedback),
            ("hf_damp", hf_damp),
            ("mix", mix),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();

        let cell = Self {
            delay_l,
            delay_r,
            buffer_size,
            write_pos: 0,
            damp_l,
            damp_r,
            time_handle,
            feedback_handle,
            hf_damp_handle,
            mix_handle,
            sample_rate: sr,
            base_values,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };

        Some((Box::new(cell), handles))
    }
}

impl DspCell for TapeDelayCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        let input_l = if !input.is_empty() { input[0] } else { 0.0 };
        let input_r = if input.len() > 1 { input[1] } else { input_l };

        let time = self.time_handle.value().clamp(0.01, MAX_DELAY_SECONDS);
        // Cap feedback at 0.95 — prevents infinite buildup while allowing long tails
        let feedback = self.feedback_handle.value().clamp(0.0, 0.95);
        let hf_damp = self.hf_damp_handle.value().clamp(0.0, 1.0);
        let mix = self.mix_handle.value().clamp(0.0, 1.0);

        // Calculate read position (samples back from write head)
        let delay_samples = std::cmp::Ord::min((time * self.sample_rate) as usize, self.buffer_size - 1);
        let read_pos = (self.write_pos + self.buffer_size - delay_samples) % self.buffer_size;

        // Read the delayed signal
        let delayed_l = self.delay_l[read_pos];
        let delayed_r = self.delay_r[read_pos];

        // Apply HF damping to the delayed signal before feeding back.
        // hf_damp=0 → cutoff=20kHz (flat); hf_damp=1 → cutoff=2kHz (warm/dark).
        // Each echo pass through this filter → progressive HF loss = tape character.
        let hf_cutoff = 20000.0 - hf_damp * 18000.0;
        let mut buf = [0.0f32];
        self.damp_l.tick(&[delayed_l, hf_cutoff], &mut buf);
        let damped_l = buf[0];
        self.damp_r.tick(&[delayed_r, hf_cutoff], &mut buf);
        let damped_r = buf[0];

        // Write to delay line: fresh input + damped feedback
        self.delay_l[self.write_pos] = input_l + damped_l * feedback;
        self.delay_r[self.write_pos] = input_r + damped_r * feedback;

        self.write_pos = (self.write_pos + 1) % self.buffer_size;

        // Dry/wet mix
        let out_l = input_l * (1.0 - mix) + damped_l * mix;
        let out_r = input_r * (1.0 - mix) + damped_r * mix;

        // Analysis
        let avg = (out_l + out_r) * 0.5;
        self.rms_acc += avg * avg;
        self.peak = self.peak.max(out_l.abs()).max(out_r.abs());
        self.sample_count += 1;

        output[0] = out_l;
        if output.len() > 1 {
            output[1] = out_r;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        if self.sample_count > 0 {
            DspAnalysis::new(
                (self.rms_acc / self.sample_count as f32).sqrt(),
                self.peak,
            )
        } else {
            DspAnalysis::new(0.0, 0.0)
        }
    }

    fn output_channels(&self) -> usize { 2 }

    fn reset(&mut self) {
        self.delay_l.iter_mut().for_each(|s| *s = 0.0);
        self.delay_r.iter_mut().for_each(|s| *s = 0.0);
        self.write_pos = 0;
        self.damp_l.reset();
        self.damp_r.reset();
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str { "tape_delay_cell" }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna(time: f32, feedback: f32, hf_damp: f32, mix: f32) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("time".into(), time);
        params.insert("feedback".into(), feedback);
        params.insert("hf_damp".into(), hf_damp);
        params.insert("mix".into(), mix);
        CellDna {
            cell_type: "tape_delay_cell".into(),
            params,
            string_params: BTreeMap::new(),
        }
    }

    fn rms(signal: &[f32]) -> f32 {
        let sum: f32 = signal.iter().map(|s| s * s).sum();
        (sum / signal.len() as f32).sqrt()
    }

    #[test]
    fn tape_delays_signal() {
        // Output should contain a delayed copy of the input
        let delay_secs = 0.1;
        let delay_samples = (delay_secs * SR) as usize;

        let dna = make_dna(delay_secs, 0.0, 0.0, 1.0); // Mix=1.0: wet only
        let (mut cell, _) = TapeDelayCell::new(&dna, SR).unwrap();

        let mut out = [0.0f32; 2];
        let impulse_sample = 100; // Input impulse at sample 100
        let mut output_before_delay = 0.0f32;
        let mut output_at_delay = 0.0f32;

        for i in 0..(delay_samples + 500) {
            let input = if i == impulse_sample { 1.0f32 } else { 0.0 };
            cell.tick(&[input, input], &mut out);

            if i == impulse_sample + delay_samples / 2 {
                output_before_delay = out[0];
            }
            if i == impulse_sample + delay_samples {
                output_at_delay = out[0];
            }
        }

        assert!(
            output_at_delay > output_before_delay + 0.01,
            "Delayed impulse should appear at expected sample: at_delay={:.4}, before={:.4}",
            output_at_delay,
            output_before_delay
        );
    }

    #[test]
    fn tape_feedback_repeats() {
        // With feedback > 0, multiple echoes should appear
        let dna = make_dna(0.05, 0.7, 0.0, 1.0); // 0.05s delay, 70% feedback
        let (mut cell, _) = TapeDelayCell::new(&dna, SR).unwrap();

        let delay_samples = (0.05 * SR) as usize;
        let mut out = [0.0f32; 2];

        // Collect output for 5 echo repeats
        let total = delay_samples * 6;
        let mut outputs = vec![0.0f32; total];
        for i in 0..total {
            let input = if i == 10 { 1.0f32 } else { 0.0 };
            cell.tick(&[input, input], &mut out);
            outputs[i] = out[0];
        }

        // First echo
        let first_echo = outputs[delay_samples + 10];
        // Second echo (should be weaker due to feedback < 1)
        let second_echo = outputs[delay_samples * 2 + 10];

        assert!(first_echo > 0.1, "First echo should be audible: {:.4}", first_echo);
        assert!(
            second_echo > 0.01,
            "Second echo should exist with feedback=0.7: {:.4}",
            second_echo
        );
        assert!(
            first_echo > second_echo,
            "Each echo should be quieter than previous: first={:.4}, second={:.4}",
            first_echo,
            second_echo
        );
    }

    #[test]
    fn tape_hf_damp_darkens() {
        // High hf_damp should reduce high-frequency content in echoes
        let dna_bright = make_dna(0.05, 0.9, 0.0, 1.0);
        let dna_dark = make_dna(0.05, 0.9, 1.0, 1.0);
        let (mut bright, _) = TapeDelayCell::new(&dna_bright, SR).unwrap();
        let (mut dark, _) = TapeDelayCell::new(&dna_dark, SR).unwrap();

        // Feed a high-frequency signal (4kHz) for a while
        let mut out = [0.0f32; 2];
        let mut bright_outputs = vec![];
        let mut dark_outputs = vec![];

        let delay_samples = (0.05 * SR) as usize;

        for i in 0..(delay_samples * 4) {
            let input = if i < delay_samples {
                let phase = (i as f32 / SR) * 4000.0 * std::f32::consts::TAU;
                phase.sin() * 0.5
            } else {
                0.0
            };

            bright.tick(&[input, input], &mut out);
            if i >= delay_samples {
                bright_outputs.push(out[0]);
            }

            dark.tick(&[input, input], &mut out);
            if i >= delay_samples {
                dark_outputs.push(out[0]);
            }
        }

        let rms_bright = rms(&bright_outputs);
        let rms_dark = rms(&dark_outputs);

        assert!(
            rms_bright > rms_dark,
            "Bright delay should preserve more HF: bright={:.4}, dark={:.4}",
            rms_bright,
            rms_dark
        );
    }

    #[test]
    fn tape_mix_blends() {
        // mix=0 → dry only, mix=1 → wet only
        let dna_dry = make_dna(0.05, 0.0, 0.0, 0.0);
        let dna_wet = make_dna(0.05, 0.0, 0.0, 1.0);
        let (mut dry_cell, _) = TapeDelayCell::new(&dna_dry, SR).unwrap();
        let (mut wet_cell, _) = TapeDelayCell::new(&dna_wet, SR).unwrap();

        let delay_samples = (0.05 * SR) as usize;
        let mut out = [0.0f32; 2];

        // Feed a constant signal for longer than delay
        let mut dry_sum = 0.0f32;
        let mut wet_sum = 0.0f32;
        for i in 0..(delay_samples * 2) {
            let s = 0.5f32;
            dry_cell.tick(&[s, s], &mut out);
            if i > delay_samples {
                dry_sum += out[0].abs();
            }
            wet_cell.tick(&[s, s], &mut out);
            if i > delay_samples {
                wet_sum += out[0].abs();
            }
        }

        assert!(dry_sum > 0.0, "Dry mix should produce output");
        assert!(wet_sum > 0.0, "Wet mix should produce output (echo arrived)");
    }

    #[test]
    fn tape_no_runaway() {
        // Even with extreme input, feedback capped at 0.95 should prevent infinite buildup
        let dna = make_dna(0.001, 0.95, 0.0, 1.0); // Very short delay, max feedback
        let (mut cell, _) = TapeDelayCell::new(&dna, SR).unwrap();

        let mut out = [0.0f32; 2];
        for i in 0..10000 {
            // Constant loud input
            let s = 1.0f32;
            cell.tick(&[s, s], &mut out);
            assert!(
                out[0].abs() < 100.0,
                "Output should not grow unbounded at sample {}: {}",
                i,
                out[0]
            );
        }
    }
}
