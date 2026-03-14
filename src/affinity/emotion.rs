const EWMA_ALPHA: f32 = 0.05;
/// Minimum arousal — prevents heat death where wander thrust collapses to zero.
/// Floor of 0.15 ensures wander thrust stays at 40*(0.3+0.15*1.5) = 21 px/s.
const AROUSAL_FLOOR: f32 = 0.15;
/// Maximum arousal increase from all modulation sources (apply_*) per frame.
/// Prevents 5+ independent arousal sources from latching arousal at 1.0.
const MAX_AROUSAL_MODULATION: f32 = 0.15;

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
    /// Arousal value after homeostatic update, before modulation sources.
    /// Caps total modulation delta to MAX_AROUSAL_MODULATION per frame.
    arousal_post_update: f32,
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
            arousal_post_update: 0.0,
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
            arousal_post_update: base_arousal,
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
            .clamp(0.0, 1.0)
            .max(AROUSAL_FLOOR);

        // Snapshot arousal after homeostatic update. Modulation sources (apply_*)
        // are capped relative to this baseline to prevent accumulation latch.
        self.arousal_post_update = self.arousal;
    }

    /// Cap a boosted arousal value to at most `arousal_post_update + MAX_AROUSAL_MODULATION`.
    /// Prevents multiple modulation sources from latching arousal at 1.0.
    fn capped_arousal(&self, boosted: f32) -> f32 {
        let ceiling = (self.arousal_post_update + MAX_AROUSAL_MODULATION).min(1.0);
        boosted.min(ceiling).max(AROUSAL_FLOOR)
    }

    /// Apply navigation valence after the normal homeostatic update.
    pub fn apply_navigation_reward(&mut self, nav_delta: f32, nav_weight: f32) {
        self.valence = (self.valence + nav_delta * nav_weight).clamp(-1.0, 1.0);
    }

    /// Apply trapping stress to arousal (drives exploration behavior).
    pub fn apply_trap_arousal(&mut self, trap_stress: f32) {
        let arousal_boost = trap_stress * 0.3;
        self.arousal = self.capped_arousal(self.arousal + arousal_boost);
    }

    /// S40: Apply harmonic consonance reward to valence.
    pub fn apply_harmonic_reward(&mut self, delta: f32) {
        self.valence = (self.valence + delta).clamp(-1.0, 1.0);
    }

    /// S40: Apply harmonic dissonance tension to arousal.
    pub fn apply_harmonic_tension(&mut self, tension: f32) {
        self.arousal = self.capped_arousal(self.arousal + tension);
    }

    /// Apply node energy exchange reward/penalty to valence.
    /// Gain → +valence (×0.3), loss → -valence (×0.5, loss aversion).
    pub fn apply_energy_exchange(&mut self, net_delta: f32) {
        let valence_delta = if net_delta >= 0.0 {
            net_delta * 0.3
        } else {
            net_delta * 0.5 // loss aversion: losses hurt more than gains
        };
        self.valence = (self.valence + valence_delta).clamp(-1.0, 1.0);
    }

    /// Apply depletion arousal: dormant node fraction → arousal spike (escape drive).
    pub fn apply_depletion_arousal(&mut self, dormant_fraction: f32) {
        let arousal_boost = dormant_fraction * 0.4;
        self.arousal = self.capped_arousal(self.arousal + arousal_boost);
    }

    /// Nutrient hunger drives exploration arousal.
    /// max_deficit = largest single-channel nutrient shortfall.
    pub fn apply_nutrient_hunger(&mut self, max_deficit: f32) {
        let arousal_boost = max_deficit * 0.3;
        self.arousal = self.capped_arousal(self.arousal + arousal_boost);
    }

    #[allow(dead_code)]
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
    fn nav_reward_valence_clamping() {
        let mut e = ModuleEmotion::new(5.0);
        e.valence = 0.9;
        e.apply_navigation_reward(1.0, 0.5); // +0.5 would push to 1.4
        assert!(e.valence <= 1.0, "valence should be clamped to 1.0: {}", e.valence);
        assert!((e.valence - 1.0).abs() < 0.001);

        e.valence = -0.9;
        e.apply_navigation_reward(-1.0, 0.5); // -0.5 would push to -1.4
        assert!(e.valence >= -1.0, "valence should be clamped to -1.0: {}", e.valence);
    }

    #[test]
    fn trap_arousal_clamping() {
        let mut e = ModuleEmotion::new(5.0);
        e.arousal = 0.95;
        e.apply_trap_arousal(1.0); // +0.3 would push to 1.25
        assert!(e.arousal <= 1.0, "arousal should be clamped: {}", e.arousal);
    }

    #[test]
    fn arousal_has_floor() {
        let mut e = ModuleEmotion::new(5.0);
        // Feed steady-state for many ticks — arousal should decay but never below floor
        for _ in 0..500 {
            e.update(5, 0);
        }
        assert!(
            e.arousal >= super::AROUSAL_FLOOR,
            "arousal should not drop below floor: {} < {}",
            e.arousal,
            super::AROUSAL_FLOOR
        );
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

    #[test]
    fn energy_exchange_gain_boosts_valence() {
        let mut e = ModuleEmotion::new(5.0);
        e.valence = 0.0;
        e.apply_energy_exchange(0.5); // net gain
        assert!(e.valence > 0.0, "gain should boost valence: {}", e.valence);
    }

    #[test]
    fn energy_exchange_loss_aversion() {
        let mut e = ModuleEmotion::new(5.0);
        e.valence = 0.0;
        e.apply_energy_exchange(-0.5); // net loss
        let loss_valence = e.valence;

        let mut e2 = ModuleEmotion::new(5.0);
        e2.valence = 0.0;
        e2.apply_energy_exchange(0.5); // equal magnitude gain
        let gain_valence = e2.valence;

        assert!(
            loss_valence.abs() > gain_valence.abs(),
            "loss should hurt more than gain helps: loss={}, gain={}",
            loss_valence, gain_valence
        );
    }

    #[test]
    fn depletion_arousal_spike() {
        let mut e = ModuleEmotion::new(5.0);
        // Establish baseline via update so arousal_post_update is set
        for _ in 0..50 {
            e.update(5, 0);
        }
        let baseline = e.arousal;
        e.apply_depletion_arousal(0.5); // 50% dormant
        assert!(e.arousal > baseline, "dormant nodes should spike arousal: {}", e.arousal);
    }

    #[test]
    fn nutrient_hunger_boosts_arousal() {
        let mut e = ModuleEmotion::new(5.0);
        // Establish baseline via update so arousal_post_update is set
        for _ in 0..50 {
            e.update(5, 0);
        }
        let baseline = e.arousal;
        e.apply_nutrient_hunger(0.3); // max deficit = 0.3
        assert!(e.arousal > baseline, "hunger should boost arousal: {}", e.arousal);
    }

    #[test]
    fn nutrient_hunger_clamping() {
        let mut e = ModuleEmotion::new(5.0);
        e.arousal = 0.95;
        e.apply_nutrient_hunger(1.0); // max deficit
        assert!(e.arousal <= 1.0, "arousal should be clamped: {}", e.arousal);
    }

    #[test]
    fn arousal_modulation_capped() {
        let mut e = ModuleEmotion::new(5.0);
        // Run update to establish arousal_post_update baseline
        for _ in 0..50 {
            e.update(5, 0);
        }
        let baseline = e.arousal;

        // Apply all 4 arousal-modifying sources with large inputs
        e.apply_trap_arousal(1.0);       // would add 0.3
        e.apply_harmonic_tension(0.5);   // would add 0.5
        e.apply_depletion_arousal(1.0);  // would add 0.4
        e.apply_nutrient_hunger(1.0);    // would add 0.3

        // Total uncapped: +1.5. Should be limited to baseline + MAX_AROUSAL_MODULATION
        let max_allowed = (baseline + MAX_AROUSAL_MODULATION).min(1.0);
        assert!(
            e.arousal <= max_allowed + 1e-6,
            "arousal {} should not exceed post-update {} + cap {}: max_allowed={}",
            e.arousal, baseline, MAX_AROUSAL_MODULATION, max_allowed
        );
        assert!(
            e.arousal > baseline,
            "arousal should still increase from baseline: {} > {}",
            e.arousal, baseline
        );
    }
}
