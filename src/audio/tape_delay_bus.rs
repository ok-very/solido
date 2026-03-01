//! Tape echo send bus with progressive HF loss per repeat.
//!
//! Mirrors the architecture of `reverb_bus`: organisms send a scaled copy of
//! their dry output here; the bus processes it as tape echo and returns wet
//! stereo back to the mix. The dry signal flows through VoiceBus unaffected.
//!
//! The delay algorithm is the same as the (now-retired) `tape_delay_cell`:
//! circular buffer + lowpole HF filter in the feedback path. Moving it to a
//! bus avoids the double-processing problem (organism → cell effect → sends to
//! reverb on already-effected signal) and gives independent control over the
//! echo return level.
//!
//! # Parameters (via `TapeDelaySendDna.params`)
//! - `time`: Delay time in seconds (0.01–2.0)
//! - `feedback`: Echo repeat level (0–0.92, hard ceiling enforced in tick to prevent runaway)
//! - `hf_damp`: High-frequency damping per echo (0 = flat, 1 = very dark)
//!
//! The bus return level is a global `Shared` handle (default 0.35), separate
//! from the per-organism send levels.

use fundsp::hacker32::*;

use crate::dsp::shared::{self, Shared};
use crate::organism::dna::TapeDelaySendDna;

/// Maximum supported delay time. Determines buffer allocation.
const MAX_DELAY_SECONDS: f32 = 2.0;

/// Amplitude below which delay output is gated to zero. ~-60 dBFS.
const NOISE_GATE: f32 = 0.001;

/// Control-thread handles for the tape delay bus.
pub struct TapeDelayBusHandles {
    /// Global wet return level (how loud the echo appears in the mix).
    pub return_level: Shared,
    /// Per-organism send levels (how much of each organism feeds into the bus).
    pub send_levels: Vec<Shared>,
    /// Delay algorithm param handles: (name, handle).
    pub params: Vec<(String, Shared)>,
}

/// Audio-thread tape delay send bus.
///
/// Sums per-organism sends, processes through a tape delay with HF loss,
/// and outputs wet echo only. Preallocated at construction — no per-tick alloc.
pub struct TapeDelayBus {
    // Preallocated stereo circular buffers
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
    return_level: Shared,
    send_levels: Vec<Shared>,

    sample_rate: f32,
}

impl TapeDelayBus {
    /// Build a tape delay bus from DNA configuration.
    ///
    /// `send_dna` provides delay params and a default send level.
    /// `organism_send_levels` sets the initial send level per organism.
    pub fn new(
        send_dna: &TapeDelaySendDna,
        organism_send_levels: &[f32],
        sr: f32,
    ) -> (Self, TapeDelayBusHandles) {
        let time = send_dna.params.get("time").copied().unwrap_or(0.375);
        let feedback = send_dna.params.get("feedback").copied().unwrap_or(0.4);
        let hf_damp = send_dna.params.get("hf_damp").copied().unwrap_or(0.5);

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
        let return_level = shared::shared(0.35);

        let send_levels: Vec<Shared> = organism_send_levels
            .iter()
            .map(|&level| shared::shared(level))
            .collect();

        let handles = TapeDelayBusHandles {
            return_level: return_level.clone(),
            send_levels: send_levels.clone(),
            params: vec![
                ("time".into(), time_handle.clone()),
                ("feedback".into(), feedback_handle.clone()),
                ("hf_damp".into(), hf_damp_handle.clone()),
            ],
        };

        let bus = Self {
            delay_l,
            delay_r,
            buffer_size,
            write_pos: 0,
            damp_l,
            damp_r,
            time_handle,
            feedback_handle,
            hf_damp_handle,
            return_level,
            send_levels,
            sample_rate: sr,
        };

        (bus, handles)
    }

