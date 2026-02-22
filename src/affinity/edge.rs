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
/// Weight evolves through Hebbian learning: edges that carry valid,
/// high-magnitude signals to happy modules get stronger. Edges that
/// carry type-mismatched or low-impact signals decay.
#[derive(Debug, Clone)]
pub struct EdgeAffinity {
    /// Routing strength [0, 1]. Higher = more signal gets through.
    pub weight: f32,
    /// Eligibility trace: did this edge fire recently? Decays each tick.
    pub eligibility: f32,
    /// EWMA fraction of type-valid deliveries.
    pub goodput: f32,
    /// EWMA downstream signal magnitude.
    pub impact: f32,
    /// How many ticks this edge has existed.
    pub age_blocks: u64,
}

impl EdgeAffinity {
    pub fn new() -> Self {
        Self {
            weight: 0.5,
            eligibility: 0.0,
            goodput: 1.0,
            impact: 0.0,
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

    /// Record a delivery attempt. Updates goodput, impact, and eligibility.
    pub fn on_delivery(&mut self, type_valid: bool, magnitude: f32) {
        let valid_f = if type_valid { 1.0 } else { 0.0 };
        self.goodput = self.goodput * (1.0 - EWMA_ALPHA) + valid_f * EWMA_ALPHA;
        self.impact = self.impact * (1.0 - EWMA_ALPHA) + magnitude * EWMA_ALPHA;
        // Eligibility spikes on any delivery — marks the edge as "active".
        // Full spike for valid deliveries, partial for invalid (so negative
        // valence can still weaken the edge through Hebbian update).
        let spike = if type_valid { 1.0 } else { 0.5 };
        self.eligibility = (self.eligibility + spike).min(1.0);
    }

    /// Hebbian weight update: reward-modulated by receiving module's valence.
    /// `dw = LR * eligibility * valence * goodput`
    pub fn apply_reward(&mut self, valence: f32) {
        let dw = LR * self.eligibility * valence * self.goodput;
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
        assert_eq!(e.goodput, 1.0);
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
    fn good_deliveries_maintain_goodput() {
        let mut e = EdgeAffinity::new();
        for _ in 0..20 {
            e.on_delivery(true, 1.0);
        }
        assert!(e.goodput > 0.9, "goodput should stay high: {}", e.goodput);
        assert!(e.impact > 0.5, "impact should rise: {}", e.impact);
    }

    #[test]
    fn bad_deliveries_tank_goodput() {
        let mut e = EdgeAffinity::new();
        for _ in 0..100 {
            e.on_delivery(false, 0.0);
        }
        assert!(e.goodput < 0.1, "goodput should drop: {}", e.goodput);
    }

    #[test]
    fn positive_reward_strengthens() {
        let mut e = EdgeAffinity::new();
        e.on_delivery(true, 1.0); // spike eligibility
        let before = e.weight;
        e.apply_reward(1.0); // positive valence
        assert!(e.weight > before, "weight should increase");
    }

    #[test]
    fn negative_reward_weakens() {
        let mut e = EdgeAffinity::new();
        e.on_delivery(true, 1.0);
        let before = e.weight;
        e.apply_reward(-1.0);
        assert!(e.weight < before, "weight should decrease");
    }

    #[test]
    fn weight_clamped_to_unit() {
        let mut e = EdgeAffinity::new();
        e.weight = 0.99;
        e.eligibility = 1.0;
        e.goodput = 1.0;
        e.apply_reward(100.0);
        assert!(e.weight <= 1.0);

        e.weight = 0.01;
        e.apply_reward(-100.0);
        assert!(e.weight >= 0.0);
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
