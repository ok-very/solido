#![allow(dead_code)]
/// Gravity Wells — convex UV lenses that focus substrate energy.
///
/// Each well has a position and radius. Wells bend substrate texture sampling,
/// concentrating nearby pixels toward the well center. Organisms follow food
/// (focused substrate energy), not harmony. The energy state machine
/// (Healthy → Wavering → Dormant) drives lens power instead of harmonic influence.

// === Well Energy Constants ===
/// Regen rate per tick when substrate is healthy.
pub const REGEN_RATE: f32 = 0.01;
/// Drain rate per tick under depletion pressure.
pub const BASE_DRAIN: f32 = 0.005;
/// Energy threshold below which wavering begins.
pub const WAVER_THRESHOLD: f32 = 0.5;
/// Wavering ticks before dormancy (~2.5s at 120Hz).
pub const DORMANT_ONSET_TICKS: u32 = 300;
/// Dormancy duration before reactivation (~5s at 120Hz).
pub const DORMANT_COOLDOWN: u32 = 600;
/// Energy level when waking from dormancy.
pub const DORMANT_SEED_ENERGY: f32 = 0.1;

// === Well Energy State Machine ===

/// Per-well energy store, updated each frame.
#[derive(Clone, Debug)]
pub struct WellEnergy {
    pub well_id: u32,
    /// Current energy level [0, 1]. Drains when occupied, regenerates when empty.
    pub energy: f32,
    /// Regeneration state machine.
    pub regen_state: RegenState,
    /// Ticks spent in current regen state.
    pub state_ticks: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegenState {
    /// energy > 0.5 or unoccupied. Constant regen proportional to emptiness.
    Healthy,
    /// energy <= 0.5 AND occupied. Stochastic regen — increasingly unreliable.
    Wavering,
    /// Extended wavering while crowded. Well shuts off for cooldown.
    Dormant { cooldown_remaining: u32 },
}

impl WellEnergy {
    pub fn new(well_id: u32) -> Self {
        Self {
            well_id,
            energy: 1.0,
            regen_state: RegenState::Healthy,
            state_ticks: 0,
        }
    }

    /// Tick energy drain and regeneration for one frame.
    /// `depletion_pressure`: [0,1] how much substrate organisms have consumed in well region.
    /// `local_energy`: [0,1] mean substrate energy in well region (for regen gating).
    pub fn tick(&mut self, depletion_pressure: f32, local_energy: f32) {
        // Drain based on substrate depletion in the well region
        if depletion_pressure > 0.01 {
            let drain = BASE_DRAIN * depletion_pressure;
            self.energy = (self.energy - drain).max(0.0);
        }

        let is_depleted = depletion_pressure > 0.3;

        // Regen state machine
        match &mut self.regen_state {
            RegenState::Healthy => {
                // Only regen when local substrate has recovered
                if local_energy > 0.5 {
                    self.energy = (self.energy + REGEN_RATE * (1.0 - self.energy)).min(1.0);
                }
                if self.energy <= WAVER_THRESHOLD && is_depleted {
                    self.regen_state = RegenState::Wavering;
                    self.state_ticks = 0;
                }
            }
            RegenState::Wavering => {
                self.state_ticks += 1;
                // Stochastic regen: probability proportional to remaining energy
                let pseudo_rand = ((self.state_ticks.wrapping_mul(2654435761)) >> 16) as f32
                    / 65535.0;
                if pseudo_rand < self.energy && local_energy > 0.5 {
                    self.energy = (self.energy + REGEN_RATE * 0.5).min(1.0);
                }
                // Recovery: energy above threshold or substrate recovered
                if self.energy > WAVER_THRESHOLD || !is_depleted {
                    self.regen_state = RegenState::Healthy;
                    self.state_ticks = 0;
                }
                // Dormancy: extended wavering under heavy depletion
                else if self.state_ticks >= DORMANT_ONSET_TICKS && depletion_pressure > 0.5 {
                    self.regen_state = RegenState::Dormant {
                        cooldown_remaining: DORMANT_COOLDOWN,
                    };
                }
            }
            RegenState::Dormant { cooldown_remaining } => {
                self.energy = 0.0;
                if *cooldown_remaining > 0 {
                    *cooldown_remaining -= 1;
                } else {
                    self.energy = DORMANT_SEED_ENERGY;
                    self.regen_state = RegenState::Healthy;
                    self.state_ticks = 0;
                }
            }
        }
    }

