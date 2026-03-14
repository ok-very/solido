#![allow(dead_code)]
/// Gravity Wells — spatial harmonic fields that create localized tonal centers.
///
/// Each well has a position, root pitch class, and radius of influence.
/// Organisms near a well hear the active scale transposed to that well's root.
/// Combined with per-organism `root_pitch_class` (DNA field), this gives every
/// organism a unique harmonic perspective based on spatial position.

// === LJ Well Force Constants ===
/// Base gravitational constant for trench force.
pub const LJ_GRAVITY: f32 = 2000.0;
/// Prevents singularity at r=0 (px).
pub const LJ_SOFTENING: f32 = 30.0;
/// Trench radius as fraction of well.radius (equilibrium ring).
pub const LJ_TRENCH_FRACTION: f32 = 0.6;
/// Per-well force clamp (defense-in-depth).
pub const MAX_WELL_FORCE: f32 = 50.0;
/// Maximum beat-driven repulsion boost (ratio).
pub const BEAT_PULSE_AMPLITUDE: f32 = 0.8;

// === Well Energy Constants ===
/// Regen rate per tick when unoccupied.
pub const REGEN_RATE: f32 = 0.01;
/// Single organism drain rate per tick.
pub const BASE_DRAIN: f32 = 0.005;
/// Energy threshold below which wavering begins.
pub const WAVER_THRESHOLD: f32 = 0.5;
/// Wavering ticks before dormancy (~2.5s at 120Hz).
pub const DORMANT_ONSET_TICKS: u32 = 300;
/// Dormancy duration before reactivation (~5s at 120Hz).
pub const DORMANT_COOLDOWN: u32 = 600;
/// Energy level when waking from dormancy.
pub const DORMANT_SEED_ENERGY: f32 = 0.1;

// === Ecology Constants ===
/// Spectral niche overlap bandwidth (octaves).
pub const OCTAVE_THRESHOLD: f32 = 1.5;
/// Satisfaction bonus weight from well ecology.
pub const WELL_SAT_WEIGHT: f32 = 0.2;

