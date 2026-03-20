//! Per-organism chaos noise generator.
//!
//! Runs on the control thread at ~60Hz. Outputs a noise value [0,1]
//! that modulates chaos intensity:
//!
//!   chaos = base_chaos + max_chaos × clamp(arousal × noise, 0, 1)
//!
//! Three noise characters produce different generative textures:
//! - Brownian: slow correlated drift (drones, pads)
//! - Pink: 1/f self-similar variation (melodic, balanced)
//! - Spectral: lowpass-filtered white noise (rhythmic, beat-synced feel)

/// Noise character — determines how chaos varies over time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoiseType {
    /// Integrated white noise. Slow correlated drift.
    Brownian,
    /// 1/f noise (Voss-McCartney). Natural across timescales.
    Pink,
    /// White noise through one-pole lowpass. Smooth, rhythm-friendly.
    Spectral,
}

/// Select noise type by species personality.
pub fn noise_type_for_species(species: &str) -> NoiseType {
    match species {
        "dron" | "hoso" => NoiseType::Brownian,
        "acid" | "tblk" | "kkit" => NoiseType::Spectral,
        _ => NoiseType::Pink, // ISAO, SPGL, RECH, etc.
    }
}

/// Per-organism chaos noise generator with history ring buffer.
pub struct ChaosNoiseGen {
    noise_type: NoiseType,
    value: f32,
    velocity: f32,          // Brownian integration
    rng: u32,               // xorshift PRNG
    pink_oct: [f32; 6],     // Voss-McCartney octaves
    pink_ctr: u32,          // frame counter for octave updates
    lp_state: f32,          // Spectral lowpass state
    /// Ring buffer of recent noise values for UI sparkline.
    pub history: [f32; 64],
    pub history_pos: usize,
}

impl ChaosNoiseGen {
    pub fn new(noise_type: NoiseType, seed: u32) -> Self {
        let mut gen = Self {
            noise_type,
            value: 0.5,
            velocity: 0.0,
            rng: seed | 1,
            pink_oct: [0.5; 6],
            pink_ctr: 0,
            lp_state: 0.5,
            history: [0.5; 64],
            history_pos: 0,
        };
        // Warm up: run 32 steps so initial state isn't all 0.5
        for _ in 0..32 {
            gen.step();
        }
        gen
    }

    /// Current noise output [0, 1].
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Advance one step. Call once per frame (~60Hz).
    pub fn update(&mut self) {
        self.step();
        self.history[self.history_pos] = self.value;
        self.history_pos = (self.history_pos + 1) % self.history.len();
    }

    /// Read history as a chronological slice (oldest first).
    pub fn history_ordered(&self) -> Vec<f32> {
        let len = self.history.len();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(self.history[(self.history_pos + i) % len]);
        }
        out
    }

    fn step(&mut self) {
        match self.noise_type {
            NoiseType::Brownian => self.step_brownian(),
            NoiseType::Pink => self.step_pink(),
            NoiseType::Spectral => self.step_spectral(),
        }
    }

    fn step_brownian(&mut self) {
        let white = rand_bipolar(&mut self.rng);
        self.velocity += white * 0.015;
        self.velocity *= 0.93;
        self.value += self.velocity;
        if self.value < 0.0 {
            self.value = 0.0;
            self.velocity = self.velocity.abs() * 0.5;
        }
        if self.value > 1.0 {
            self.value = 1.0;
            self.velocity = -self.velocity.abs() * 0.5;
        }
    }

    fn step_pink(&mut self) {
        self.pink_ctr = self.pink_ctr.wrapping_add(1);
        for i in 0..6 {
            if self.pink_ctr % (1 << i) == 0 {
                self.pink_oct[i] = rand_f32(&mut self.rng);
            }
        }
        let sum: f32 = self.pink_oct.iter().sum();
        self.value = (sum / 6.0).clamp(0.0, 1.0);
    }

    fn step_spectral(&mut self) {
        let white = rand_f32(&mut self.rng);
        let alpha = 0.08; // ~2Hz cutoff at 60fps
        self.lp_state += alpha * (white - self.lp_state);
        self.value = self.lp_state.clamp(0.0, 1.0);
    }
}

// ─── RT-safe PRNG ────────────────────────────────────────────────────

fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn rand_f32(state: &mut u32) -> f32 {
    (xorshift32(state) & 0xFFFF) as f32 / 65535.0
}

fn rand_bipolar(state: &mut u32) -> f32 {
    rand_f32(state) * 2.0 - 1.0
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brownian_stays_bounded() {
        let mut gen = ChaosNoiseGen::new(NoiseType::Brownian, 42);
        for _ in 0..10000 {
            gen.update();
            assert!(gen.value() >= 0.0 && gen.value() <= 1.0);
        }
    }

    #[test]
    fn pink_stays_bounded() {
        let mut gen = ChaosNoiseGen::new(NoiseType::Pink, 42);
        for _ in 0..10000 {
            gen.update();
            assert!(gen.value() >= 0.0 && gen.value() <= 1.0);
        }
    }

    #[test]
    fn spectral_stays_bounded() {
        let mut gen = ChaosNoiseGen::new(NoiseType::Spectral, 42);
        for _ in 0..10000 {
            gen.update();
            assert!(gen.value() >= 0.0 && gen.value() <= 1.0);
        }
    }

    #[test]
    fn brownian_has_variance() {
        let mut gen = ChaosNoiseGen::new(NoiseType::Brownian, 123);
        let mut min = 1.0f32;
        let mut max = 0.0f32;
        for _ in 0..1000 {
            gen.update();
            min = min.min(gen.value());
            max = max.max(gen.value());
        }
        assert!(max - min > 0.1, "Brownian should vary: min={min}, max={max}");
    }

    #[test]
    fn history_wraps_correctly() {
        let mut gen = ChaosNoiseGen::new(NoiseType::Pink, 99);
        for _ in 0..100 {
            gen.update();
        }
        let hist = gen.history_ordered();
        assert_eq!(hist.len(), 64);
        // Last element should be the most recent value
        assert!((hist[63] - gen.value()).abs() < 0.01);
    }

    #[test]
    fn species_mapping() {
        assert_eq!(noise_type_for_species("dron"), NoiseType::Brownian);
        assert_eq!(noise_type_for_species("acid"), NoiseType::Spectral);
        assert_eq!(noise_type_for_species("isao"), NoiseType::Pink);
    }
}
