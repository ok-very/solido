use crate::module::{ModuleId, PortId};

/// Identifies a directed edge: (source_module, output_port, target_module, input_port).
pub type EdgeId = (ModuleId, PortId, ModuleId, PortId);

/// Hebbian learning constants.
const DECAY: f32 = 0.001;
const LR: f32 = 0.02;
const ELIG_DECAY: f32 = 0.85;
const EWMA_ALPHA: f32 = 0.05;

/// A single affinity edge between two module ports.
///
/// Weight evolves through Hebbian learning: edges that carry satisfying
/// signals to happy modules get stronger. Edges that carry unsatisfying
/// or type-mismatched signals decay.
#[derive(Debug, Clone)]
pub struct EdgeAffinity {
    /// Routing strength [0, 1]. Higher = more signal gets through.
    pub weight: f32,
    /// Eligibility trace: did this edge fire recently? Decays each tick.
    pub eligibility: f32,
    /// EWMA of receiver-reported satisfaction [0, 1].
    /// Replaces the old binary type_valid goodput. Receivers report
    /// quality via `port_satisfaction()` after each delivery.
    pub satisfaction: f32,
    /// EWMA of signal change magnitude (delta from previous delivery).
    /// Tracks information novelty, not raw amplitude.
    pub impact: f32,
    /// Previous delivery magnitude, for computing delta.
    prev_magnitude: f32,
    /// How many ticks this edge has existed.
    pub age_blocks: u64,
}

impl EdgeAffinity {
    pub fn new() -> Self {
        Self {
            weight: 0.5,
            eligibility: 0.0,
            satisfaction: 1.0,
            impact: 0.0,
            prev_magnitude: 0.0,
            age_blocks: 0,
        }
    }

    /// Decay weight toward 0.5, decay eligibility trace. Called every tick.
    pub fn tick_decay(&mut self) {
        // Weight drifts toward 0.5 (neutral)
        self.weight += (0.5 - self.weight) * DECAY;
        // Eligibility trace decays
        self.eligibility *= ELIG_DECAY;
        self.age_blocks += 1;
    }

    /// Record a delivery attempt. Updates satisfaction, impact, and eligibility.
    ///
    /// `receiver_satisfaction` is [0,1] quality score from the receiving module's
    /// `port_satisfaction()`. Default 1.0 for modules that don't override.
    pub fn on_delivery(&mut self, type_valid: bool, magnitude: f32, receiver_satisfaction: f32) {
        // Satisfaction: blend type validity with receiver quality.
        // Type mismatch → 0 regardless of receiver opinion.
        let effective = if type_valid { receiver_satisfaction } else { 0.0 };
        self.satisfaction = self.satisfaction * (1.0 - EWMA_ALPHA) + effective * EWMA_ALPHA;

        // Impact as change-magnitude (delta from previous delivery).
        // A constant signal carries no new information → impact converges to 0.
        let delta = (magnitude - self.prev_magnitude).abs();
        self.prev_magnitude = magnitude;
        self.impact = self.impact * (1.0 - EWMA_ALPHA) + delta * EWMA_ALPHA;

        // Eligibility spikes on any delivery — marks the edge as "active".
        let spike = if type_valid { 1.0 } else { 0.5 };
        self.eligibility = (self.eligibility + spike).min(1.0);
    }

    /// Hebbian weight update: reward-modulated by satisfaction and valence.
    /// `dw = LR * eligibility * valence * satisfaction`
    pub fn apply_reward(&mut self, valence: f32) {
        let dw = LR * self.eligibility * valence * self.satisfaction;
        self.weight = (self.weight + dw).clamp(0.0, 1.0);
    }

