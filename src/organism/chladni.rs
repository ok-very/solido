//! CPU-side Chladni node position computation + NodeWell energy state machine.
//!
//! Mirrors the sub-node geometry from biofield.wgsl so CPU physics can compute
//! node well forces. Each organism's sub-nodes act as mini gravity wells that
//! draw on the host's energy, creating predator/prey/altruistic dynamics.

use std::f32::consts::TAU;

/// Maximum sub-nodes per organism (matches GPU MAX_SUBNODES).
pub const MAX_SUBNODES: usize = 6;

// NodeWell energy constants (faster-cycling than gravity wells)
/// Energy regeneration rate when no visitors.
pub const NODE_REGEN_RATE: f32 = 0.015;
/// Energy below this threshold triggers dormancy.
pub const NODE_DORMANT_THRESHOLD: f32 = 0.1;
/// Ticks at 120Hz before dormant well reactivates (~1.5s).
pub const NODE_DORMANT_COOLDOWN: u32 = 180;
/// Attraction range in pixels.
pub const NODE_WELL_RANGE: f32 = 120.0;
/// Base attraction strength.
pub const NODE_WELL_STRENGTH: f32 = 4.0;

/// Energy state of a node well.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeState {
    Active,
    Dormant { cooldown: u32 },
}

/// A mini gravity well anchored at a Chladni sub-node position.
#[derive(Debug, Clone)]
pub struct NodeWell {
    pub position: [f32; 2],
    pub energy: f32,
    pub state: NodeState,
}

impl NodeWell {
    pub fn new(position: [f32; 2]) -> Self {
        Self {
            position,
            energy: 1.0,
            state: NodeState::Active,
        }
    }

    /// Tick energy: drain when visited, regen in solitude, dormancy when depleted.
    pub fn tick_energy(&mut self, visitor_count: u32, drain_rate: f32, regen_rate: f32) {
        match self.state {
            NodeState::Active => {
                if visitor_count > 0 {
                    // Drain proportional to visitors (sub-linear with sqrt)
                    let drain = drain_rate * (visitor_count as f32).sqrt();
                    self.energy = (self.energy - drain).max(0.0);
                } else {
                    // Regen in solitude — diminishing returns near max (logistic)
                    self.energy = (self.energy + regen_rate * (1.0 - self.energy)).min(1.0);
                }
                // Check dormancy
                if self.energy < NODE_DORMANT_THRESHOLD {
                    self.state = NodeState::Dormant { cooldown: NODE_DORMANT_COOLDOWN };
                    self.energy = 0.0;
                }
            }
            NodeState::Dormant { ref mut cooldown } => {
                if *cooldown > 0 {
                    *cooldown -= 1;
                } else {
                    // Reactivate with partial energy
                    self.state = NodeState::Active;
                    self.energy = 0.3;
                }
            }
        }
    }

    /// Whether this well can attract visitors.
    pub fn is_active(&self) -> bool {
        matches!(self.state, NodeState::Active) && self.energy > 0.01
    }
}

/// Compute world-space position of a single sub-node.
///
/// Mirrors biofield.wgsl sub-node geometry exactly:
/// - Golden-ratio angular spacing
/// - Chladni standing wave: cos(m*θ + ω1*phase) * cos(n*θ + ω2*phase)
/// - Directional crawl bias
/// - Clamped reach
pub fn node_world_position(
    center: [f32; 2],
    vis_radius: f32,
    node_idx: usize,
    node_count: usize,
    id_f: f32,
    m: f32,
    n: f32,
    phase: f32,
    amp: f32,
    energy: f32,
    heading: f32,
    speed: f32,
) -> [f32; 2] {
    let fn_sub = node_count as f32;
    let fi = node_idx as f32;

    // Fixed world-space angular position (golden ratio offset per organism)
    let theta_i = fi / fn_sub * TAU + id_f * 0.618034;

    // Per-species frequency scalars (must match GPU exactly)
    let omega1 = m * 0.35 + 0.5 + (id_f * 0.317).fract() * 0.3;
    let omega2 = n * 0.25 + 0.4 + (id_f * 0.223).fract() * 0.2;

    // Chladni standing wave
    let chladni = (m * theta_i + omega1 * phase).cos()
                * (n * theta_i + omega2 * phase).cos();

    // Directional crawl bias
    let speed_factor = (speed / 120.0).min(1.0);
    let crawl_bias = (theta_i - heading).cos() * speed_factor * 0.75;

    // Combined extension
    let dance_intensity = 0.15 + energy * 0.85;
    let extension = amp * chladni * dance_intensity + crawl_bias * amp;

    // Radial reach (clamped)
    let reach = vis_radius * (0.25 + extension * 0.45).clamp(0.15, 0.75);

    // World-space node position
    [
        center[0] + theta_i.cos() * reach,
        center[1] + theta_i.sin() * reach,
    ]
}

