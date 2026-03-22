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

// === S39 Navigation Reward ===

/// Fraction of max_speed below which an exit is "passive" (no departure reward).
pub const DEPARTURE_SPEED_FRAC: f32 = 0.1;
/// Exit speed must be ≥ this ratio × entry speed for slingshot.
pub const SLINGSHOT_SPEED_RATIO: f32 = 1.2;
/// Min distance must be < this fraction of radius for slingshot depth.
pub const SLINGSHOT_DEPTH_FRAC: f32 = 0.5;
/// Frames inside a well before trapping stress begins (~2.5s at 60Hz).
pub const TRAP_ONSET_TICKS: u64 = 150;
/// Fraction of max_speed below which an organism counts as "stuck".
pub const TRAP_SPEED_FRAC: f32 = 0.05;
/// Trap stress accumulation rate per frame tick.
pub const TRAP_STRESS_RATE: f32 = 0.002;
/// Frames between departure and arrival for a transition event (~5s at 60Hz).
pub const TRANSITION_WINDOW_TICKS: u64 = 300;
/// Blending weight for navigation reward vs homeostatic valence.
pub const NAV_WEIGHT: f32 = 0.5;
/// Exponential decay multiplier for trap stress per frame when not visiting.
pub const TRAP_STRESS_DECAY: f32 = 0.97;

/// Per-organism navigation state for a single gravity well visit.
#[derive(Debug, Clone)]
pub struct WellVisit {
    pub well_id: u32,
    pub well_radius: f32,
    pub entry_tick: u64,
    pub entry_speed: f32,
    pub min_distance: f32,
    pub current_distance: f32,
    pub consonance: f32,
}

/// Per-organism navigation tracker across all wells.
#[derive(Debug, Clone)]
pub struct WellTracker {
    /// Active visits: organism is currently inside this well's radius.
    pub active_visits: std::collections::HashMap<u32, WellVisit>,
    /// Last well departed: (well_id, tick, consonance).
    pub last_departure: Option<(u32, u64, f32)>,
    pub last_departure_speed: f32,
    /// Accumulated navigation valence delta this frame (reset each frame).
    pub nav_valence_delta: f32,
    /// Trapping stress accumulator per well [0, 1].
    pub trap_stress: std::collections::HashMap<u32, f32>,
}

impl WellTracker {
    pub fn new() -> Self {
        Self {
            active_visits: std::collections::HashMap::new(),
            last_departure: None,
            last_departure_speed: 0.0,
            nav_valence_delta: 0.0,
            trap_stress: std::collections::HashMap::new(),
        }
    }

    /// Reset accumulated delta before processing a new frame.
    pub fn reset_delta(&mut self) {
        self.nav_valence_delta = 0.0;
    }

    /// Process one organism-well pair for the current frame.
    pub fn process_well(
        &mut self,
        well_id: u32,
        distance: f32,
        well_radius: f32,
        speed: f32,
        max_speed: f32,
        consonance: f32,
        scale_affinity: f32,
        current_tick: u64,
    ) {
        let was_inside = self.active_visits.contains_key(&well_id);
        let is_inside = distance < well_radius;

        if !was_inside && is_inside {
            // === E1: Arrival ===
            self.active_visits.insert(well_id, WellVisit {
                well_id,
                well_radius,
                entry_tick: current_tick,
                entry_speed: speed,
                min_distance: distance,
                current_distance: distance,
                consonance,
            });
            self.nav_valence_delta += 0.05 * consonance * scale_affinity;

            // === E6: Transition check ===
            if let Some((last_id, last_tick, last_consonance)) = self.last_departure {
                if last_id != well_id && current_tick.saturating_sub(last_tick) < TRANSITION_WINDOW_TICKS {
                    let elapsed = (current_tick - last_tick) as f32;
                    let directness = (1.0 - elapsed / TRANSITION_WINDOW_TICKS as f32).max(0.3);
                    let avg_consonance = (last_consonance + consonance) * 0.5;
                    self.nav_valence_delta += 0.15 * avg_consonance * scale_affinity * directness;
                }
            }
        } else if was_inside && is_inside {
            // === Still inside: update tracking + E5: Trapping ===
            if let Some(visit) = self.active_visits.get_mut(&well_id) {
                visit.current_distance = distance;
                visit.min_distance = visit.min_distance.min(distance);

                let dwell_ticks = current_tick.saturating_sub(visit.entry_tick);
                let trap_speed_threshold = max_speed * TRAP_SPEED_FRAC;
                if dwell_ticks >= TRAP_ONSET_TICKS && speed < trap_speed_threshold {
                    let stress = self.trap_stress.entry(well_id).or_insert(0.0);
                    *stress = (*stress + TRAP_STRESS_RATE).min(1.0);
                    self.nav_valence_delta -= TRAP_STRESS_RATE * scale_affinity;
                }
            }
        }
        // Exit detection is handled by finalize_frame
    }

