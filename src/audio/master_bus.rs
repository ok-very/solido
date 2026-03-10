/// Custom master bus for post-processing the voice pool output.
///
/// Processing chain (stereo):
/// ```text
/// L/R input
///   → declick (startup fade-in)
///   → stereo-linked peak limiter(10ms atk, 200ms rel, threshold 1.0)
///   → DC block(10Hz)
///   → hard clamp ±1.0 (DAC safety)
///   → output L/R
/// ```

/// Linear fade-in ramp from 0→1 over `len` samples.
struct Declick {
    pos: u32,
    len: u32,
}

impl Declick {
    fn new(fade_secs: f32, sr: f32) -> Self {
        Self {
            pos: 0,
            len: (fade_secs * sr).max(1.0) as u32,
        }
    }

    fn tick(&mut self) -> f32 {
        if self.pos >= self.len {
            return 1.0;
        }
        let gain = self.pos as f32 / self.len as f32;
        self.pos += 1;
        gain
    }
}

/// Peak limiter with separate attack/release envelope and configurable threshold.
struct PeakLimiter {
    env: f32,
    atk_coeff: f32,
    rel_coeff: f32,
    threshold: f32,
}

impl PeakLimiter {
    fn new(attack_s: f32, release_s: f32, sr: f32, threshold: f32) -> Self {
        Self {
            env: 0.0,
            atk_coeff: (-1.0 / (attack_s * sr)).exp(),
            rel_coeff: (-1.0 / (release_s * sr)).exp(),
            threshold,
        }
    }

    /// Process stereo pair with linked envelope (max of both channels).
    fn tick_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level = l.abs().max(r.abs());
        let coeff = if level > self.env {
            self.atk_coeff
        } else {
            self.rel_coeff
        };
        self.env = level + coeff * (self.env - level);

        let gain = if self.env > self.threshold {
            self.threshold / self.env
        } else {
            1.0
        };
        (l * gain, r * gain)
    }
}

/// First-order DC blocking filter.
/// y[n] = x[n] - x[n-1] + coeff * y[n-1]
struct DcBlock {
    x_prev: f32,
    y_prev: f32,
    coeff: f32,
}

impl DcBlock {
    fn new(freq: f32, sr: f32) -> Self {
        Self {
            x_prev: 0.0,
            y_prev: 0.0,
            coeff: 1.0 - 2.0 * std::f32::consts::PI * freq / sr,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = x - self.x_prev + self.coeff * self.y_prev;
        self.x_prev = x;
        self.y_prev = y;
        y
    }
}

/// Custom master bus for post-processing the voice pool output.
///
/// Simplified chain: declick → limiter → DC block → hard clamp.
/// No crossover — a 2nd-order Butterworth LP+HP sum has a complete null at the
/// crossover frequency (H_LP + H_HP = 0 at ω₀), which was cutting all energy
/// around 200Hz. Since per-band limiters were removed, the crossover served
/// no purpose.
pub struct MasterBus {
    declick: Declick,
    master_lim: PeakLimiter,
    dc: [DcBlock; 2],
}

impl MasterBus {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            declick: Declick::new(0.01, sample_rate),
            // Single stereo limiter: 10ms attack, 200ms release, threshold 1.0
            // Catches peaks at 0 dBFS — the DAC hard-clips above 1.0.
            master_lim: PeakLimiter::new(0.01, 0.2, sample_rate, 1.0),
            dc: [
                DcBlock::new(10.0, sample_rate),
                DcBlock::new(10.0, sample_rate),
            ],
        }
    }

    /// Process interleaved stereo audio in-place.
    pub fn process(&mut self, data: &mut [f32], channels: u16) {
        let ch = channels as usize;
        if ch == 0 {
            return;
        }
        let num_frames = data.len() / ch;

        for frame in 0..num_frames {
            let base = frame * ch;
            let l_in = data[base];
            let r_in = if ch > 1 { data[base + 1] } else { l_in };

            // Declick fade-in
            let fade = self.declick.tick();
            let l = l_in * fade;
            let r = r_in * fade;

            // Stereo-linked peak limiter (threshold 1.0)
            let (lim_l, lim_r) = self.master_lim.tick_stereo(l, r);

            // DC block + hard safety clamp at ±1.0
            let out_l = self.dc[0].tick(lim_l).clamp(-1.0, 1.0);
            let out_r = self.dc[1].tick(lim_r).clamp(-1.0, 1.0);

            data[base] = out_l;
            if ch > 1 {
                data[base + 1] = out_r;
            }
            // Fill remaining channels with left output
            for c in 2..ch {
                data[base + c] = out_l;
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
        assert!(
            buf.iter().all(|&s| s.abs() < 1e-6),
            "Silence in should produce silence out"
        );
    }

    #[test]
    fn master_bus_limits_loud_signal() {
        let mut bus = MasterBus::new(SR);
        let frames = 4096;
        let mut buf = vec![0.0f32; frames * 2];
        for frame in 0..frames {
            let t = frame as f32 / SR;
            let sample = 5.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            buf[frame * 2] = sample;
            buf[frame * 2 + 1] = sample;
        }

        bus.process(&mut buf, 2);

        let peak: f32 = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        // Hard clamp at ±1.0 — no sample should exceed DAC range.
        assert!(
            peak <= 1.0,
            "Output should never exceed ±1.0 (DAC range), got peak={peak}"
        );
    }

    #[test]
    fn master_bus_removes_dc() {
        let mut bus = MasterBus::new(SR);
        let frames = 8192;
        let mut buf = vec![0.0f32; frames * 2];
        for frame in 0..frames {
            buf[frame * 2] = 0.5;
            buf[frame * 2 + 1] = 0.5;
        }

        bus.process(&mut buf, 2);

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
        bus.process(&mut buf, 1);
    }

    #[test]
    fn no_notch_at_200hz() {
        // Regression test: the old 2nd-order Butterworth crossover at 200Hz
        // had LP+HP sum = 0 at the crossover frequency (complete cancellation).
        // This test verifies that a 200Hz sine passes through at near-unity.
        let mut bus = MasterBus::new(SR);
        let frames = 4096;
        let amplitude = 0.5;
        let mut buf = vec![0.0f32; frames * 2];
        for frame in 0..frames {
            let t = frame as f32 / SR;
            let sample = amplitude * (2.0 * std::f32::consts::PI * 200.0 * t).sin();
            buf[frame * 2] = sample;
            buf[frame * 2 + 1] = sample;
        }

        bus.process(&mut buf, 2);

        // Measure peak in the second half (after declick and DC block settle)
        let peak: f32 = buf[frames..].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            peak > amplitude * 0.8,
            "200Hz sine should pass through at near-unity, got peak={peak} (expected ~{amplitude})"
        );
    }

    #[test]
    fn limiter_allows_signal_at_threshold() {
        let mut lim = PeakLimiter::new(0.01, 0.2, SR, 1.0);
        // Signal at 0.5 — below threshold, should pass through unchanged
        let (l, r) = lim.tick_stereo(0.5, 0.5);
        assert!((l - 0.5).abs() < 0.01, "Below-threshold signal should pass: got {l}");
        assert!((r - 0.5).abs() < 0.01, "Below-threshold signal should pass: got {r}");
    }
}
