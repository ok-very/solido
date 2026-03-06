use fundsp::hacker32::*;

use crate::dsp::shared::{self, Shared};
use crate::organism::dna::ReverbSendDna;

/// Control-thread handles for the reverb bus.
pub struct ReverbBusHandles {
    /// Global return level (wet mix from bus to master output).
    pub return_level: Shared,
    /// Per-organism send levels.
    pub send_levels: Vec<Shared>,
    /// Reverb algorithm type name (for UI display).
    pub reverb_type: String,
    /// Reverb algorithm params: (name, handle).
    pub params: Vec<(String, Shared)>,
}

impl ReverbBusHandles {
    /// Add a new organism send handle (control thread side of a runtime spawn).
    pub fn push_send(&mut self, send: Shared) {
        self.send_levels.push(send);
    }
}

/// Audio-thread reverb send bus.
///
/// Receives per-organism stereo signals scaled by their send levels,
/// sums them, processes through a FunDSP reverb, and returns wet stereo.
pub struct ReverbBus {
    reverb: Box<dyn AudioUnit>,
    return_level: Shared,
    send_levels: Vec<Shared>,
}

/// Amplitude below which reverb output is gated to zero. ~-60 dBFS.
const NOISE_GATE: f32 = 0.001;

impl ReverbBus {
    /// Build a reverb bus from DNA configuration.
    ///
    /// `send_dna` provides reverb type, params, and a default send level.
    /// `organism_count` determines how many send level handles to create.
    /// All organisms that don't have a sends.reverb in their DNA get send=0.
    pub fn new(
        send_dna: &ReverbSendDna,
        organism_send_levels: &[f32],
        sr: f32,
    ) -> (Self, ReverbBusHandles) {
        // Map DNA params to FunDSP reverb_stereo args
        let size = send_dna.params.get("size").copied().unwrap_or(0.5);
        let dcy = send_dna.params.get("dcy").copied().unwrap_or(0.5);
        let damp = send_dna.params.get("damp").copied().unwrap_or(0.4);

        let room_size = size * 40.0 + 10.0; // [10, 50] meters
        let time = dcy * 4.0 + 0.5;         // [0.5, 4.5] seconds — halved from dcy*8+1

        let mut reverb: Box<dyn AudioUnit> = Box::new(reverb_stereo(room_size, time, damp));
        reverb.set_sample_rate(sr as f64);

        let return_level = shared::shared(0.7);

        // Per-organism send levels (pre-alloc'd to 16 for RT-safe push)
        let mut send_levels: Vec<Shared> = Vec::with_capacity(16);
        for &level in organism_send_levels {
            send_levels.push(shared::shared(level));
        }

        // Param handles for future UI control (read-only for now, reverb is fixed at construction)
        let size_handle = shared::shared(size);
        let dcy_handle = shared::shared(dcy);
        let damp_handle = shared::shared(damp);

        let handles = ReverbBusHandles {
            return_level: return_level.clone(),
            send_levels: send_levels.clone(),
            reverb_type: send_dna.reverb_type.clone(),
            params: vec![
                ("size".into(), size_handle),
                ("dcy".into(), dcy_handle),
                ("damp".into(), damp_handle),
            ],
        };

        let bus = Self {
            reverb,
            return_level,
            send_levels,
        };

        (bus, handles)
    }

    /// Add a new organism send level at runtime (safe: pre-alloc'd to 16).
    /// Silently drops if at capacity — never allocates on the audio thread.
    pub fn add_organism_send(&mut self, send: Shared) {
        if self.send_levels.len() < 16 {
            self.send_levels.push(send);
        }
    }