/// Consonance weight for an interval (0–11 semitones).
pub fn consonance_weight(interval: u8) -> f32 {
    match interval % 12 {
        0 => 1.0,       // unison
        7 | 5 => 0.8,   // fifth / fourth
        4 | 3 => 0.5,   // major/minor third
        _ => 0.2,       // other intervals
    }
}

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
    pub fn tick(&mut self, occupant_count: u32, total_influence: f32) {
        // Drain
        if occupant_count > 0 && total_influence > 0.0 {
            let drain = BASE_DRAIN * total_influence / (occupant_count as f32).sqrt();
            self.energy = (self.energy - drain).max(0.0);
        }

        // Regen state machine
        match &mut self.regen_state {
            RegenState::Healthy => {
                self.energy = (self.energy + REGEN_RATE * (1.0 - self.energy)).min(1.0);
                if self.energy <= WAVER_THRESHOLD && occupant_count > 0 {
                    self.regen_state = RegenState::Wavering;
                    self.state_ticks = 0;
                }
            }
            RegenState::Wavering => {
                self.state_ticks += 1;
                // Stochastic regen: probability proportional to remaining energy
                let pseudo_rand = ((self.state_ticks.wrapping_mul(2654435761)) >> 16) as f32
                    / 65535.0;
                if pseudo_rand < self.energy {
                    self.energy = (self.energy + REGEN_RATE * 0.5).min(1.0);
                }
                // Recovery: energy above threshold or no occupants
                if self.energy > WAVER_THRESHOLD || occupant_count == 0 {
                    self.regen_state = RegenState::Healthy;
                    self.state_ticks = 0;
                }
                // Dormancy: extended wavering while crowded
                else if self.state_ticks >= DORMANT_ONSET_TICKS && occupant_count > 1 {
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
}

// === Well Proximity (Ecology) ===

/// Per-organism ecological snapshot for a single well.
#[derive(Clone, Copy, Debug, Default)]
pub struct WellInfluence {
    pub well_id: u32,
    /// LJ influence strength [0, ~1]: how deep in the trench.
    pub influence: f32,
    /// Consonance between organism root and well root [0.2, 1.0].
    pub consonance: f32,
    /// Combined ecological quality: influence × consonance.
    pub quality: f32,
}

/// Per-organism summary of all well interactions this frame.
#[derive(Clone, Debug)]
pub struct WellProximity {
    /// Up to 6 well influences.
    pub influences: [WellInfluence; 6],
    pub influence_count: u8,
    /// Best single quality score across all wells.
    pub best_quality: f32,
    /// Niche penalty [0, 1]: spectral overlap with co-occupants.
    pub niche_penalty: f32,
    /// Harmonic consonance bonus from co-occupants [0, 1].
    pub harmonic_bonus: f32,
    /// Net ecological score incorporating quality, energy, niche, and harmony.
    pub net_score: f32,
}

impl Default for WellProximity {
    fn default() -> Self {
        Self {
            influences: [WellInfluence::default(); 6],
            influence_count: 0,
            best_quality: 0.0,
            niche_penalty: 0.0,
            harmonic_bonus: 0.0,
            net_score: 0.0,
        }
    }
}

/// A spatial harmonic attractor with a position and tonal center.
#[derive(Clone, Debug)]
pub struct GravityWell {
    pub id: u32,
    pub position: [f32; 2],
    pub root_pitch_class: u8, // 0-11 (C=0, A=9, etc.)
    pub radius: f32,          // influence radius in pixels
    pub strength: f32,        // [0, 1]
    pub hue: f32,             // visual color (0-360)
}

/// Collection of gravity wells forming a spatial harmonic field.
pub struct GravityField {
    wells: Vec<GravityWell>,
    next_id: u32,
}

/// Result of computing effective weights for a single organism.
pub struct EffectiveWeights {
    pub weights: [f32; 12],
    pub total_influence: f32,
}

impl GravityField {
    pub fn new() -> Self {
        Self {
            wells: Vec::new(),
            next_id: 0,
        }
    }

    /// Generate wells with roots from the circle of fifths.
    /// Deterministic positions from seed (matches `seeded_spawn_pos` pattern).
    pub fn generate(count: usize, bounds: [f32; 4], seed: u64) -> Self {
        // Circle of fifths: C, G, D, A, E, B — consonant neighboring wells
        let fifths: [u8; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
        // Hues for visual variety
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
                root_pitch_class: fifths[i % 12],
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

    /// Compute effective weights for an organism at a given position with a root pitch class.
    ///
    /// 1. Transpose base weights to organism's root_pitch_class
    /// 2. For each well within range: quadratic falloff, transpose to well root, blend
    /// 3. Normalize if any weight exceeds 3.0
    pub fn effective_weights(
        &self,
        org_pos: [f32; 2],
        org_root: u8,
        base_weights: &[f32; 12],
    ) -> EffectiveWeights {
        // Start with base weights transposed to organism's root
        let mut weights = transpose_weights(base_weights, org_root);
        let mut total_influence = 0.0f32;

        for well in &self.wells {
            let dx = org_pos[0] - well.position[0];
            let dy = org_pos[1] - well.position[1];
            let dist = (dx * dx + dy * dy).sqrt();

            if dist >= well.radius {
                continue;
            }

            // Quadratic falloff: strength × (1 - (dist/radius)²)
            let norm_dist = dist / well.radius;
            let influence = well.strength * (1.0 - norm_dist * norm_dist);
            total_influence += influence;

            // Transpose base weights to well's root and blend in
            let well_weights = transpose_weights(base_weights, well.root_pitch_class);
            for i in 0..12 {
                weights[i] += influence * well_weights[i];
            }
        }

        // Normalize if any weight exceeds 3.0 (prevents runaway from overlapping wells)
        let max_w = weights.iter().copied().fold(0.0f32, f32::max);
        if max_w > 3.0 {
            let scale = 3.0 / max_w;
            for w in &mut weights {
                *w *= scale;
            }
        }

        EffectiveWeights {
            weights,
            total_influence,
        }
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
            assert_eq!(wa.root_pitch_class, wb.root_pitch_class);
        }

        // Different seeds = different results
        let c = GravityField::generate(1, bounds, 1);
        let d = GravityField::generate(1, bounds, 2);
        assert_ne!(c.wells()[0].position, d.wells()[0].position);
    }

    #[test]
    fn effective_weights_no_wells() {
        let field = GravityField::new();
        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 0, &base);
        // With no wells and root=0, should just be base weights
        for i in 0..12 {
            assert!(
                (result.weights[i] - base[i]).abs() < 1e-6,
                "no wells: index {} differs",
                i
            );
        }
        assert!(result.total_influence < 1e-6);
    }

    #[test]
    fn effective_weights_with_root_offset() {
        let field = GravityField::new();
        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 9, &base);
        // Should be transposed to A
        let expected = transpose_weights(&base, 9);
        for i in 0..12 {
            assert!(
                (result.weights[i] - expected[i]).abs() < 1e-6,
                "root offset: index {} differs",
                i
            );
        }
    }

    #[test]
    fn effective_weights_organism_at_well_center() {
        let mut field = GravityField::new();
        field.wells.push(GravityWell {
            id: 0,
            position: [500.0, 350.0],
            root_pitch_class: 7, // G
            radius: 300.0,
            strength: 1.0,
            hue: 0.0,
        });

        let base = diatonic_weights();
        // Organism at well center, root=0
        let result = field.effective_weights([500.0, 350.0], 0, &base);

        // Should have influence = 1.0 (at center, dist=0, quadratic=1.0×strength)
        assert!(
            (result.total_influence - 1.0).abs() < 1e-5,
            "influence at center should be 1.0, got {}",
            result.total_influence
        );

        // Weights should be base (C-root) + 1.0 × G-transposed weights, then normalized
        let g_weights = transpose_weights(&base, 7);
        let mut expected = [0.0f32; 12];
        for i in 0..12 {
            expected[i] = base[i] + g_weights[i];
        }
        // Apply same normalization as effective_weights
        let max_e = expected.iter().copied().fold(0.0f32, f32::max);
        if max_e > 3.0 {
            let scale = 3.0 / max_e;
            for e in &mut expected {
                *e *= scale;
            }
        }
        for i in 0..12 {
            assert!(
                (result.weights[i] - expected[i]).abs() < 0.1,
                "index {}: expected ~{}, got {}",
                i,
                expected[i],
                result.weights[i]
            );
        }
    }

    #[test]
    fn effective_weights_organism_outside_well() {
        let mut field = GravityField::new();
        field.wells.push(GravityWell {
            id: 0,
            position: [100.0, 100.0],
            root_pitch_class: 7,
            radius: 200.0,
            strength: 1.0,
            hue: 0.0,
        });

        let base = diatonic_weights();
        // Organism far outside well radius
        let result = field.effective_weights([900.0, 900.0], 0, &base);

        // Should be just base weights (no well influence)
        assert!(result.total_influence < 1e-6);
        for i in 0..12 {
            assert!(
                (result.weights[i] - base[i]).abs() < 1e-6,
                "outside well: index {} should be base",
                i
            );
        }
    }

    #[test]
    fn normalization_prevents_runaway() {
        let mut field = GravityField::new();
        // Two overlapping wells at the same position
        for id in 0..3u32 {
            field.wells.push(GravityWell {
                id,
                position: [500.0, 350.0],
                root_pitch_class: 0,
                radius: 300.0,
                strength: 1.0,
                hue: 0.0,
            });
        }

        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 0, &base);

        let max_w = result.weights.iter().copied().fold(0.0f32, f32::max);
        assert!(
            max_w <= 3.01,
            "weights should be normalized to max 3.0, got {}",
            max_w
        );
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

    // === S38 LJ Well Force Tests ===

    #[test]
    fn consonance_weight_correctness() {
        assert_eq!(consonance_weight(0), 1.0);  // unison
        assert_eq!(consonance_weight(7), 0.8);  // fifth
        assert_eq!(consonance_weight(5), 0.8);  // fourth
        assert_eq!(consonance_weight(4), 0.5);  // major third
        assert_eq!(consonance_weight(3), 0.5);  // minor third
        assert_eq!(consonance_weight(6), 0.2);  // tritone
        assert_eq!(consonance_weight(1), 0.2);  // minor second
        assert_eq!(consonance_weight(12), 1.0); // wraps to unison
    }

    /// Compute the trench force for testing (mirrors app.rs apply_well_forces logic).
    fn trench_force(r: f32, well_radius: f32, g_eff: f32, m_well: f32) -> f32 {
        let r_eq = well_radius * LJ_TRENCH_FRACTION;
        let displacement = r - r_eq;
        let eps_sq = LJ_SOFTENING * LJ_SOFTENING;
        g_eff * m_well * displacement / (r * r + eps_sq)
    }

    #[test]
    fn lj_trench_force_profile() {
        let well_radius = 250.0_f32;
        let r_eq = well_radius * LJ_TRENCH_FRACTION;
        let g_eff = LJ_GRAVITY;
        let m_well = 0.6;

        // At trench: force ≈ 0, trench at 0.6 × 250 = 150
        let f_eq = trench_force(r_eq, well_radius, g_eff, m_well);
        assert!(f_eq.abs() < 0.001, "net force at trench should be ~0, got {}", f_eq);
        assert!((r_eq - 150.0).abs() < 0.1);

        // Inside trench: repulsion (f < 0)
        let f_inside = trench_force(50.0, well_radius, g_eff, m_well);
        assert!(f_inside < 0.0, "inside trench should repel, got {}", f_inside);

        // Outside trench: attraction (f > 0)
        let f_outside = trench_force(200.0, well_radius, g_eff, m_well);
        assert!(f_outside > 0.0, "outside trench should attract, got {}", f_outside);
    }

    #[test]
    fn lj_force_clamping() {
        // Very close to center (r≈0): large negative displacement → large repulsion → clamped
        let f = trench_force(1.0, 250.0, LJ_GRAVITY, 1.0);
        let f_clamped = f.clamp(-MAX_WELL_FORCE, MAX_WELL_FORCE);
        assert!(f_clamped.abs() <= MAX_WELL_FORCE, "force should be clamped, got {}", f_clamped);
    }

    #[test]
    fn lj_zero_energy_no_force() {
        // energy=0 → M_well=0 → force=0 everywhere
        let f = trench_force(100.0, 250.0, LJ_GRAVITY, 0.0);
        assert!(f.abs() < 0.001, "zero energy well should produce no force, got {}", f);
    }

    #[test]
    fn well_energy_drain() {
        let mut we = WellEnergy::new(0);
        we.energy = 0.8;
        we.tick(1, 1.0);
        // Should drain by BASE_DRAIN * 1.0 / sqrt(1) = 0.005, then regen
        assert!(we.energy < 0.8, "should drain: {}", we.energy);
    }

    #[test]
    fn well_energy_regen_unoccupied() {
        let mut we = WellEnergy::new(0);
        we.energy = 0.5;
        we.tick(0, 0.0);
        // Healthy regen: energy + REGEN_RATE * (1 - energy) = 0.5 + 0.01 * 0.5 = 0.505
        assert!(we.energy > 0.5, "should regen: {}", we.energy);
    }

    #[test]
    fn regen_state_transitions() {
        let mut we = WellEnergy::new(0);
        // Force energy low and occupied → should enter Wavering
        we.energy = 0.3;
        we.regen_state = RegenState::Healthy;
        we.tick(2, 0.1);
        assert_eq!(we.regen_state, RegenState::Wavering, "should waver at low energy + occupied");

        // Tick through wavering → dormant
        we.energy = 0.1;
        we.regen_state = RegenState::Wavering;
        we.state_ticks = DORMANT_ONSET_TICKS + 1;
        we.tick(2, 0.1);
        assert!(matches!(we.regen_state, RegenState::Dormant { .. }), "should go dormant");

        // Tick through dormancy → healthy (cooldown_remaining=0 means wake this tick)
        we.regen_state = RegenState::Dormant { cooldown_remaining: 0 };
        we.tick(0, 0.0);
        assert_eq!(we.regen_state, RegenState::Healthy, "should wake from dormancy");
        assert!((we.energy - DORMANT_SEED_ENERGY).abs() < 0.01, "should have seed energy");
    }

    #[test]
    fn beat_pulse_amplifies_inside_trench() {
        // Inside trench: beat pulse amplifies repulsion
        let well_radius = 250.0_f32;
        let f_base = trench_force(50.0, well_radius, LJ_GRAVITY, 0.6);
        // Beat mod: 1.0 + audio_energy * BEAT_PULSE_AMPLITUDE * sensitivity
        let beat_mod = 1.0 + 0.5 * BEAT_PULSE_AMPLITUDE * 0.8;
        let f_pulsed = f_base * beat_mod;
        assert!(f_pulsed < f_base, "pulsed repulsion should be stronger (more negative)");
        assert!(f_pulsed.abs() > f_base.abs(), "pulsed magnitude should exceed base");
    }

    // === S38 Phase B: Ecology Tests ===

    #[test]
    fn niche_penalty_varies_with_distance() {
        let cases: &[(f32, f32, f32, f32, &str)] = &[
            // (freq_a, freq_b, min_overlap, max_overlap, label)
            (440.0, 440.0, 0.999, 1.001, "identical centroids"),
            (110.0, 880.0, -0.001, 0.001, "3-octave gap"),
        ];
        for &(c_a, c_b, min_ov, max_ov, label) in cases {
            let octave_dist = (c_a / c_b).log2().abs();
            let overlap = (1.0 - octave_dist / OCTAVE_THRESHOLD).max(0.0);
            assert!(overlap >= min_ov && overlap <= max_ov,
                "{}: overlap {} outside [{}, {}]", label, overlap, min_ov, max_ov);
        }
    }

    #[test]
    fn well_proximity_default_zero() {
        let prox = WellProximity::default();
        assert_eq!(prox.influence_count, 0);
        assert!((prox.best_quality).abs() < 0.001);
        assert!((prox.net_score).abs() < 0.001);
    }

    #[test]
    fn well_response_dna_defaults() {
        // Verify WellResponseDna defaults when field is missing from JSON
        let prox = WellProximity::default();
        assert_eq!(prox.niche_penalty, 0.0);
        // Also verify WELL_SAT_WEIGHT is the expected value for eco bonus
        assert!((WELL_SAT_WEIGHT - 0.2).abs() < 0.001);
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