    /// Compute lens power from well energy.
    /// Full energy → strongest lens (0.2), depleted → flat (1.0).
    pub fn lens_power(&self) -> f32 {
        0.2 + 0.8 * (1.0 - self.energy)
    }
}

/// A spatial substrate lens with a position and radius.
#[derive(Clone, Debug)]
pub struct GravityWell {
    pub id: u32,
    pub position: [f32; 2],
    pub radius: f32,          // lens radius in pixels
    pub strength: f32,        // [0, 1]
    pub hue: f32,             // visual color (0-360)
}

/// Collection of gravity wells forming a spatial lens field.
pub struct GravityField {
    wells: Vec<GravityWell>,
    next_id: u32,
}

impl GravityField {
    pub fn new() -> Self {
        Self {
            wells: Vec::new(),
            next_id: 0,
        }
    }

    /// Generate wells at deterministic positions from seed.
    pub fn generate(count: usize, bounds: [f32; 4], seed: u64) -> Self {
        let hues: [f32; 6] = [0.0, 60.0, 120.0, 200.0, 280.0, 330.0];

        let [min_x, min_y, max_x, max_y] = bounds;
        let margin = 100.0;

        let mut wells = Vec::with_capacity(count);
        for i in 0..count {
            let h1 = seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(0x6c62272e07bb0142);
            let h2 = h1
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(0x6c62272e07bb0142);
            let nx = (h1 >> 16 & 0xFFFF) as f32 / 65535.0;
            let ny = (h2 >> 16 & 0xFFFF) as f32 / 65535.0;

            let x = min_x + margin + nx * (max_x - min_x - 2.0 * margin);
            let y = min_y + margin + ny * (max_y - min_y - 2.0 * margin);

            // Radius: 200-350px based on hash
            let h3 = h2
                .wrapping_mul(0xbf58476d1ce4e5b9)
                .wrapping_add(0x94d049bb133111eb);
            let radius = 200.0 + ((h3 >> 32 & 0xFFFF) as f32 / 65535.0) * 150.0;

            wells.push(GravityWell {
                id: i as u32,
                position: [x, y],
                radius,
                strength: 0.6,
                hue: hues[i % hues.len()],
            });
        }

        Self {
            wells,
            next_id: count as u32,
        }
    }

    pub fn wells(&self) -> &[GravityWell] {
        &self.wells
    }

    pub fn wells_mut(&mut self) -> &mut [GravityWell] {
        &mut self.wells
    }

    pub fn well_mut(&mut self, id: u32) -> Option<&mut GravityWell> {
        self.wells.iter_mut().find(|w| w.id == id)
    }

    /// Regenerate wells with a new count, preserving deterministic layout.
    pub fn regenerate(&mut self, count: usize, bounds: [f32; 4], seed: u64) {
        *self = GravityField::generate(count, bounds, seed);
    }

    pub fn len(&self) -> usize {
        self.wells.len()
    }
}

/// Map pitch class number (0–11) to note name.
pub fn pitch_class_name(pc: u8) -> &'static str {
    match pc % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => unreachable!(),
    }
}

/// Shift a C-rooted weight array so the root falls on a different pitch class.
///
/// `transpose_weights(c_major, 9)` moves the root weight (index 0) to index 9 (A),
/// so the organism hears A-major intervals.
pub fn transpose_weights(weights: &[f32; 12], root: u8) -> [f32; 12] {
    let shift = root as usize % 12;
    let mut out = [0.0f32; 12];
    for i in 0..12 {
        out[(i + shift) % 12] = weights[i];
    }
    out
}