/// Compute cell_id matching GPU payload formula from registry.rs.
/// `(idx % 9 + 1) * 10 + (idx / 9 % 9 + 1)`
pub fn cell_id_f(organism_index: usize) -> f32 {
    let i = organism_index as u32;
    ((i % 9 + 1) * 10 + (i / 9 % 9 + 1)) as f32
}

/// Compute all sub-node world positions for an organism.
///
/// Returns stack-allocated array + count. No heap allocation.
pub fn compute_all_node_positions(
    center: [f32; 2],
    vis_radius: f32,
    id_f: f32,
    m: f32,
    n: f32,
    phase: f32,
    amp: f32,
    energy: f32,
    heading: f32,
    speed: f32,
    node_count: usize,
) -> ([[f32; 2]; MAX_SUBNODES], usize) {
    let count = node_count.min(MAX_SUBNODES);
    let mut positions = [[0.0f32; 2]; MAX_SUBNODES];

    for i in 0..count {
        positions[i] = node_world_position(
            center, vis_radius, i, count, id_f,
            m, n, phase, amp, energy, heading, speed,
        );
    }

    (positions, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_positions_golden_ratio_spacing() {
        // Sub-nodes should be angularly distributed, not overlapping
        let (positions, count) = compute_all_node_positions(
            [500.0, 500.0], 100.0, 11.0,
            2.0, 1.0, 0.0, 0.3, 0.5, 0.0, 0.0, 4,
        );
        assert_eq!(count, 4);

        // All nodes should be distinct positions
        for i in 0..count {
            for j in (i + 1)..count {
                let dx = positions[i][0] - positions[j][0];
                let dy = positions[i][1] - positions[j][1];
                let dist = (dx * dx + dy * dy).sqrt();
                assert!(dist > 1.0, "nodes {} and {} overlap: dist={}", i, j, dist);
            }
        }
    }

    #[test]
    fn node_positions_within_radius() {
        // All nodes should be within visual radius of center
        let center = [500.0, 500.0];
        let vis_r = 100.0;
        let (positions, count) = compute_all_node_positions(
            center, vis_r, 22.0,
            3.0, 2.0, 1.5, 0.4, 0.8, 0.5, 50.0, 6,
        );
        assert_eq!(count, 6);

        for i in 0..count {
            let dx = positions[i][0] - center[0];
            let dy = positions[i][1] - center[1];
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(
                dist <= vis_r * 0.76, // max reach = 0.75 * vis_r
                "node {} at dist {} exceeds max reach {}",
                i, dist, vis_r * 0.76
            );
        }
    }

    #[test]
    fn node_positions_crawl_bias() {
        // Forward-facing nodes should extend further when speed > 0
        let center = [500.0, 500.0];
        let vis_r = 100.0;
        let heading = 0.0; // pointing right

        let (pos_still, _) = compute_all_node_positions(
            center, vis_r, 11.0, 2.0, 1.0, 0.0, 0.3, 0.5, heading, 0.0, 4,
        );
        let (pos_moving, _) = compute_all_node_positions(
            center, vis_r, 11.0, 2.0, 1.0, 0.0, 0.3, 0.5, heading, 100.0, 4,
        );

        // At least one node should be further from center when moving
        let mut any_further = false;
        for i in 0..4 {
            let d_still = ((pos_still[i][0] - center[0]).powi(2)
                + (pos_still[i][1] - center[1]).powi(2)).sqrt();
            let d_moving = ((pos_moving[i][0] - center[0]).powi(2)
                + (pos_moving[i][1] - center[1]).powi(2)).sqrt();
            if d_moving > d_still + 1.0 {
                any_further = true;
            }
        }
        assert!(any_further, "crawl bias should push forward-facing nodes further when moving");
    }

    #[test]
    fn cell_id_f_matches_gpu() {
        // Verify formula matches registry.rs: (i%9+1)*10 + (i/9%9+1)
        assert_eq!(cell_id_f(0), 11.0); // (0%9+1)*10 + (0/9%9+1) = 10+1
        assert_eq!(cell_id_f(1), 21.0); // (1%9+1)*10 + (1/9%9+1) = 20+1
        assert_eq!(cell_id_f(9), 11.0 + 1.0); // wraps: (9%9+1)*10 + (9/9%9+1) = 10+2 = 12
    }

    #[test]
    fn node_well_energy_drains_with_visitors() {
        let mut well = NodeWell::new([100.0, 100.0]);
        assert_eq!(well.energy, 1.0);

        // Drain with 1 visitor
        for _ in 0..10 {
            well.tick_energy(1, 0.01, 0.015);
        }
        assert!(well.energy < 1.0, "energy should drain: {}", well.energy);
    }

    #[test]
    fn node_well_regens_in_solitude() {
        let mut well = NodeWell::new([100.0, 100.0]);
        well.energy = 0.5;

        for _ in 0..10 {
            well.tick_energy(0, 0.01, 0.015);
        }
        assert!(well.energy > 0.5, "energy should regen: {}", well.energy);
    }

    #[test]
    fn node_well_goes_dormant() {
        let mut well = NodeWell::new([100.0, 100.0]);

        // Heavy drain
        for _ in 0..200 {
            well.tick_energy(3, 0.05, 0.001);
        }
        assert!(
            matches!(well.state, NodeState::Dormant { .. }),
            "well should go dormant after heavy drain: {:?}", well.state
        );
    }

    #[test]
    fn node_well_recovers_from_dormancy() {
        let mut well = NodeWell::new([100.0, 100.0]);
        well.state = NodeState::Dormant { cooldown: 5 };
        well.energy = 0.0;

        for _ in 0..10 {
            well.tick_energy(0, 0.01, 0.015);
        }
        assert!(
            matches!(well.state, NodeState::Active),
            "well should recover from dormancy: {:?}", well.state
        );
        assert!(well.energy > 0.0, "energy should be non-zero after recovery");
    }

    #[test]
    fn node_well_dormant_blocks_attraction() {
        let mut well = NodeWell::new([100.0, 100.0]);
        well.state = NodeState::Dormant { cooldown: 100 };
        well.energy = 0.0;
        assert!(!well.is_active(), "dormant well should not attract");
    }

    #[test]
    fn max_subnodes_cap() {
        // Even if we request more than MAX_SUBNODES, we get capped
        let (_, count) = compute_all_node_positions(
            [0.0, 0.0], 100.0, 11.0, 2.0, 1.0, 0.0, 0.3, 0.5, 0.0, 0.0, 100,
        );
        assert_eq!(count, MAX_SUBNODES);
    }

    #[test]
    fn node_regen_diminishing_near_max() {
        // At high energy, regen should be slower than at low energy (logistic)
        let mut well_high = NodeWell::new([0.0, 0.0]);
        well_high.energy = 0.95;
        let mut well_low = NodeWell::new([0.0, 0.0]);
        well_low.energy = 0.5;

        let e_high_before = well_high.energy;
        let e_low_before = well_low.energy;

        well_high.tick_energy(0, 0.01, NODE_REGEN_RATE);
        well_low.tick_energy(0, 0.01, NODE_REGEN_RATE);

        let gain_high = well_high.energy - e_high_before;
        let gain_low = well_low.energy - e_low_before;

        assert!(
            gain_low > gain_high,
            "regen at 0.5 ({}) should be faster than at 0.95 ({})",
            gain_low, gain_high
        );
    }
}