    /// Process an exit from a well (called by finalize_frame).
    fn process_exit(
        &mut self,
        visit: WellVisit,
        speed: f32,
        max_speed: f32,
        scale_affinity: f32,
        current_tick: u64,
    ) {
        let departure_speed_threshold = max_speed * DEPARTURE_SPEED_FRAC;

        if speed >= departure_speed_threshold {
            let speed_ratio = speed / visit.entry_speed.max(0.001);
            let deep_enough = visit.min_distance < visit.well_radius * SLINGSHOT_DEPTH_FRAC;

            if speed_ratio >= SLINGSHOT_SPEED_RATIO && deep_enough {
                // === E4: Slingshot (replaces E2) ===
                let speed_gain_factor = (speed_ratio - 1.0).clamp(0.0, 1.0);
                self.nav_valence_delta += 0.20 * visit.consonance * scale_affinity * speed_gain_factor;
            } else {
                // === E2: Departure ===
                let speed_factor = (speed / max_speed).clamp(0.1, 1.0);
                self.nav_valence_delta += 0.10 * visit.consonance * scale_affinity * speed_factor;
            }
        }
        // else: E3 passive exit — no delta

        self.last_departure = Some((visit.well_id, current_tick, visit.consonance));
        self.last_departure_speed = speed;
        self.trap_stress.remove(&visit.well_id);
    }

    /// End-of-frame: detect exits for wells no longer in range.
    pub fn finalize_frame(
        &mut self,
        active_well_ids: &[u32],
        speed: f32,
        max_speed: f32,
        scale_affinity: f32,
        current_tick: u64,
    ) {
        let exited: Vec<u32> = self.active_visits.keys()
            .filter(|id| !active_well_ids.contains(id))
            .copied()
            .collect();

        for well_id in exited {
            if let Some(visit) = self.active_visits.remove(&well_id) {
                self.process_exit(visit, speed, max_speed, scale_affinity, current_tick);
            }
        }
    }

    /// Decay trap stress for wells not currently visited.
    pub fn decay_trap_stress(&mut self, active_well_ids: &[u32]) {
        self.trap_stress.retain(|id, stress| {
            if !active_well_ids.contains(id) {
                *stress *= TRAP_STRESS_DECAY;
                *stress > 0.001 // remove when negligible
            } else {
                true
            }
        });
    }