// === Substrate Navigation Constants ===
/// Blending weight for navigation reward vs homeostatic valence.
pub const NAV_WEIGHT: f32 = 0.5;
/// Energy delta threshold for discovery reward.
pub const DISCOVERY_THRESHOLD: f32 = 0.15;
/// Local energy below this triggers departure reward when moving.
pub const DEPARTURE_ENERGY_THRESHOLD: f32 = 0.3;
/// Local energy below this triggers starvation pressure.
pub const STARVATION_THRESHOLD: f32 = 0.2;
/// Speed below which organism counts as "stuck" (px/s).
pub const STARVATION_SPEED: f32 = 10.0;
/// Minimum speed for departure boost (px/s).
pub const DEPARTURE_SPEED: f32 = 20.0;
/// Minimum speed for grazing run tracking (px/s).
pub const GRAZING_SPEED: f32 = 15.0;
/// Seconds of sustained grazing before reward.
pub const GRAZING_RUN_DURATION: f32 = 1.0;


#[cfg(test)]
mod tests {
    use super::*;

    fn diatonic_weights() -> [f32; 12] {
        // C-rooted diatonic: C D E F G A B
        [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0]
    }

    #[test]
    fn transpose_weights_all_roots() {
        let w = diatonic_weights();
        let sum_orig: f32 = w.iter().sum();

        // Identity: root=0 returns same weights
        let t0 = transpose_weights(&w, 0);
        for i in 0..12 {
            assert!((t0[i] - w[i]).abs() < 1e-6, "identity: index {}", i);
        }

        // All roots: sum preservation + root weight placement
        for root in 0..12u8 {
            let t = transpose_weights(&w, root);
            let sum_t: f32 = t.iter().sum();
            assert!((sum_t - sum_orig).abs() < 1e-5,
                "root {}: sum {} != {}", root, sum_t, sum_orig);
            assert!((t[root as usize] - w[0]).abs() < 1e-6,
                "root {}: root weight should be {} at index {}, got {}",
                root, w[0], root, t[root as usize]);
        }

        // Spot check: root=9 (A)
        let t9 = transpose_weights(&w, 9);
        assert!((t9[9] - 2.0).abs() < 1e-6, "A should have root weight 2.0");
        assert!((t9[4] - 1.8).abs() < 1e-6, "E should have fifth weight 1.8");
    }

    #[test]
    fn generate_properties() {
        let bounds = [0.0, 0.0, 1200.0, 700.0];

        // Correct count
        let field = GravityField::generate(3, bounds, 42);
        assert_eq!(field.wells().len(), 3);

        // Bounds + radius range (larger count for coverage)
        let field5 = GravityField::generate(10, [0.0, 0.0, 2000.0, 1000.0], 99);
        for well in field5.wells() {
            assert!(well.position[0] >= 0.0 && well.position[0] <= 2000.0,
                "x out of bounds: {}", well.position[0]);
            assert!(well.position[1] >= 0.0 && well.position[1] <= 1000.0,
                "y out of bounds: {}", well.position[1]);
            assert!(well.radius >= 200.0 && well.radius <= 350.0,
                "radius out of range: {}", well.radius);
        }

        // Deterministic: same seed = same result
        let a = GravityField::generate(3, bounds, 42);
        let b = GravityField::generate(3, bounds, 42);
        for (wa, wb) in a.wells().iter().zip(b.wells()) {
            assert_eq!(wa.position, wb.position);
        }

        // Different seeds = different results
        let c = GravityField::generate(1, bounds, 1);
        let d = GravityField::generate(1, bounds, 2);
        assert_ne!(c.wells()[0].position, d.wells()[0].position);
    }

