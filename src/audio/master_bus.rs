/// Custom master bus for post-processing the voice pool output.
///
/// Processing chain (stereo):
/// ```text
/// L/R input
///   → declick (startup fade-in)
///   → 2-band crossover at 200Hz:
///       bass:   Butterworth LP(200Hz) → peak limiter(10ms, 100ms)
///       treble: Butterworth HP(200Hz) → peak limiter(5ms, 150ms)
///       sum
///   → stereo-linked peak limiter(10ms, 200ms)
///   → DC block(10Hz)
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

/// Standard biquad filter (Direct Form II Transposed).
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl Biquad {
    /// Butterworth lowpass (Q = 1/sqrt(2)).
    fn lowpass(freq: f32, sr: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Butterworth highpass (Q = 1/sqrt(2)).
    fn highpass(freq: f32, sr: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Peak limiter with separate attack/release envelope.
struct PeakLimiter {
    env: f32,
    atk_coeff: f32,
    rel_coeff: f32,
}

impl PeakLimiter {
    fn new(attack_s: f32, release_s: f32, sr: f32) -> Self {
        Self {
            env: 0.0,
            atk_coeff: (-1.0 / (attack_s * sr)).exp(),
            rel_coeff: (-1.0 / (release_s * sr)).exp(),
        }
    }

    /// Process mono sample, returns gain-reduced sample.
    fn tick(&mut self, x: f32) -> f32 {
        let level = x.abs();
        let coeff = if level > self.env {
            self.atk_coeff
        } else {
            self.rel_coeff
        };
        self.env = level + coeff * (self.env - level);

        let gain = if self.env > 1.0 {
            1.0 / self.env
        } else {
            1.0
        };
        x * gain
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

        let gain = if self.env > 1.0 {
            1.0 / self.env
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
pub struct MasterBus {
    declick: Declick,
    bass_lp: [Biquad; 2],
    treble_hp: [Biquad; 2],
    bass_lim: [PeakLimiter; 2],
    treble_lim: [PeakLimiter; 2],
    master_lim: PeakLimiter,
    dc: [DcBlock; 2],
}

impl MasterBus {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            declick: Declick::new(0.01, sample_rate),
            bass_lp: [
                Biquad::lowpass(200.0, sample_rate),
                Biquad::lowpass(200.0, sample_rate),
            ],
            treble_hp: [
                Biquad::highpass(200.0, sample_rate),
                Biquad::highpass(200.0, sample_rate),
            ],
            bass_lim: [
                PeakLimiter::new(0.01, 0.1, sample_rate),
                PeakLimiter::new(0.01, 0.1, sample_rate),
            ],
            treble_lim: [
                PeakLimiter::new(0.005, 0.15, sample_rate),
                PeakLimiter::new(0.005, 0.15, sample_rate),
            ],
            master_lim: PeakLimiter::new(0.01, 0.2, sample_rate),
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

            // 2-band crossover + per-band limiting
            let bass_l = self.bass_lim[0].tick(self.bass_lp[0].tick(l));
            let bass_r = self.bass_lim[1].tick(self.bass_lp[1].tick(r));
            let treble_l = self.treble_lim[0].tick(self.treble_hp[0].tick(l));
            let treble_r = self.treble_lim[1].tick(self.treble_hp[1].tick(r));

            let sum_l = bass_l + treble_l;
            let sum_r = bass_r + treble_r;

            // Stereo-linked master limiter
            let (lim_l, lim_r) = self.master_lim.tick_stereo(sum_l, sum_r);

            // DC block
            let out_l = self.dc[0].tick(lim_l);
            let out_r = self.dc[1].tick(lim_r);

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
        assert!(
            peak < 3.5,
            "Limiter should tame peaks below input of 5.0, got peak={peak}"
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
}
