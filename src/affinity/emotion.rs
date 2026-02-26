const EWMA_ALPHA: f32 = 0.05;

/// Per-module emotional state driven by homeostatic activity tracking.
///
/// Valence reflects happiness (good deliveries, on-target activity).
/// Arousal reflects surprise/boredom (deviation from expected throughput).
/// These drive the affinity graph: valence modulates Hebbian reward,
/// arousal gates exploration of new edges.
#[derive(Debug, Clone)]
pub struct ModuleEmotion {
    /// [-1, 1] happy/unhappy. Driven by homeostatic error + error rate.
    pub valence: f32,
    /// [0, 1] bored/overstimulated. High arousal triggers edge exploration.
    pub arousal: f32,
    /// EWMA of signals received per tick.
    pub activity: f32,
    /// Homeostatic setpoint — the "ideal" throughput for this module.
    pub target_activity: f32,
    /// EWMA of type-error fraction.
    pub error_rate: f32,
}

impl ModuleEmotion {
    pub fn new(target_activity: f32) -> Self {
        Self {
            valence: 0.0,
            arousal: 0.0,
            activity: 0.0,
            target_activity,
            error_rate: 0.0,
        }
    }

    /// Create with DNA-derived base arousal and valence.
    /// Used by organism modules so their initial visual identity
    /// persists until the reactor's Hebbian learning overrides.
    pub fn with_base(target_activity: f32, base_arousal: f32, base_valence: f32) -> Self {
        Self {
            valence: base_valence,
            arousal: base_arousal,
            activity: 0.0,
            target_activity,
            error_rate: 0.0,
        }
    }

    /// Update emotion after a tick. `signals` = total deliveries received,
    /// `errors` = type-mismatched deliveries this tick.
    pub fn update(&mut self, signals: u32, errors: u32) {
        let signals_f = signals as f32;
        let errors_f = errors as f32;
        let error_frac = if signals > 0 {
            errors_f / signals_f
        } else {
            0.0
        };

        // EWMA activity
        self.activity = self.activity * (1.0 - EWMA_ALPHA) + signals_f * EWMA_ALPHA;

        // EWMA error rate
        self.error_rate = self.error_rate * (1.0 - EWMA_ALPHA) + error_frac * EWMA_ALPHA;

        // Homeostatic error: how far from target activity
        let homeostatic_error = (self.activity - self.target_activity)
            / (self.target_activity + 1.0);

        // Valence: positive when on-target with low errors, negative when off-target.
        // Baseline 1.0, penalized by homeostatic deviation and error rate.
        // Perfect (on-target, no errors) → 1.0. Far off-target or high errors → -1.0.
        self.valence = (1.0 - homeostatic_error * homeostatic_error * 4.0 - self.error_rate * 2.0)
            .clamp(-1.0, 1.0);

        // Arousal: surprise = deviation from expected throughput
        let surprise = (signals_f - self.activity).abs();
        let raw_arousal = surprise / (self.activity + 1.0);
        self.arousal = (self.arousal * (1.0 - EWMA_ALPHA) + raw_arousal * EWMA_ALPHA)
            .clamp(0.0, 1.0);
    }

    /// Homeostatic gain: amplify when starved, suppress when overdriven.
    /// Returns a multiplier ~1.0 at target, >1.0 when starved, <1.0 when flooded.
    pub fn homeostatic_gain(&self) -> f32 {
        if self.target_activity < 0.001 {
            return 1.0;
        }
        let ratio = self.activity / self.target_activity;
        // Inverse: starved → high gain, flooded → low gain
        // Clamp to [0.1, 3.0] to avoid runaway
        (1.0 / (ratio + 0.1)).clamp(0.1, 3.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_emotion_starts_neutral() {
        let e = ModuleEmotion::new(5.0);
        assert_eq!(e.valence, 0.0);
        assert_eq!(e.arousal, 0.0);
        assert_eq!(e.activity, 0.0);
        assert_eq!(e.target_activity, 5.0);
    }

    #[test]
    fn steady_activity_at_target_yields_neutral_valence() {
        let mut e = ModuleEmotion::new(5.0);
        // Feed exactly target activity for many ticks
        for _ in 0..200 {
            e.update(5, 0);
        }
        // Valence should be positive (happy, on target, no errors)
        assert!(
            e.valence > 0.5,
            "valence should be positive at target: {}",
            e.valence
        );
    }

    #[test]
    fn starved_module_has_negative_valence() {
        let mut e = ModuleEmotion::new(5.0);
        // Feed zero signals
        for _ in 0..100 {
            e.update(0, 0);
        }
        // Valence should be negative (unhappy, starved)
        assert!(e.valence <= 0.0, "starved valence should be <= 0: {}", e.valence);
    }

    #[test]
    fn high_errors_tank_valence() {
        let mut e = ModuleEmotion::new(5.0);
        for _ in 0..100 {
            e.update(5, 5); // 100% error rate
        }
        assert!(e.valence < -0.5, "valence should be very negative: {}", e.valence);
    }

    #[test]
    fn homeostatic_gain_amplifies_starved() {
        let mut e = ModuleEmotion::new(5.0);
        e.activity = 0.5; // well below target
        let gain = e.homeostatic_gain();
        assert!(gain > 1.0, "starved gain should be > 1.0: {}", gain);
    }

    #[test]
    fn homeostatic_gain_suppresses_flooded() {
        let mut e = ModuleEmotion::new(5.0);
        e.activity = 20.0; // well above target
        let gain = e.homeostatic_gain();
        assert!(gain < 1.0, "flooded gain should be < 1.0: {}", gain);
    }

    #[test]
    fn arousal_spikes_on_surprise() {
        let mut e = ModuleEmotion::new(5.0);
        // Establish baseline
        for _ in 0..50 {
            e.update(5, 0);
        }
        let baseline_arousal = e.arousal;
        // Sudden spike
        e.update(50, 0);
        assert!(
            e.arousal > baseline_arousal,
            "arousal should spike: {} > {}",
            e.arousal,
            baseline_arousal
        );
    }
}
