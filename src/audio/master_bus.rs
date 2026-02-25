use fundsp::audiounit::AudioUnit;
use fundsp::prelude32::*;

/// FunDSP-based master bus for post-processing the voice pool output.
///
/// Processing chain (stereo):
/// ```text
/// L/R input
///   → declick (startup fade-in)
///   → 2-band crossover at 200Hz:
///       bass:   butterpass(200)  → limiter(10ms, 100ms)
///       treble: highpass(200)    → limiter(5ms, 150ms)
///       sum
///   → limiter_stereo(10ms, 200ms)  linked stereo master limiter
///   → dcblock(10Hz)                remove DC offset
///   → output L/R
/// ```
pub struct MasterBus {
    unit: Box<dyn AudioUnit>,
}

impl MasterBus {
    pub fn new(sample_rate: f32) -> Self {
        let mono_chain = || {
            (butterpass_hz(200.0) >> limiter(0.01, 0.1))
                & (highpass_hz(200.0, 0.707) >> limiter(0.005, 0.15))
        };

        let mut graph: Box<dyn AudioUnit> = Box::new(
            (declick_s(0.01) | declick_s(0.01))
                >> (mono_chain() | mono_chain())
                >> limiter_stereo(0.01, 0.2)
                >> (dcblock_hz(10.0) | dcblock_hz(10.0)),
        );

        graph.set_sample_rate(sample_rate as f64);
        graph.allocate();

        Self { unit: graph }
    }

    /// Process interleaved stereo audio in-place.
    ///
    /// Uses per-sample `tick()` — negligible cost for a single master instance.
    pub fn process(&mut self, data: &mut [f32], channels: u16) {
        let ch = channels as usize;
        if ch == 0 {
            return;
        }
        let num_frames = data.len() / ch;

        // FunDSP tick: 2 inputs, 2 outputs (stereo)
        let mut output = [0.0f32; 2];

        for frame in 0..num_frames {
            let base = frame * ch;
            let l = data[base];
            let r = if ch > 1 { data[base + 1] } else { l };

            self.unit.tick(&[l, r], &mut output);

            data[base] = output[0];
            if ch > 1 {
                data[base + 1] = output[1];
            }
            // Fill remaining channels with left output
            for c in 2..ch {
                data[base + c] = output[0];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    #[test]
    fn master_bus_creates_without_panic() {
        let _bus = MasterBus::new(SR);
    }

    #[test]
    fn master_bus_processes_silence() {
        let mut bus = MasterBus::new(SR);
        let mut buf = vec![0.0f32; 512];
        bus.process(&mut buf, 2);
        // All samples should remain zero (or very near zero)
        assert!(
            buf.iter().all(|&s| s.abs() < 1e-6),
            "Silence in should produce silence out"
        );
    }

    #[test]
    fn master_bus_limits_loud_signal() {
        let mut bus = MasterBus::new(SR);
        // Create a very loud stereo signal (amplitude 5.0)
        let frames = 4096;
        let mut buf = vec![0.0f32; frames * 2];
        for frame in 0..frames {
            let t = frame as f32 / SR;
            let sample = 5.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            buf[frame * 2] = sample;
            buf[frame * 2 + 1] = sample;
        }

        bus.process(&mut buf, 2);

        // After limiting, peaks should be well below the input peak of 5.0
        let peak: f32 = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            peak < 2.0,
            "Limiter should tame peaks, got peak={peak}"
        );
    }

    #[test]
    fn master_bus_removes_dc() {
        let mut bus = MasterBus::new(SR);
        // Feed DC offset signal
        let frames = 8192;
        let mut buf = vec![0.0f32; frames * 2];
        for frame in 0..frames {
            buf[frame * 2] = 0.5;
            buf[frame * 2 + 1] = 0.5;
        }

        bus.process(&mut buf, 2);

        // Last samples should have DC removed (close to 0)
        let last_l = buf[(frames - 1) * 2];
        assert!(
            last_l.abs() < 0.3,
            "DC block should reduce DC offset, got {last_l}"
        );
    }

    #[test]
    fn master_bus_mono_fallback() {
        let mut bus = MasterBus::new(SR);
        let mut buf = vec![0.0f32; 256];
        // Should not panic with 1 channel
        bus.process(&mut buf, 1);
    }

    /// Compile-time verification that FunDSP 0.23 exports the functions
    /// referenced in organism DSP sketches (S13a/b/c specs).
    #[test]
    fn fundsp_api_surface_check() {
        // --- Oscillators ---
        let _noise: Box<dyn AudioUnit> = Box::new(noise());
        let _pink: Box<dyn AudioUnit> = Box::new(pink());
        let _sine: Box<dyn AudioUnit> = Box::new(sine_hz(440.0));
        let _saw: Box<dyn AudioUnit> = Box::new(saw_hz(440.0));
        let _square: Box<dyn AudioUnit> = Box::new(square_hz(440.0));
        // pulse() exists (signal-input: 2 in for freq+width), but pulse_hz() does NOT
        let _pulse: Box<dyn AudioUnit> = Box::new(pulse());

        // --- Filters ---
        let _resonator: Box<dyn AudioUnit> = Box::new(resonator_hz(440.0, 100.0));
        let _bell: Box<dyn AudioUnit> = Box::new(bell_hz(1000.0, 1.0, 6.0));
        let _lp: Box<dyn AudioUnit> = Box::new(lowpass_hz(1000.0, 0.707));
        let _hp: Box<dyn AudioUnit> = Box::new(highpass_hz(1000.0, 0.707));
        let _bp: Box<dyn AudioUnit> = Box::new(butterpass_hz(200.0));
        let _ap: Box<dyn AudioUnit> = Box::new(allpass_hz(1000.0, 0.5));
        let _onepole: Box<dyn AudioUnit> = Box::new(lowpole_hz(8000.0));

        // --- Effects ---
        let _delay: Box<dyn AudioUnit> = Box::new(delay(0.1));
        let _lim: Box<dyn AudioUnit> = Box::new(limiter(0.01, 0.1));
        let _lim_st: Box<dyn AudioUnit> = Box::new(limiter_stereo(0.01, 0.2));
        let _dcb: Box<dyn AudioUnit> = Box::new(dcblock_hz(10.0));
        let _dcb0: Box<dyn AudioUnit> = Box::new(dcblock());
        let _decl: Box<dyn AudioUnit> = Box::new(declick_s(0.01));
        let _pan: Box<dyn AudioUnit> = Box::new(pan(0.0));
        let _dc: Box<dyn AudioUnit> = Box::new(dc(1.0));

        // --- Envelope follower ---
        let _follow: Box<dyn AudioUnit> = Box::new(follow(0.01));

        // --- Envelope with time ---
        let _env: Box<dyn AudioUnit> = Box::new(envelope2(|t, _x| (-t * 10.0).exp()));

        // --- Chained DSP graph (as used in organism sketches) ---
        let _chain: Box<dyn AudioUnit> = Box::new(
            noise() >> resonator_hz(180.0, 50.0) >> limiter(0.002, 0.05)
        );

        // --- Feedback delay (loopback node must be An<_>, not raw float) ---
        let _fb: Box<dyn AudioUnit> = Box::new(
            sine_hz(440.0) >> feedback2(delay(0.005), delay(0.005) * dc(0.4))
        );

        // --- Also verify feedback (single-arg version) ---
        let _fb1: Box<dyn AudioUnit> = Box::new(
            feedback(delay(0.005) >> lowpass_hz(2000.0, 0.5))
        );
    }
}
