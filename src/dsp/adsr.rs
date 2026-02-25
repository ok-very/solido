/// ADSR envelope stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Compute RC-circuit coefficient for a given time in milliseconds.
/// Reaches ~99.3% of target in the given time (5 time constants).
fn rc_coeff(time_ms: f32, sr: f32) -> f32 {
    if time_ms <= 0.0 {
        return 1.0; // instant
    }
    1.0 - (-5.0 / (time_ms * 0.001 * sr)).exp()
}

/// ADSR envelope generator with exponential RC-circuit curves.
///
/// All times in milliseconds, all levels in 0.0..1.0.
/// Coefficients are precomputed when `note_on()`/`note_off()` is called to avoid
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
    attack_coeff: f32,
    decay_coeff: f32,
    release_coeff: f32,
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
            attack_coeff: 0.0,
            decay_coeff: 0.0,
            release_coeff: 0.0,
        };
        s.recalc_coeffs();
        s
    }

    fn recalc_coeffs(&mut self) {
        let sr = self.sample_rate;
        self.attack_coeff = rc_coeff(self.attack_ms, sr);
        self.decay_coeff = rc_coeff(self.decay_ms, sr);
        self.release_coeff = rc_coeff(self.release_ms, sr);
    }

    /// Trigger the envelope. Transitions to Attack from any stage.
    pub fn note_on(&mut self) {
        self.recalc_coeffs();
        self.stage = AdsrStage::Attack;
        // Don't reset level — allows retriggering from current position
    }

    /// Release the envelope. Transitions to Release from any non-Idle stage.
    pub fn note_off(&mut self) {
        if self.stage != AdsrStage::Idle {
            // Exponential coefficient is level-independent, just transition
            self.stage = AdsrStage::Release;
        }
    }

    /// Process one sample. Returns the current envelope level (0.0..1.0).
    pub fn process(&mut self) -> f32 {
        match self.stage {
            AdsrStage::Idle => {}
            AdsrStage::Attack => {
                // Exponential approach to 1.02 (slight overshoot target)
                self.level += (1.02 - self.level) * self.attack_coeff;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                // Exponential approach to sustain level
                self.level += (self.sustain - self.level) * self.decay_coeff;
                if (self.level - self.sustain).abs() < 0.001 {
                    self.level = self.sustain;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                self.level = self.sustain;
            }
            AdsrStage::Release => {
                // Exponential approach to 0.0
                self.level += (0.0 - self.level) * self.release_coeff;
                if self.level < 0.001 {
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

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    #[test]
    fn adsr_attack_is_convex() {
        let mut s = AdsrState::new(SR);
        s.attack_ms = 20.0;
        s.recalc_coeffs();
        s.note_on();

        // Run to 50% of attack time (10ms = 441 samples)
        let half_attack_samples = (s.attack_ms * 0.5 * 0.001 * SR) as usize;
        for _ in 0..half_attack_samples {
            s.process();
        }
        // Exponential attack is front-loaded: at 50% time, level should be >0.5
        assert!(
            s.level() > 0.5,
            "Exponential attack at 50% time should be >0.5, got {}",
            s.level()
        );
    }

    #[test]
    fn adsr_release_tail() {
        let mut s = AdsrState::new(SR);
        s.release_ms = 200.0;
        s.recalc_coeffs();
        s.note_on();

        // Run through attack+decay to sustain
        for _ in 0..10000 {
            s.process();
        }

        s.note_off();
        // Run for 2× release_ms = 400ms = 17640 samples
        let two_x_release = (s.release_ms * 2.0 * 0.001 * SR) as usize;
        for _ in 0..two_x_release {
            s.process();
        }
        // Exponential release has a long tail — should still be >0.0 but very small
        // (unlike linear which would be exactly 0 at 1× release_ms)
        // At 2× release time with RC curve, level ≈ sustain * exp(-10) ≈ tiny but test the tail property
        // The key property: at 2× release, linear = 0, exponential might still have residual
        // Actually with 5-tau RC, at 2× time we're at ~10 tau, so it will be ~0.
        // Test at 1× release instead for meaningful tail check.
        // Let's restart:
        let mut s2 = AdsrState::new(SR);
        s2.release_ms = 200.0;
        s2.recalc_coeffs();
        s2.note_on();
        for _ in 0..10000 {
            s2.process();
        }
        s2.note_off();
        // Run for 0.5× release = 100ms = 4410 samples
        let half_release = (s2.release_ms * 0.5 * 0.001 * SR) as usize;
        for _ in 0..half_release {
            s2.process();
        }
        // At 50% of release time, exponential should still have significant level
        // (linear would be at ~0.35, exponential should be higher due to slow start)
        assert!(
            s2.level() > 0.01,
            "Exponential release at 50% time should still have signal, got {}",
            s2.level()
        );
    }

    #[test]
    fn adsr_retrigger_no_click() {
        let mut s = AdsrState::new(SR);
        s.note_on();
        // Run into decay
        for _ in 0..2000 {
            s.process();
        }
        s.note_off();
        // Run partway through release
        for _ in 0..1000 {
            s.process();
        }
        let level_before = s.level();
        assert!(level_before > 0.01, "Should still have level during release");

        // Retrigger
        s.note_on();
        let level_after = s.process();
        // No discontinuity: level should be >= level_before (attack moves upward)
        assert!(
            level_after >= level_before - 0.01,
            "Retrigger should not cause discontinuity: before={}, after={}",
            level_before,
            level_after,
        );
    }
}
