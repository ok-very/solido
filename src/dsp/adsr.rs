/// ADSR envelope stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR envelope generator with precomputed per-sample rates.
///
/// All times in milliseconds, all levels in 0.0..1.0.
/// Rates are precomputed when `note_on()`/`note_off()` is called to avoid
/// division in the audio callback hot path.
#[derive(Debug, Clone)]
pub struct AdsrState {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    stage: AdsrStage,
    level: f32,
    sample_rate: f32,
    attack_rate: f32,
    decay_rate: f32,
    release_rate: f32,
}

impl AdsrState {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            attack_ms: 10.0,
            decay_ms: 100.0,
            sustain: 0.7,
            release_ms: 200.0,
            stage: AdsrStage::Idle,
            level: 0.0,
            sample_rate,
            attack_rate: 0.0,
            decay_rate: 0.0,
            release_rate: 0.0,
        };
        s.recalc_rates();
        s
    }

    fn recalc_rates(&mut self) {
        let sr = self.sample_rate;
        self.attack_rate = if self.attack_ms > 0.0 {
            1.0 / (self.attack_ms * 0.001 * sr)
        } else {
            1.0 // instant attack
        };
        self.decay_rate = if self.decay_ms > 0.0 {
            (1.0 - self.sustain) / (self.decay_ms * 0.001 * sr)
        } else {
            1.0
        };
        self.release_rate = if self.release_ms > 0.0 {
            self.sustain / (self.release_ms * 0.001 * sr)
        } else {
            1.0
        };
    }

    /// Trigger the envelope. Transitions to Attack from any stage.
    pub fn note_on(&mut self) {
        self.recalc_rates();
        self.stage = AdsrStage::Attack;
        // Don't reset level — allows retriggering from current position
    }

    /// Release the envelope. Transitions to Release from any non-Idle stage.
    pub fn note_off(&mut self) {
        if self.stage != AdsrStage::Idle {
            // Recompute release rate from current level (not sustain level)
            self.release_rate = if self.release_ms > 0.0 {
                self.level / (self.release_ms * 0.001 * self.sample_rate)
            } else {
                1.0
            };
            self.stage = AdsrStage::Release;
        }
    }

    /// Process one sample. Returns the current envelope level (0.0..1.0).
    pub fn process(&mut self) -> f32 {
        match self.stage {
            AdsrStage::Idle => {}
            AdsrStage::Attack => {
                self.level += self.attack_rate;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                self.level -= self.decay_rate;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                self.level = self.sustain;
            }
            AdsrStage::Release => {
                self.level -= self.release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                }
            }
        }
        self.level
    }

    pub fn is_idle(&self) -> bool {
        self.stage == AdsrStage::Idle
    }

    pub fn stage(&self) -> AdsrStage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// Reset to idle with zero level.
    pub fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
    }
}