    /// Maximum trap stress across all wells.
    pub fn max_trap_stress(&self) -> f32 {
        self.trap_stress.values().copied().fold(0.0_f32, f32::max)
    }
}

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

    // === S39 Navigation Reward Tests ===

    #[test]
    fn nav_arrival_detection() {
        let mut t = WellTracker::new();
        // Organism enters well radius
        t.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.8, 10);
        assert!(t.nav_valence_delta > 0.0, "arrival should produce positive delta: {}", t.nav_valence_delta);
        assert!(t.active_visits.contains_key(&0), "should have active visit");
        // Expected: 0.05 * 0.8 * 0.8 = 0.032
        assert!((t.nav_valence_delta - 0.032).abs() < 0.001);
    }

    #[test]
    fn nav_departure_detection() {
        let mut t = WellTracker::new();
        // Enter well
        t.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.8, 10);
        let arrival_delta = t.nav_valence_delta;
        t.reset_delta();
        // Exit with speed above departure threshold (200 * 0.1 = 20)
        t.finalize_frame(&[], 50.0, 200.0, 0.8, 20);
        assert!(t.nav_valence_delta > 0.0, "departure should produce positive delta: {}", t.nav_valence_delta);
        assert!(!t.active_visits.contains_key(&0), "visit should be removed");
        assert!(t.last_departure.is_some(), "should record departure");
    }

    #[test]
    fn nav_passive_exit() {
        let mut t = WellTracker::new();
        t.process_well(0, 180.0, 200.0, 5.0, 200.0, 0.8, 0.8, 10);
        t.reset_delta();
        // Exit with speed below departure threshold (200 * 0.1 = 20)
        t.finalize_frame(&[], 5.0, 200.0, 0.8, 20);
        assert!((t.nav_valence_delta).abs() < 0.001, "passive exit should give zero delta: {}", t.nav_valence_delta);
    }

    #[test]
    fn nav_slingshot_detection() {
        let mut t = WellTracker::new();
        // Enter at low speed
        t.process_well(0, 190.0, 200.0, 20.0, 200.0, 1.0, 1.0, 10);
        t.reset_delta();
        // Dive deep (update min_distance)
        t.process_well(0, 50.0, 200.0, 30.0, 200.0, 1.0, 1.0, 15);
        t.reset_delta();
        // Exit at higher speed (ratio = 30/20 = 1.5 ≥ 1.2, depth 50 < 200*0.5=100)
        t.finalize_frame(&[], 30.0, 200.0, 1.0, 20);
        let slingshot_delta = t.nav_valence_delta;

        // Compare with normal departure
        let mut t2 = WellTracker::new();
        t2.process_well(0, 190.0, 200.0, 25.0, 200.0, 1.0, 1.0, 10);
        t2.reset_delta();
        // Don't dive deep — stay near edge
        t2.process_well(0, 180.0, 200.0, 25.0, 200.0, 1.0, 1.0, 15);
        t2.reset_delta();
        // Exit at same speed (ratio = 25/25 = 1.0 < 1.2 → normal departure)
        t2.finalize_frame(&[], 25.0, 200.0, 1.0, 20);
        let departure_delta = t2.nav_valence_delta;

        assert!(slingshot_delta > departure_delta,
            "slingshot ({}) should exceed departure ({})", slingshot_delta, departure_delta);
    }

    #[test]
    fn nav_trapping_detection() {
        let mut t = WellTracker::new();
        t.process_well(0, 100.0, 200.0, 5.0, 200.0, 0.8, 0.8, 0);
        t.reset_delta();

        // Tick past trap onset while stuck (speed < 200*0.05=10)
        for tick in 1..=200 {
            t.process_well(0, 100.0, 200.0, 5.0, 200.0, 0.8, 0.8, tick);
        }

        assert!(t.nav_valence_delta < 0.0, "trapping should produce negative delta: {}", t.nav_valence_delta);
        assert!(t.trap_stress.get(&0).copied().unwrap_or(0.0) > 0.0, "should accumulate trap stress");
    }

    #[test]
    fn nav_transition_detection() {
        let mut t = WellTracker::new();
        // Enter well 0
        t.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.8, 10);
        t.reset_delta();
        // Depart well 0
        t.finalize_frame(&[], 50.0, 200.0, 0.8, 20);
        t.reset_delta();
        // Arrive at well 1 within transition window
        t.process_well(1, 180.0, 200.0, 50.0, 200.0, 0.5, 0.8, 100);
        let transition_delta = t.nav_valence_delta;

        // Compare with arrival-only (no prior departure)
        let mut t2 = WellTracker::new();
        t2.process_well(1, 180.0, 200.0, 50.0, 200.0, 0.5, 0.8, 100);
        let arrival_only_delta = t2.nav_valence_delta;

        assert!(transition_delta > arrival_only_delta,
            "transition ({}) should exceed arrival-only ({})", transition_delta, arrival_only_delta);
    }

    #[test]
    fn nav_kkit_immunity() {
        let mut t = WellTracker::new();
        // scale_affinity = 0.0 → all deltas should be zero
        t.process_well(0, 180.0, 200.0, 50.0, 200.0, 1.0, 0.0, 10);
        assert!((t.nav_valence_delta).abs() < 0.001, "KKIT should get zero delta: {}", t.nav_valence_delta);
    }

    #[test]
    fn nav_species_scaling() {
        // Same event, different scale_affinity
        let mut t_dron = WellTracker::new();
        t_dron.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.3, 10);

        let mut t_hoso = WellTracker::new();
        t_hoso.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.8, 10);

        let ratio = t_hoso.nav_valence_delta / t_dron.nav_valence_delta;
        assert!((ratio - 0.8 / 0.3).abs() < 0.01,
            "delta ratio should be 0.8/0.3={:.2}, got {:.2}", 0.8 / 0.3, ratio);
    }

    #[test]
    fn nav_trap_stress_decay() {
        let mut t = WellTracker::new();
        t.trap_stress.insert(0, 0.5);
        // Well 0 is NOT in active list → should decay
        t.decay_trap_stress(&[]);
        let s = t.trap_stress.get(&0).copied().unwrap_or(0.0);
        assert!((s - 0.5 * TRAP_STRESS_DECAY).abs() < 0.001,
            "should decay: expected {}, got {}", 0.5 * TRAP_STRESS_DECAY, s);
    }

    #[test]
    fn nav_multiple_wells() {
        let mut t = WellTracker::new();
        // Enter both wells simultaneously
        t.process_well(0, 180.0, 200.0, 50.0, 200.0, 0.8, 0.8, 10);
        t.process_well(1, 150.0, 200.0, 50.0, 200.0, 0.5, 0.8, 10);
        assert_eq!(t.active_visits.len(), 2, "should track both wells");
        // Both arrivals contribute independently
        let expected = 0.05 * 0.8 * 0.8 + 0.05 * 0.5 * 0.8;
        assert!((t.nav_valence_delta - expected).abs() < 0.001,
            "combined delta: expected {}, got {}", expected, t.nav_valence_delta);
    }
}