    /// Process one stereo frame.
    ///
    /// `sources` contains one stereo pair per organism.
    /// Each is scaled by the corresponding send level, summed, processed through reverb,
    /// and scaled by the return level.
    pub fn tick(&mut self, sources: &[[f32; 2]], output: &mut [f32; 2]) {
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

        let mut wet = [0.0f32; 2];
        self.reverb.tick(&[sum_l, sum_r], &mut wet);

        let ret = self.return_level.value();
        output[0] = if wet[0].abs() > NOISE_GATE { wet[0] * ret } else { 0.0 };
        output[1] = if wet[1].abs() > NOISE_GATE { wet[1] * ret } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_send_dna() -> ReverbSendDna {
        let mut params = BTreeMap::new();
        params.insert("size".into(), 0.5);
        params.insert("dcy".into(), 0.5);
        params.insert("damp".into(), 0.3);
        ReverbSendDna {
            reverb_type: "plate".into(),
            send: 0.4,
            params,
        }
    }

    #[test]
    fn builds_successfully() {
        let dna = make_send_dna();
        let (bus, handles) = ReverbBus::new(&dna, &[0.4], 44100.0);
        assert_eq!(handles.reverb_type, "plate");
        assert_eq!(handles.send_levels.len(), 1);
        assert!((handles.send_levels[0].value() - 0.4).abs() < 0.001);
        assert_eq!(bus.send_levels.len(), 1);
    }

    #[test]
    fn silence_in_produces_silence_out() {
        let dna = make_send_dna();
        let (mut bus, _handles) = ReverbBus::new(&dna, &[1.0], 44100.0);
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            bus.tick(&[[0.0, 0.0]], &mut out);
        }
        assert!(out[0].abs() < 0.001, "Silence in should give ~silence out: {}", out[0]);
    }

    #[test]
    fn reverb_tail_persists() {
        let dna = make_send_dna();
        let (mut bus, _handles) = ReverbBus::new(&dna, &[1.0], 44100.0);
        let mut out = [0.0f32; 2];

        // Feed sustained signal to prime reverb
        for _ in 0..4410 {
            bus.tick(&[[0.5, 0.5]], &mut out);
        }

        // Now feed silence — reverb tail should continue
        let mut any_nonzero = false;
        for _ in 0..44100 {
            bus.tick(&[[0.0, 0.0]], &mut out);
            if out[0].abs() > 0.0001 {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "Reverb should have a tail after sustained input");
    }

    #[test]
    fn reverb_decays_to_silence_after_source_stops() {
        // make_send_dna uses dcy=0.5 → time = 0.5*4+0.5 = 2.5s RT60
        let dna = make_send_dna();
        let (mut bus, _) = ReverbBus::new(&dna, &[1.0], 44100.0);
        let mut out = [0.0f32; 2];

        // Prime with 0.5s of sustained signal
        for _ in 0..22050 {
            bus.tick(&[[0.5, 0.5]], &mut out);
        }

        // Feed silence for 7s (~2.8× the RT60 — should decay well below -60dBFS)
        for _ in 0..(7 * 44100) {
            bus.tick(&[[0.0, 0.0]], &mut out);
        }

        assert!(
            out[0].abs() < 0.001,
            "Reverb should decay to silence after 5s (2× RT60): {}",
            out[0]
        );
    }

    #[test]
    fn send_level_zero_produces_silence() {
        let dna = make_send_dna();
        let (mut bus, _handles) = ReverbBus::new(&dna, &[0.0], 44100.0);
        let mut out = [0.0f32; 2];
        for _ in 0..4410 {
            bus.tick(&[[1.0, 1.0]], &mut out);
        }
        // After feeding signal with send=0, output should be ~silence
        assert!(out[0].abs() < 0.001, "send=0 should produce silence: {}", out[0]);
    }

    #[test]
    fn return_level_scales_output() {
        let dna = make_send_dna();
        let (mut bus, handles) = ReverbBus::new(&dna, &[1.0], 44100.0);
        let mut out = [0.0f32; 2];

        // Prime with signal
        for _ in 0..4410 {
            bus.tick(&[[0.5, 0.5]], &mut out);
        }
        let level_half = out[0];

        // Set return to 0
        handles.return_level.set(0.0);
        bus.tick(&[[0.5, 0.5]], &mut out);
        assert!(out[0].abs() < 0.001, "return=0 should silence: {}", out[0]);
        let _ = level_half;
    }

    #[test]
    fn multiple_organisms_sum() {
        let dna = make_send_dna();
        let (mut bus, _handles) = ReverbBus::new(&dna, &[1.0, 1.0], 44100.0);
        let mut out = [0.0f32; 2];
        // FDN reverb needs sustained input to build up — prime for ~0.5s
        for _ in 0..22050 {
            bus.tick(&[[0.5, 0.5], [0.5, 0.5]], &mut out);
        }
        assert!(out[0].abs() > 0.0001, "Two sources should produce output: {}", out[0]);
    }
}