    #[test]
    fn wells_mut_modifies() {
        let mut field = GravityField::generate(2, [0.0, 0.0, 1200.0, 700.0], 42);
        field.wells_mut()[0].strength = 0.0;
        assert!((field.wells()[0].strength - 0.0).abs() < 1e-6);
    }

    #[test]
    fn well_mut_by_id() {
        let mut field = GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42);
        assert!(field.well_mut(1).is_some());
        field.well_mut(1).unwrap().radius = 999.0;
        assert!((field.wells()[1].radius - 999.0).abs() < 1e-6);
        assert!(field.well_mut(99).is_none());
    }

    #[test]
    fn regenerate_changes_count() {
        let mut field = GravityField::generate(2, [0.0, 0.0, 1200.0, 700.0], 42);
        assert_eq!(field.len(), 2);
        field.regenerate(5, [0.0, 0.0, 1200.0, 700.0], 42);
        assert_eq!(field.len(), 5);
    }

    #[test]
    fn pitch_class_name_all() {
        assert_eq!(pitch_class_name(0), "C");
        assert_eq!(pitch_class_name(9), "A");
        assert_eq!(pitch_class_name(11), "B");
        assert_eq!(pitch_class_name(12), "C"); // wraps
    }

    // === Well Energy Tests ===

    #[test]
    fn well_energy_drain() {
        let mut we = WellEnergy::new(0);
        we.energy = 0.8;
        we.tick(0.8, 0.5); // high depletion, moderate local energy
        assert!(we.energy < 0.8, "should drain: {}", we.energy);
    }

    #[test]
    fn well_energy_regen_healthy_substrate() {
        let mut we = WellEnergy::new(0);
        we.energy = 0.5;
        we.tick(0.0, 0.8); // no depletion, healthy substrate
        assert!(we.energy > 0.5, "should regen: {}", we.energy);
    }

    #[test]
    fn well_energy_no_regen_depleted_substrate() {
        let mut we = WellEnergy::new(0);
        we.energy = 0.5;
        let before = we.energy;
        we.tick(0.0, 0.2); // no depletion, but substrate is depleted — no regen
        // Energy should stay roughly the same (no regen when local_energy < 0.5)
        assert!((we.energy - before).abs() < 0.001, "should not regen with depleted substrate: {}", we.energy);
    }

    #[test]
    fn regen_state_transitions() {
        let mut we = WellEnergy::new(0);
        // Force energy low and high depletion → should enter Wavering
        we.energy = 0.3;
        we.regen_state = RegenState::Healthy;
        we.tick(0.5, 0.3); // depletion > 0.3 threshold
        assert_eq!(we.regen_state, RegenState::Wavering, "should waver at low energy + depleted");

        // Tick through wavering → dormant
        we.energy = 0.1;
        we.regen_state = RegenState::Wavering;
        we.state_ticks = DORMANT_ONSET_TICKS + 1;
        we.tick(0.6, 0.2); // depletion > 0.5 threshold for dormancy
        assert!(matches!(we.regen_state, RegenState::Dormant { .. }), "should go dormant");

        // Tick through dormancy → healthy (cooldown_remaining=0 means wake this tick)
        we.regen_state = RegenState::Dormant { cooldown_remaining: 0 };
        we.tick(0.0, 0.8);
        assert_eq!(we.regen_state, RegenState::Healthy, "should wake from dormancy");
        assert!((we.energy - DORMANT_SEED_ENERGY).abs() < 0.01, "should have seed energy");
    }

    #[test]
    fn lens_power_range() {
        let mut we = WellEnergy::new(0);
        we.energy = 1.0;
        assert!((we.lens_power() - 0.2).abs() < 0.01, "full energy = strong lens: {}", we.lens_power());
        we.energy = 0.0;
        assert!((we.lens_power() - 1.0).abs() < 0.01, "zero energy = flat: {}", we.lens_power());
        we.energy = 0.5;
        assert!((we.lens_power() - 0.6).abs() < 0.01, "half energy: {}", we.lens_power());
    }

}