    /// True if this edge is old and weak enough to prune.
    pub fn should_prune(&self, min_age: u64, threshold: f32) -> bool {
        self.age_blocks > min_age && self.weight < threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_edge_starts_neutral() {
        let e = EdgeAffinity::new();
        assert_eq!(e.weight, 0.5);
        assert_eq!(e.eligibility, 0.0);
        assert_eq!(e.satisfaction, 1.0);
        assert_eq!(e.age_blocks, 0);
    }

    #[test]
    fn tick_decay_ages_and_decays_eligibility() {
        let mut e = EdgeAffinity::new();
        e.eligibility = 1.0;
        for _ in 0..10 {
            e.tick_decay();
        }
        assert_eq!(e.age_blocks, 10);
        assert!(e.eligibility < 0.3, "eligibility should decay: {}", e.eligibility);
        // Weight should stay near 0.5 (already at neutral)
        assert!((e.weight - 0.5).abs() < 0.01);
    }

    #[test]
    fn good_deliveries_maintain_satisfaction() {
        let mut e = EdgeAffinity::new();
        for _ in 0..20 {
            e.on_delivery(true, 1.0, 1.0);
        }
        assert!(e.satisfaction > 0.9, "satisfaction should stay high: {}", e.satisfaction);
    }

    #[test]
    fn bad_deliveries_tank_satisfaction() {
        let mut e = EdgeAffinity::new();
        for _ in 0..100 {
            e.on_delivery(false, 0.0, 1.0);
        }
        assert!(e.satisfaction < 0.1, "satisfaction should drop: {}", e.satisfaction);
    }

    #[test]
    fn low_receiver_satisfaction_weakens_satisfaction() {
        let mut e = EdgeAffinity::new();
        for _ in 0..100 {
            e.on_delivery(true, 1.0, 0.1);
        }
        assert!(e.satisfaction < 0.2, "low receiver quality should lower satisfaction: {}", e.satisfaction);
    }

    #[test]
    fn impact_tracks_delta_not_absolute() {
        let mut e = EdgeAffinity::new();
        // First delivery has large delta from 0→308, but subsequent are 0.
        // After enough deliveries, the initial spike should decay away.
        for _ in 0..200 {
            e.on_delivery(true, 308.0, 1.0);
        }
        assert!(e.impact < 1.0, "constant signal should have near-zero impact: {}", e.impact);

        // Changing signal → impact stays elevated
        let mut e2 = EdgeAffinity::new();
        for i in 0..200 {
            let mag = if i % 2 == 0 { 308.0 } else { 440.0 };
            e2.on_delivery(true, mag, 1.0);
        }
        assert!(e2.impact > 5.0, "changing signal should have positive impact: {}", e2.impact);
        assert!(e2.impact > e.impact * 10.0, "changing > constant");
    }

    #[test]
    fn positive_reward_strengthens() {
        let mut e = EdgeAffinity::new();
        e.on_delivery(true, 1.0, 1.0);
        let before = e.weight;
        e.apply_reward(1.0);
        assert!(e.weight > before, "weight should increase");
    }

    #[test]
    fn negative_reward_weakens() {
        let mut e = EdgeAffinity::new();
        e.on_delivery(true, 1.0, 1.0);
        let before = e.weight;
        e.apply_reward(-1.0);
        assert!(e.weight < before, "weight should decrease");
    }

    #[test]
    fn weight_clamped_to_unit() {
        let mut e = EdgeAffinity::new();
        e.weight = 0.99;
        e.eligibility = 1.0;
        e.satisfaction = 1.0;
        e.apply_reward(100.0);
        assert!(e.weight <= 1.0);

        e.weight = 0.01;
        e.apply_reward(-100.0);
        assert!(e.weight >= 0.0);
    }

    #[test]
    fn satisfaction_modulates_learning_rate() {
        // Run many deliveries so satisfaction EWMA has converged
        let mut high = EdgeAffinity::new();
        for _ in 0..100 {
            high.on_delivery(true, 1.0, 0.9);
        }
        let h_before = high.weight;
        high.apply_reward(1.0);
        let h_delta = high.weight - h_before;

        let mut low = EdgeAffinity::new();
        for _ in 0..100 {
            low.on_delivery(true, 1.0, 0.1);
        }
        let l_before = low.weight;
        low.apply_reward(1.0);
        let l_delta = low.weight - l_before;

        assert!(h_delta > l_delta * 2.0,
            "high satisfaction ({:.6}) should yield larger dw than low ({:.6})",
            h_delta, l_delta);
    }

    #[test]
    fn prune_old_weak_edges() {
        let mut e = EdgeAffinity::new();
        e.weight = 0.05;
        e.age_blocks = 1500;
        assert!(e.should_prune(1000, 0.1));

        // Too young to prune
        e.age_blocks = 500;
        assert!(!e.should_prune(1000, 0.1));

        // Too strong to prune
        e.age_blocks = 1500;
        e.weight = 0.5;
        assert!(!e.should_prune(1000, 0.1));
    }
}