    /// Process one stereo frame.
    ///
    /// `sources` contains one stereo pair per organism. Each source is scaled
    /// by the corresponding send level, summed into the delay input, and the
    /// wet echo (damped delayed signal × return level) is written to `output`.
    pub fn tick(&mut self, sources: &[[f32; 2]], output: &mut [f32; 2]) {
        // Sum organism sends
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        for (i, src) in sources.iter().enumerate() {
            if let Some(send) = self.send_levels.get(i) {
                let level = send.value();
                if level > 0.001 {
                    sum_l += src[0] * level;
                    sum_r += src[1] * level;
                }
            }
        }

        let time = self.time_handle.value().clamp(0.01, MAX_DELAY_SECONDS);
        let feedback = self.feedback_handle.value().clamp(0.0, 0.92);
        let hf_damp = self.hf_damp_handle.value().clamp(0.0, 1.0);

        // Read delayed signal
        let delay_samples = std::cmp::Ord::min(
            (time * self.sample_rate) as usize,
            self.buffer_size - 1,
        );
        let read_pos = (self.write_pos + self.buffer_size - delay_samples) % self.buffer_size;
        let delayed_l = self.delay_l[read_pos];
        let delayed_r = self.delay_r[read_pos];

        // Apply HF damping to feedback (hf_damp=0 → flat, hf_damp=1 → dark at 2kHz)
        let hf_cutoff = 20000.0 - hf_damp * 18000.0;
        let mut buf = [0.0f32];
        self.damp_l.tick(&[delayed_l, hf_cutoff], &mut buf);
        let damped_l = buf[0];
        self.damp_r.tick(&[delayed_r, hf_cutoff], &mut buf);
        let damped_r = buf[0];

        // Write: fresh input + damped feedback
        self.delay_l[self.write_pos] = sum_l + damped_l * feedback;
        self.delay_r[self.write_pos] = sum_r + damped_r * feedback;
        self.write_pos = (self.write_pos + 1) % self.buffer_size;

        // Return wet echo only (dry flows separately through VoiceBus)
        let ret = self.return_level.value();
        output[0] = if damped_l.abs() > NOISE_GATE { damped_l * ret } else { 0.0 };
        output[1] = if damped_r.abs() > NOISE_GATE { damped_r * ret } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_send_dna(time: f32, feedback: f32, hf_damp: f32) -> TapeDelaySendDna {
        let mut params = BTreeMap::new();
        params.insert("time".into(), time);
        params.insert("feedback".into(), feedback);
        params.insert("hf_damp".into(), hf_damp);
        TapeDelaySendDna { send: 0.5, params }
    }

    #[test]
    fn builds_successfully() {
        let dna = make_send_dna(0.375, 0.4, 0.5);
        let (bus, handles) = TapeDelayBus::new(&dna, &[0.5], SR);
        assert_eq!(handles.send_levels.len(), 1);
        assert!((handles.send_levels[0].value() - 0.5).abs() < 0.001);
        assert_eq!(bus.send_levels.len(), 1);
    }

    #[test]
    fn silence_in_produces_silence_out() {
        let dna = make_send_dna(0.1, 0.0, 0.0);
        let (mut bus, _) = TapeDelayBus::new(&dna, &[1.0], SR);
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            bus.tick(&[[0.0, 0.0]], &mut out);
        }
        assert!(out[0].abs() < 0.001, "Silence in → silence out: {}", out[0]);
    }

    #[test]
    fn delay_tail_persists() {
        // Send an impulse; after delay_samples, there should be an echo
        let delay_secs = 0.1;
        let delay_samples = (delay_secs * SR) as usize;
        let dna = make_send_dna(delay_secs, 0.0, 0.0);
        let (mut bus, _) = TapeDelayBus::new(&dna, &[1.0], SR);

        let mut out = [0.0f32; 2];
        let impulse_sample = 100;
        let mut before = 0.0f32;
        let mut at_delay = 0.0f32;

        for i in 0..(delay_samples + 500) {
            let input = if i == impulse_sample { 1.0f32 } else { 0.0 };
            bus.tick(&[[input, input]], &mut out);
            if i == impulse_sample + delay_samples / 2 {
                before = out[0];
            }
            if i == impulse_sample + delay_samples {
                at_delay = out[0];
            }
        }

        assert!(
            at_delay > before + 0.01,
            "Echo should appear at delay time: at={:.4}, before={:.4}",
            at_delay,
            before
        );
    }

    #[test]
    fn send_level_zero_produces_silence() {
        let dna = make_send_dna(0.05, 0.7, 0.0);
        let (mut bus, _) = TapeDelayBus::new(&dna, &[0.0], SR);
        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            bus.tick(&[[1.0, 1.0]], &mut out);
        }
        assert!(out[0].abs() < 0.001, "send=0 → silence: {}", out[0]);
    }

    #[test]
    fn return_level_scales_output() {
        let dna = make_send_dna(0.05, 0.4, 0.0);
        let (mut bus, handles) = TapeDelayBus::new(&dna, &[1.0], SR);
        let mut out = [0.0f32; 2];

        // Prime the delay
        for _ in 0..4410 {
            bus.tick(&[[0.5, 0.5]], &mut out);
        }

        // Zero the return — output should go silent
        handles.return_level.set(0.0);
        bus.tick(&[[0.5, 0.5]], &mut out);
        assert!(out[0].abs() < 0.001, "return=0 → silence: {}", out[0]);
    }

    #[test]
    fn tape_delay_feedback_clamped() {
        // Feedback value above the hard ceiling — handle says 1.2, tick should clamp to 0.92
        let dna = make_send_dna(0.1, 0.4, 0.0);
        let (mut bus, handles) = TapeDelayBus::new(&dna, &[1.0], SR);

        // Overwrite the feedback handle to an illegal value
        let feedback_handle = handles.params.iter().find(|(n, _)| n == "feedback").unwrap();
        feedback_handle.1.set(1.2);

        let mut out = [0.0f32; 2];
        for i in 0..10000 {
            bus.tick(&[[0.5, 0.5]], &mut out);
            assert!(
                out[0].abs() < 10.0,
                "Feedback clamped to 0.92 should not cause runaway at sample {}: {}",
                i, out[0]
            );
        }
    }

    #[test]
    fn no_runaway_with_max_feedback() {
        let dna = make_send_dna(0.001, 0.95, 0.0);
        let (mut bus, _) = TapeDelayBus::new(&dna, &[1.0], SR);
        let mut out = [0.0f32; 2];
        for i in 0..10000 {
            bus.tick(&[[1.0, 1.0]], &mut out);
            assert!(
                out[0].abs() < 100.0,
                "Output should not grow unbounded at sample {}: {}",
                i,
                out[0]
            );
        }
    }
}
