/// Organism registry: owns all organisms, drives simulation, builds GPU payload.
///
/// Manages the lifecycle: spawn, despawn, tick, and GPU data extraction.
/// Integration (fusion) is triggered when IntegratePropose dwell timers exceed
/// threshold and both organisms consent.

use std::collections::HashMap;

use super::chladni;
use super::interaction::{self, AttachParams};
use super::sim::{OrganismId, OrganismState};
use super::sonar::Sonar;
use crate::organism::dna::InteractionMode;
use crate::renderer::biofield_renderer::CellData;
use crate::tuning::gravity_well;
use crate::tuning::harmony::histogram_consonance;

// ── Nutrient channel constants ──
/// Per-frame depletion rate for each nutrient channel (~42s from 1.0→0.5 at 120Hz).
const NUTRIENT_DECAY_RATE: f32 = 0.003;
/// Replenishment multiplier per unit of energy gained × host profile weight.
const NUTRIENT_REPLENISH_RATE: f32 = 0.15;
/// Below this level, the nutrient channel triggers hunger drive.
const NUTRIENT_DEFICIENCY_THRESHOLD: f32 = 0.3;
/// Max force multiplier bonus when visitor is deficient in what host provides.
const NUTRIENT_FORCE_BONUS: f32 = 0.6;
/// Seconds of sustained satiation + low arousal before wanderlust pulse.
const WANDERLUST_TRIGGER_SECS: f32 = 15.0;
/// Arousal floor during wanderlust pulse.
const WANDERLUST_AROUSAL_TARGET: f32 = 0.45;

/// Species → nutrient profile [ch0, ch1, ch2]. Rows sum to 1.0.
///
/// Note: organisms of the same species provide identical nutrient profiles.
/// This means monocultures (multiple same-species organisms) cannot sustain
/// each other — they create mutual deficiency on the same channels.
/// This is intentional: species diversity is required for ecosystem health.
fn species_nutrient_profile(species: &str) -> [f32; 3] {
    match species {
        "dron" => [0.7, 0.1, 0.2],
        "hoso" => [0.3, 0.2, 0.5],
        "spgl" => [0.5, 0.2, 0.3],
        "acid" => [0.1, 0.7, 0.2],
        "tblk" => [0.1, 0.5, 0.4],
        "kkit" => [0.0, 0.8, 0.2],
        "isao" => [0.3, 0.4, 0.3],
        _ => [0.33, 0.34, 0.33],
    }
}

/// Central owner of all organisms in the simulation.
pub struct OrganismRegistry {
    organisms: Vec<OrganismState>,
    next_id: OrganismId,

    // World boundary (soft wall)
    pub world_bounds: [f32; 4], // [min_x, min_y, max_x, max_y]
    pub boundary_force: f32,
    pub boundary_margin: f32,

    // Sonar — periodic neighbor detection infrastructure
    pub sonar: Sonar,

    // Pairwise affinities from the Hebbian graph, keyed (min_id, max_id)
    pairwise_affinities: HashMap<(OrganismId, OrganismId), f32>,

    // Continuous attachment strengths [0,1] computed from affinities via log curve
    pairwise_attachments: HashMap<(OrganismId, OrganismId), f32>,

}

impl OrganismRegistry {
    pub fn new() -> Self {
        Self {
            organisms: Vec::new(),
            next_id: 0,
            world_bounds: [0.0, 0.0, 1200.0, 700.0],
            boundary_force: 50.0,
            boundary_margin: 200.0,
            sonar: Sonar::new(),
            pairwise_affinities: HashMap::new(),
            pairwise_attachments: HashMap::new(),
        }
    }

    /// Spawn a new organism at the given position.
    /// Applies deterministic ±15% radius noise from spawn index hash.
    pub fn spawn(&mut self, position: [f32; 2], lobe_count: u8, core_radius: f32) -> OrganismId {
        let id = self.next_id;
        self.next_id += 1;
        let noisy_radius = core_radius * spawn_scale_factor(id);
        let org = OrganismState::new(id, position, lobe_count, noisy_radius);
        self.organisms.push(org);
        id
    }

    /// Remove an organism by ID.
    pub fn despawn(&mut self, id: OrganismId) {
        self.organisms.retain(|o| o.id != id);
        // Clean up integrate timers referencing this organism
        for org in &mut self.organisms {
            org.integrate_timers.remove(&id);
        }
        // Clean pairwise state referencing the despawned organism
        self.pairwise_affinities.retain(|&(a, b), _| a != id && b != id);
        self.pairwise_attachments.retain(|&(a, b), _| a != id && b != id);
    }

    /// Get a reference to an organism by ID.
    pub fn get(&self, id: OrganismId) -> Option<&OrganismState> {
        self.organisms.iter().find(|o| o.id == id)
    }

    /// Get a mutable reference to an organism by ID.
    pub fn get_mut(&mut self, id: OrganismId) -> Option<&mut OrganismState> {
        self.organisms.iter_mut().find(|o| o.id == id)
    }

    /// Number of active organisms.
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.organisms.len()
    }

    /// Read-only access to all organisms.
    pub fn organisms(&self) -> &[OrganismState] {
        &self.organisms
    }

    /// Mutable access to all organisms (for substrate feeding updates).
    pub fn organisms_mut(&mut self) -> &mut [OrganismState] {
        &mut self.organisms
    }

    /// Apply audio-to-kinetic impulses for all organisms (once per frame).
    pub fn apply_audio_impulses(&mut self) {
        for org in &mut self.organisms {
            org.apply_audio_impulse();
        }
    }

    /// Apply all forces: boundary, sonar/curiosity, DNA interactions (once per frame).
    pub fn tick_forces(&mut self, dt: f32) {
        // Apply world boundary forces
        self.apply_boundary_forces();

        // Sonar: periodic neighbor detection + curiosity attraction
        self.sonar.tick(dt, &self.organisms);
        // Build audio energy map for curiosity weighting
        let audio_energies: HashMap<OrganismId, f32> = self.organisms.iter()
            .map(|o| (o.id, o.audio_energy)).collect();
        let curiosity = self.sonar.curiosity_forces(&audio_energies);
        for (org_id, force) in &curiosity {
            if let Some(org) = self.organisms.iter_mut().find(|o| o.id == *org_id) {
                org.apply_force(*force);
            }
        }

        // Apply pairwise interaction forces from DNA rules
        self.apply_interactions(dt);
    }

    /// Fixed-timestep physics integration + boundary clamp (per substep at PHYS_DT).
    pub fn tick_physics(&mut self, dt: f32) {
        // Tick each organism's physics
        for org in &mut self.organisms {
            org.tick_physics(dt, self.world_bounds);
        }

        // Hard boundary clamp + velocity reflection — gentle bounce at walls
        const RESTITUTION: f32 = 0.3;
        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        for org in &mut self.organisms {
            let clamp_margin = org.core_radius * 2.0;
            // Reflect outward velocity instead of zeroing it
            if org.position[0] <= min_x + clamp_margin {
                org.position[0] = min_x + clamp_margin;
                if org.velocity[0] < 0.0 {
                    org.velocity[0] *= -RESTITUTION;
                }
            }
            if org.position[0] >= max_x - clamp_margin {
                org.position[0] = max_x - clamp_margin;
                if org.velocity[0] > 0.0 {
                    org.velocity[0] *= -RESTITUTION;
                }
            }
            if org.position[1] <= min_y + clamp_margin {
                org.position[1] = min_y + clamp_margin;
                if org.velocity[1] < 0.0 {
                    org.velocity[1] *= -RESTITUTION;
                }
            }
            if org.position[1] >= max_y - clamp_margin {
                org.position[1] = max_y - clamp_margin;
                if org.velocity[1] > 0.0 {
                    org.velocity[1] *= -RESTITUTION;
                }
            }
        }
    }

    /// Per-frame non-physics updates: affinities, attachments, proximity, visual smoothing.
    pub fn tick_frame(&mut self, dt: f32) {
        // Compute emergent pairwise affinities from proximity + audio + desire
        self.compute_emergent_affinities(dt);

        // Compute continuous attachment strengths from affinities
        self.compute_attachments();

        // Compute per-organism proximity_energy from sonar detections
        self.compute_proximity_energy();

        // Visual updates for each organism
        for org in &mut self.organisms {
            org.tick_visual(dt);
        }

        // Nutrient decay + wanderlust pulse
        for org in &mut self.organisms {
            // Only deplete nutrients when other organisms are detectable.
            // Prevents cold death for solo organisms.
            let has_neighbors = org.proximity_energy > 0.01;
            if has_neighbors {
                for ch in 0..3 {
                    org.nutrient_levels[ch] = (org.nutrient_levels[ch] - NUTRIENT_DECAY_RATE * dt).max(0.0);
                }
            }

            // Wanderlust: detect sustained satiation + low arousal (equilibrium)
            // Note: org.arousal is bridge-lerped at ~3Hz from emotion state (0.3s lag).
            // 1-frame ordering lag (8ms) is negligible against this smoothing window.
            let all_fed = org.nutrient_levels.iter().all(|&n| n > 0.4);
            if all_fed && org.arousal < 0.25 {
                org.monotony_timer += dt;
            } else {
                org.monotony_timer = (org.monotony_timer - dt * 0.5).max(0.0);
            }

            // Fire wanderlust pulse: arousal burst + accelerate nutrient depletion
            if org.monotony_timer >= WANDERLUST_TRIGGER_SECS {
                org.arousal = org.arousal.max(WANDERLUST_AROUSAL_TARGET);
                for ch in 0..3 {
                    org.nutrient_levels[ch] *= 0.5;
                }
                org.monotony_timer = 0.0;
            }
        }
    }

    /// Dual-root discovery: hybrid organisms drift their root_blend toward the
    /// more consonant root, committing after 5s of stability.
    ///
    /// `well_pitch_classes` is a slice of `(pitch_class, position)` for all active wells.
    /// Dual-root commit tick (no-op: wells no longer have harmonic identity
    /// in the substrate paradigm — hybrid organisms commit immediately to primary root).
    pub fn tick_dual_root(&mut self, _dt: f32, _well_positions: &[(u8, [f32; 2])]) {
        for org in &mut self.organisms {
            if let Some(_alt) = org.alt_root_pitch_class {
                // Immediately commit to primary root (no well-based consonance to compare)
                org.alt_root_pitch_class = None;
                org.root_blend = 0.0;
                org.root_blend_commit_timer = 0.0;
            }
        }
    }

    /// Advance all organisms by `dt` seconds (compat wrapper).
    ///
    /// Calls tick_frame + apply_audio_impulses + tick_forces + tick_physics in sequence.
    /// Used by tests and legacy call sites.
    #[allow(dead_code)]
    pub fn tick(&mut self, dt: f32) {
        self.tick_frame(dt);
        self.apply_audio_impulses();
        self.tick_forces(dt);
        self.tick_physics(dt);
    }

    /// Apply pairwise interaction forces based on each organism's DNA rules,
    /// modulated by Hebbian affinity and desire_to_connect.
    ///
    /// O(n²) pairwise evaluation — fine for ≤12 organisms. Snapshots state
    /// immutably first, computes forces, then applies them in a second pass
    /// to satisfy the borrow checker.
    fn apply_interactions(&mut self, dt: f32) {
        let n = self.organisms.len();
        if n < 2 {
            return;
        }

        // Snapshot immutable state for force computation
        struct OrgSnap {
            state: OrganismState,
        }
        let snaps: Vec<OrgSnap> = self
            .organisms
            .iter()
            .map(|o| OrgSnap {
                state: o.clone(),
            })
            .collect();

        // Accumulate forces and energy deltas per organism index
        let mut forces: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];
        let mut energy_deltas: Vec<f32> = vec![0.0; n];
        // Nutrient replenishment deltas: (organism_idx, host_species_profile, energy_gained)
        let mut nutrient_deltas: Vec<(usize, [f32; 3], f32)> = Vec::new();
        // Pairs that qualify for union state dwell timer ticking
        let mut dwell_pairs: Vec<(usize, usize, f32)> = Vec::new();

        // Pre-compute nutrient profiles for all organisms
        let profiles: Vec<[f32; 3]> = snaps.iter()
            .map(|s| species_nutrient_profile(&s.state.species))
            .collect();

        for i in 0..n {
            for j in (i + 1)..n {
                let a = &snaps[i].state;
                let b = &snaps[j].state;

                // Look up Hebbian pairwise affinity
                let affinity = self.affinity_between(a.id, b.id);

                // Check a's rules against b
                for rule in &a.interaction_rules {
                    if !species_matches(&rule.with_species, &b.species) {
                        continue;
                    }
                    let f = dispatch_interaction(a, b, rule, affinity, dt);
                    forces[i][0] += f.force_a[0];
                    forces[i][1] += f.force_a[1];
                    forces[j][0] += f.force_b[0];
                    forces[j][1] += f.force_b[1];
                }

                // Check b's rules against a (asymmetric rules possible)
                for rule in &b.interaction_rules {
                    if !species_matches(&rule.with_species, &a.species) {
                        continue;
                    }
                    // Flip a/b so the rule owner is "a"
                    let f = dispatch_interaction(b, a, rule, affinity, dt);
                    forces[j][0] += f.force_a[0];
                    forces[j][1] += f.force_a[1];
                    forces[i][0] += f.force_b[0];
                    forces[i][1] += f.force_b[1];
                }

                // Continuous pull: attachment-driven attraction with damping
                let attachment = self.attachment_between(a.id, b.id);
                if attachment > 0.01 {
                    let desire_avg = (a.desire_to_connect + b.desire_to_connect) * 0.5;
                    let f = interaction::continuous_pull(
                        a, b, attachment, 400.0, 15.0, desire_avg,
                    );
                    forces[i][0] += f.force_a[0];
                    forces[i][1] += f.force_a[1];
                    forces[j][0] += f.force_b[0];
                    forces[j][1] += f.force_b[1];
                }

                // Organism field: tangential Chladni forces from Hebbian affinity (S38)
                let f = interaction::organism_field(a, b, affinity);
                forces[i][0] += f.force_a[0];
                forces[i][1] += f.force_a[1];
                forces[j][0] += f.force_b[0];
                forces[j][1] += f.force_b[1];

                // Node well forces: A's nodes attract B, B's nodes attract A.
                // Nutrient deficiency modulates attraction: hungry visitors
                // are pulled more strongly toward hosts that provide what they lack.
                if !a.node_wells.is_empty() {
                    let result = interaction::node_well_force(
                        &a.node_wells, b.position,
                        a.node_drain_rate, b.node_absorption_rate,
                    );
                    // B's nutrient deficiency toward A's profile → force bonus
                    let deficiency: f32 = (0..3).map(|ch| {
                        let deficit = (NUTRIENT_DEFICIENCY_THRESHOLD - b.nutrient_levels[ch]).max(0.0);
                        deficit * profiles[i][ch]
                    }).sum();
                    let need_mul = 1.0 + deficiency * NUTRIENT_FORCE_BONUS;
                    forces[j][0] += result.force[0] * need_mul;
                    forces[j][1] += result.force[1] * need_mul;
                    energy_deltas[i] -= result.host_drain;
                    energy_deltas[j] += result.visitor_gain;
                    if result.visitor_gain > 0.001 {
                        nutrient_deltas.push((j, profiles[i], result.visitor_gain));
                    }
                }
                if !b.node_wells.is_empty() {
                    let result = interaction::node_well_force(
                        &b.node_wells, a.position,
                        b.node_drain_rate, a.node_absorption_rate,
                    );
                    // A's nutrient deficiency toward B's profile → force bonus
                    let deficiency: f32 = (0..3).map(|ch| {
                        let deficit = (NUTRIENT_DEFICIENCY_THRESHOLD - a.nutrient_levels[ch]).max(0.0);
                        deficit * profiles[j][ch]
                    }).sum();
                    let need_mul = 1.0 + deficiency * NUTRIENT_FORCE_BONUS;
                    forces[i][0] += result.force[0] * need_mul;
                    forces[i][1] += result.force[1] * need_mul;
                    energy_deltas[j] -= result.host_drain;
                    energy_deltas[i] += result.visitor_gain;
                    if result.visitor_gain > 0.001 {
                        nutrient_deltas.push((i, profiles[j], result.visitor_gain));
                    }
                }

                // Union state dwell: high affinity + mutual consent
                if affinity > 0.5
                    && a.consents_to_integrate()
                    && b.consents_to_integrate()
                {
                    let dist = interaction::dist_between_pub(a, b);
                    dwell_pairs.push((i, j, dist));
                }
            }
        }

        // Apply accumulated forces and energy deltas
        for (idx, force) in forces.iter().enumerate() {
            self.organisms[idx].apply_force(*force);
            self.organisms[idx].node_energy_balance = energy_deltas[idx];
        }

        // Apply nutrient replenishment from feeding
        for (idx, host_profile, gain) in &nutrient_deltas {
            let org = &mut self.organisms[*idx];
            for ch in 0..3 {
                org.nutrient_levels[ch] =
                    (org.nutrient_levels[ch] + gain * host_profile[ch] * NUTRIENT_REPLENISH_RATE)
                        .min(1.0);
            }
        }

        // Update node well positions and tick their energy state machines
        for org in &mut self.organisms {
            if org.node_wells.is_empty() {
                continue;
            }
            let id_f = chladni::cell_id_f(org.id as usize);
            let speed = (org.velocity[0] * org.velocity[0]
                + org.velocity[1] * org.velocity[1]).sqrt();
            let node_count = org.node_wells.len();
            let (positions, count) = chladni::compute_all_node_positions(
                org.position, org.visual_radius(), id_f,
                org.chladni_m, org.chladni_n, org.chladni_phase,
                org.harmonic_amp, org.audio_energy, org.heading, speed,
                node_count,
            );
            for i in 0..count {
                org.node_wells[i].position = positions[i];
                // Visitor count approximation: use proximity of other organisms
                // (actual per-node visitor counting would require O(N*M) lookups;
                // we use energy_balance sign as proxy: negative = being drained)
                let visitors = if org.node_energy_balance < -0.001 { 1 } else { 0 };
                org.node_wells[i].tick_energy(visitors, org.node_drain_rate, org.node_regen_rate);
            }
        }

        // Second pass: tick dwell timers for union state candidates
        for (i, j, dist) in dwell_pairs {
            let a_id = self.organisms[i].id;
            let b_id = self.organisms[j].id;
            interaction::integrate_propose_tick(&mut self.organisms[i], b_id, dist, 500.0, dt);
            interaction::integrate_propose_tick(&mut self.organisms[j], a_id, dist, 500.0, dt);
        }
    }

    /// Apply soft wall constraint forces near world boundary.
    /// Uses quadratic penetration for harder deceleration near walls.
    fn apply_boundary_forces(&mut self) {
        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        let margin = self.boundary_margin;
        let force = self.boundary_force;

        for org in &mut self.organisms {
            // Left wall
            if org.position[0] < min_x + margin {
                let penetration = (min_x + margin - org.position[0]) / margin;
                org.velocity[0] += force * penetration * penetration;
            }
            // Right wall
            if org.position[0] > max_x - margin {
                let penetration = (org.position[0] - (max_x - margin)) / margin;
                org.velocity[0] -= force * penetration * penetration;
            }
            // Top wall
            if org.position[1] < min_y + margin {
                let penetration = (min_y + margin - org.position[1]) / margin;
                org.velocity[1] += force * penetration * penetration;
            }
            // Bottom wall
            if org.position[1] > max_y - margin {
                let penetration = (org.position[1] - (max_y - margin)) / margin;
                org.velocity[1] -= force * penetration * penetration;
            }
        }
    }

    /// Compute per-organism proximity_energy from sonar detections.
    ///
    /// Each detection contributes a linear falloff factor (1 at distance=0,
    /// 0 at max_range). Accumulated per-organism and clamped to [0, 1].
    fn compute_proximity_energy(&mut self) {
        // Reset all proximity
        for org in &mut self.organisms {
            org.proximity_energy = 0.0;
        }

        let max_range = self.sonar.max_range;
        for det in self.sonar.detections() {
            let factor = (1.0 - det.distance / max_range).max(0.0);
            // Find organisms by ID and accumulate
            if let Some(a) = self.organisms.iter_mut().find(|o| o.id == det.org_a) {
                a.proximity_energy += factor;
            }
            if let Some(b) = self.organisms.iter_mut().find(|o| o.id == det.org_b) {
                b.proximity_energy += factor;
            }
        }

        // Clamp to [0, 1]
        for org in &mut self.organisms {
            org.proximity_energy = org.proximity_energy.min(1.0);
        }
    }

    /// Compute emergent pairwise affinities from proximity, audio energy, and desire.
    ///
    /// Merges with any externally-set graph affinities (takes max). This provides
    /// the connection signal even without cross-organism graph edges.
    ///
    /// Formula: additive blend — proximity, audio_corr, and desire are independent
    /// contributors. Two distant organisms playing together can still build affinity.
    fn compute_emergent_affinities(&mut self, dt: f32) {
        let n = self.organisms.len();
        if n < 2 {
            return;
        }

        // Affinity detection range: must exceed orbit distance (400px in DNA)
        // so orbiting organisms register as "near" each other.
        let affinity_range = 800.0_f32;
        // Smoothing rate: ~3 second convergence (rise and decay)
        let smooth_rate = 0.3 * dt;

        for i in 0..n {
            for j in (i + 1)..n {
                let a = &self.organisms[i];
                let b = &self.organisms[j];

                let dx = b.position[0] - a.position[0];
                let dy = b.position[1] - a.position[1];
                let dist = (dx * dx + dy * dy).sqrt();

                let proximity = (1.0 - dist / affinity_range).max(0.0);

                let key = if a.id < b.id { (a.id, b.id) } else { (b.id, a.id) };

                if proximity < 0.01 {
                    // Decay affinity when out of range
                    if let Some(v) = self.pairwise_affinities.get_mut(&key) {
                        *v = (*v - smooth_rate).max(0.0);
                    }
                    continue;
                }

                // Audio correlation: geometric mean of both organisms' energy
                let audio_corr = (a.audio_energy * b.audio_energy).sqrt();

                // Desire factor: average desire_to_connect
                let desire_avg = (a.desire_to_connect + b.desire_to_connect) * 0.5;

                // S40: Harmonic consonance — consumption histogram overlap
                let harmonic = histogram_consonance(&a.pitch_histogram, &b.pitch_histogram);

                // Additive blend: each factor contributes independently.
                // S40: harmonic term added (20% weight), others rebalanced.
                let target = (proximity * 0.30 + audio_corr * 0.25
                    + desire_avg * 0.25 + harmonic * 0.20).clamp(0.0, 1.0);

                // Exponential smoothing: affinity tracks target, rises and decays
                let existing = self.pairwise_affinities.get(&key).copied().unwrap_or(0.0);
                let smoothed = existing + (target - existing) * smooth_rate;
                self.pairwise_affinities.insert(key, smoothed.clamp(0.0, 1.0));
            }
        }
    }

    /// Compute continuous attachment strengths from stored pairwise affinities.
    ///
    /// Applies a logarithmic curve with a 0.15 threshold: slow approach,
    /// then rapid lock-in at high affinity.
    fn compute_attachments(&mut self) {
        self.pairwise_attachments.clear();
        for (&(a, b), &affinity) in &self.pairwise_affinities {
            let attachment = attachment_from_affinity(affinity);
            if attachment > 0.01 {
                self.pairwise_attachments.insert((a, b), attachment);
            }
        }
    }

    /// Look up pairwise attachment between two organisms. Returns 0.0 if none.
    pub fn attachment_between(&self, a: OrganismId, b: OrganismId) -> f32 {
        let key = if a < b { (a, b) } else { (b, a) };
        self.pairwise_attachments.get(&key).copied().unwrap_or(0.0)
    }

    /// Maximum attachment this organism has with any other organism.
    pub fn max_attachment_for(&self, id: OrganismId) -> f32 {
        let mut max = 0.0_f32;
        for (&(a, b), &att) in &self.pairwise_attachments {
            if a == id || b == id {
                max = max.max(att);
            }
        }
        max
    }

    /// Merge Hebbian graph affinities into existing emergent affinities.
    /// Takes the max of graph weight and emergent value for each pair,
    /// preserving the exponential smoothing accumulator from compute_emergent_affinities().
    pub fn update_affinities(&mut self, affinities: &[(OrganismId, OrganismId, f32)]) {
        for &(a, b, w) in affinities {
            let key = if a < b { (a, b) } else { (b, a) };
            let existing = self.pairwise_affinities.get(&key).copied().unwrap_or(0.0);
            self.pairwise_affinities.insert(key, existing.max(w));
        }
    }

    /// Look up pairwise affinity between two organisms. Returns 0.0 if no edge exists.
    pub fn affinity_between(&self, a: OrganismId, b: OrganismId) -> f32 {
        let key = if a < b { (a, b) } else { (b, a) };
        self.pairwise_affinities.get(&key).copied().unwrap_or(0.0)
    }

    /// Build GPU payload for the BioField renderer.
    ///
    /// One CellData per organism (single-node). Radius is scaled from sim-space
    /// to screen-space so organisms appear ~90-150px on screen.
    ///
    /// Cell ID: digits 0-9 map to the resistor palette. Digit 0 = Black, which is
    /// invisible against the dark background. We map spawn index → a 2-digit code
    /// where both digits are 1-9, cycling with tens=idx%9+1, units=idx/9%9+1.
    /// This yields 81 unique non-Black combinations — enough for 64 organisms.
    pub fn build_gpu_payload(&self) -> Vec<CellData> {
        const RADIUS_SCALE: f32 = 12.0;

        let mut cells = Vec::with_capacity(self.organisms.len());
        for (idx, org) in self.organisms.iter().enumerate() {
            let i = idx as u32;
            let cell_id = (i % 9 + 1) * 10 + (i / 9 % 9 + 1);
            let speed_swell = (org.smooth_speed / 120.0).min(1.0) * 0.08;
            let energy_swell = 1.0 + org.audio_energy * 0.3 + speed_swell;
            cells.push(CellData {
                pos:             org.position,
                radius:          org.core_radius * RADIUS_SCALE * energy_swell,
                audio_energy:    org.audio_energy,
                cell_id,
                hue:             org.base_hue,
                vel:             [org.visual_dir[0] * org.smooth_speed,
                                  org.visual_dir[1] * org.smooth_speed],
                harmonic_count:  org.harmonic_count,
                ring_phase:      org.ring_phase,
                shape_amplitude: org.shape_amplitude,
                shape_frequency: org.shape_frequency,
                harmonic_amp:    org.harmonic_amp,
                rd_fkr: {
                    let f = (org.rd_feed * 10000.0) as u32;
                    let k = (org.rd_kill * 10000.0) as u32;
                    let r = (org.rd_reactivity * 1000.0) as u32;
                    (f << 20) | (k << 10) | r
                },
                elongation:      org.chladni_m + org.chladni_n * 0.1,
                rd_scale:        org.rd_scale,
                chladni_phase:   org.chladni_phase,
                _pad1:           0.0,
                _pad2:           0.0,
                _pad3:           0.0,
            });
        }
        cells
    }

    /// Check and execute integration (fusion) events.
    ///
    /// When two organisms have been within range for dwell_threshold seconds
    /// and both consent, merge them into a new organism.
    #[allow(dead_code)]
    pub fn check_integrations(&mut self, dwell_threshold: f32) -> Vec<(OrganismId, OrganismId, OrganismId)> {
        let mut fusions = Vec::new();

        // Collect pairs that are ready to fuse
        let ids: Vec<OrganismId> = self.organisms.iter().map(|o| o.id).collect();

        for &a_id in &ids {
            for &b_id in &ids {
                if a_id >= b_id {
                    continue;
                }

                let (a_consent, a_timer, b_consent) = {
                    let a = match self.get(a_id) {
                        Some(a) => a,
                        None => continue,
                    };
                    let b = match self.get(b_id) {
                        Some(b) => b,
                        None => continue,
                    };
                    let a_timer = a.integrate_timers.get(&b_id).copied().unwrap_or(0.0);
                    (a.consents_to_integrate(), a_timer, b.consents_to_integrate())
                };

                if a_consent && b_consent && a_timer >= dwell_threshold {
                    fusions.push((a_id, b_id));
                }
            }
        }

        let mut results = Vec::new();
        for (a_id, b_id) in fusions {
            if let Some(new_id) = self.execute_fusion(a_id, b_id) {
                results.push((a_id, b_id, new_id));
            }
        }

        results
    }

    /// Execute a fusion: merge two organisms into a new one.
    ///
    /// Energy-weighted averaging for continuous params, union for collections,
    /// dual-root discovery for tonal identity, refractory period on desire.
    #[allow(dead_code)]
    fn execute_fusion(&mut self, a_id: OrganismId, b_id: OrganismId) -> Option<OrganismId> {
        let a = self.get(a_id)?.clone();
        let b = self.get(b_id)?.clone();

        // Compute centroid
        let pos = [
            (a.position[0] + b.position[0]) * 0.5,
            (a.position[1] + b.position[1]) * 0.5,
        ];

        // Area-conserving radius
        let new_radius = (a.core_radius * a.core_radius + b.core_radius * b.core_radius).sqrt();
        let new_lobe_count = a.lobe_count.max(b.lobe_count);

        // Spawn new organism
        let new_id = self.spawn(pos, new_lobe_count, new_radius);

        // Energy weights for blending
        let total_energy = a.energy + b.energy;
        let wa = if total_energy > 0.001 {
            a.energy / total_energy
        } else {
            0.5
        };
        let wb = 1.0 - wa;

        if let Some(new_org) = self.get_mut(new_id) {
            // --- Visual / physics (existing) ---
            new_org.energy = (a.energy + b.energy).min(1.0);
            new_org.velocity = [
                a.velocity[0] * wa + b.velocity[0] * wb,
                a.velocity[1] * wa + b.velocity[1] * wb,
            ];
            new_org.smin_k = a.smin_k * wa + b.smin_k * wb;
            new_org.edge_softness = a.edge_softness * wa + b.edge_softness * wb;
            new_org.base_hue = a.base_hue * wa + b.base_hue * wb;
            new_org.base_glow = a.base_glow * wa + b.base_glow * wb;
            new_org.pulse_response = a.pulse_response * wa + b.pulse_response * wb;
            new_org.drag = a.drag * wa + b.drag * wb;
            new_org.max_speed = a.max_speed * wa + b.max_speed * wb;
            new_org.mass = a.mass + b.mass;
            new_org.pseudopod_gain = a.pseudopod_gain * wa + b.pseudopod_gain * wb;
            new_org.consent_flags = a.consent_flags | b.consent_flags;

            // --- Emotion ---
            new_org.arousal = a.arousal * wa + b.arousal * wb;
            new_org.valence = a.valence * wa + b.valence * wb;
            new_org.desire_to_connect = 0.1; // refractory period

            // --- Nutrients ---
            new_org.nutrient_levels = [
                a.nutrient_levels[0] * wa + b.nutrient_levels[0] * wb,
                a.nutrient_levels[1] * wa + b.nutrient_levels[1] * wb,
                a.nutrient_levels[2] * wa + b.nutrient_levels[2] * wb,
            ];

            // --- Ecology ---
            new_org.node_drain_rate = a.node_drain_rate * wa + b.node_drain_rate * wb;
            new_org.node_absorption_rate = a.node_absorption_rate * wa + b.node_absorption_rate * wb;
            new_org.node_regen_rate = a.node_regen_rate * wa + b.node_regen_rate * wb;

            // --- Node wells: union of both parents' wells ---
            new_org.node_wells = merge_node_wells(&a.node_wells, &b.node_wells);

            // --- Tonal identity: dual-root discovery ---
            new_org.root_pitch_class = a.root_pitch_class;
            new_org.alt_root_pitch_class = Some(b.root_pitch_class);
            new_org.root_blend = 0.5;
            new_org.root_blend_commit_timer = 0.0;

            // --- Musical identity ---
            new_org.scale_affinity = a.scale_affinity * wa + b.scale_affinity * wb;
            new_org.current_seq_pitch_hz = a.current_seq_pitch_hz;

            // --- Interaction rules: union of both parents ---
            let mut merged_rules = a.interaction_rules.clone();
            for rule in &b.interaction_rules {
                // Deduplicate by (with_species, mode discriminant)
                let dominated = merged_rules.iter().any(|r| {
                    r.with_species == rule.with_species
                        && std::mem::discriminant(&r.mode) == std::mem::discriminant(&rule.mode)
                });
                if !dominated {
                    merged_rules.push(rule.clone());
                }
            }
            new_org.interaction_rules = merged_rules;

            // --- Species code ---
            new_org.species = gene_code_from_parents(&a.species, &b.species);

            // --- Lineage ---
            new_org.parent_codes = Some((a.species.clone(), b.species.clone()));

            // --- Body shape: energy-weighted ---
            new_org.shape_amplitude = a.shape_amplitude * wa + b.shape_amplitude * wb;
            new_org.shape_frequency = a.shape_frequency * wa + b.shape_frequency * wb;
            new_org.harmonic_count = a.harmonic_count * wa + b.harmonic_count * wb;
            new_org.harmonic_amp = a.harmonic_amp * wa + b.harmonic_amp * wb;
            new_org.elongation = a.elongation * wa + b.elongation * wb;
            new_org.chladni_m = a.chladni_m * wa + b.chladni_m * wb;
            new_org.chladni_n = a.chladni_n * wa + b.chladni_n * wb;

            // --- RD trail ---
            new_org.rd_reactivity = a.rd_reactivity * wa + b.rd_reactivity * wb;
            new_org.rd_feed = a.rd_feed * wa + b.rd_feed * wb;
            new_org.rd_kill = a.rd_kill * wa + b.rd_kill * wb;
            new_org.rd_scale = a.rd_scale * wa + b.rd_scale * wb;

            // --- Audio params ---
            new_org.reverb_send_base = a.reverb_send_base * wa + b.reverb_send_base * wb;
            new_org.tape_delay_send_base = a.tape_delay_send_base * wa + b.tape_delay_send_base * wb;
            new_org.viscosity = a.viscosity * wa + b.viscosity * wb;
        }

        // Despawn parents
        self.despawn(a_id);
        self.despawn(b_id);

        Some(new_id)
    }
}

// ============================================================================
// Fusion helpers
// ============================================================================

/// Merge two parents' node wells: union, deduplicate by proximity, cap at 12.
fn merge_node_wells(a_wells: &[chladni::NodeWell], b_wells: &[chladni::NodeWell]) -> Vec<chladni::NodeWell> {
    let mut merged: Vec<chladni::NodeWell> = Vec::with_capacity(a_wells.len() + b_wells.len());
    merged.extend_from_slice(a_wells);
    merged.extend_from_slice(b_wells);

    // Deduplicate overlapping wells (within 20px): keep higher-energy one
    deduplicate_wells_by_proximity(&mut merged, 20.0);

    // Cap at 12 wells — prune lowest-energy
    merged.sort_by(|a, b| b.energy.partial_cmp(&a.energy).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(12);

    merged
}

/// Remove wells within `threshold` pixels of each other, keeping the higher-energy one.
fn deduplicate_wells_by_proximity(wells: &mut Vec<chladni::NodeWell>, threshold: f32) {
    let threshold_sq = threshold * threshold;
    let mut keep = vec![true; wells.len()];

    for i in 0..wells.len() {
        if !keep[i] { continue; }
        for j in (i + 1)..wells.len() {
            if !keep[j] { continue; }
            let dx = wells[i].position[0] - wells[j].position[0];
            let dy = wells[i].position[1] - wells[j].position[1];
            if dx * dx + dy * dy < threshold_sq {
                // Keep the one with more energy
                if wells[j].energy > wells[i].energy {
                    keep[i] = false;
                    break; // i is dead, no need to check more
                } else {
                    keep[j] = false;
                }
            }
        }
    }

    let mut idx = 0;
    wells.retain(|_| { let k = keep[idx]; idx += 1; k });
}

/// Generate a 4-letter gene code from two parent species codes.
///
/// Takes first 2 chars of each parent (alphabetical order).
/// E.g. ACID + DRON → ACDR, DRON + HOSO → DRHO.
fn gene_code_from_parents(a_species: &str, b_species: &str) -> String {
    let (first, second) = if a_species <= b_species {
        (a_species, b_species)
    } else {
        (b_species, a_species)
    };

    let a_prefix: String = first.chars().take(2).collect();
    let b_prefix: String = second.chars().take(2).collect();
    format!("{}{}", a_prefix, b_prefix).to_uppercase()
}

// ============================================================================
// Free helpers for interaction dispatch
// ============================================================================

/// Logarithmic attachment curve: affinity [0,1] → attachment [0,1].
///
/// Below `threshold` (0.15), attachment is zero. Above threshold, a log10
/// curve maps normalized affinity to attachment strength. This produces
/// slow approach then rapid lock-in — like two musicians finding a groove.
pub(crate) fn attachment_from_affinity(affinity: f32) -> f32 {
    let threshold = 0.15;
    if affinity < threshold {
        return 0.0;
    }
    let normalized = (affinity - threshold) / (1.0 - threshold);
    (1.0 + normalized * 9.0).log10().clamp(0.0, 1.0)
}

/// Deterministic scale noise: ±15% variance from spawn index hash.
/// Same organism ID always gets same scale — no RNG dependency.
fn spawn_scale_factor(id: OrganismId) -> f32 {
    let hash = (id as u32).wrapping_mul(0x9e3779b9);
    let noise = (hash & 0xFFFF) as f32 / 65535.0;
    0.85 + noise * 0.30 // range [0.85, 1.15]
}

/// Check if a rule's `with_species` tag matches a target species.
fn species_matches(rule_species: &str, target_species: &str) -> bool {
    rule_species == "*" || rule_species == target_species
}

/// Dispatch an interaction rule to the appropriate physics function.
///
/// `a` is the rule owner, `b` is the other organism.
/// `affinity` is the pairwise Hebbian affinity [0, 1] between the organisms.
fn dispatch_interaction(
    a: &OrganismState,
    b: &OrganismState,
    rule: &crate::organism::dna::InteractionRule,
    affinity: f32,
    _dt: f32,
) -> interaction::InteractionForce {
    // If rule has affinity_threshold and pair doesn't meet it, skip
    if let Some(threshold) = rule.affinity_threshold {
        if affinity < threshold {
            return interaction::InteractionForce::zero();
        }
    }

    let range = rule.range;
    let strength = rule.strength;

    // Mood modulation: average desire_to_connect of both organisms
    let desire_avg = (a.desire_to_connect + b.desire_to_connect) * 0.5;

    match rule.mode {
        InteractionMode::Repel => {
            // High affinity + high desire → repel fades to zero
            let repel_factor = (1.0 - affinity * desire_avg).max(0.0);
            interaction::repel(a, b, range, strength * repel_factor)
        }
        InteractionMode::Bounce => interaction::bounce(a, b, range, strength, 0.5),
        InteractionMode::Slow => {
            // Active organisms resist being slowed
            let resist = 1.0 - a.audio_energy * 0.4;
            interaction::slow(a, b, range, strength * resist.max(0.2))
        }
        InteractionMode::Attach => {
            let params = AttachParams {
                rest_length: rule.rest_length.unwrap_or(80.0),
                spring_k: strength,
                break_distance: rule.break_distance.unwrap_or(200.0),
                break_force: rule.break_force.unwrap_or(100.0),
            };
            let (force, _should_break) = interaction::attach(a, b, &params);
            force
        }
        InteractionMode::Glob => {
            // Glob rules now use continuous pull scaled by attachment
            let attachment = attachment_from_affinity(affinity);
            interaction::continuous_pull(a, b, attachment.max(0.1), range, strength, desire_avg)
        }
        InteractionMode::Orbit => {
            // Arousal tightens orbit, valence strengthens pull
            let mood_range = range * (1.0 - a.arousal * 0.15);
            let mood_strength = strength * (0.6 + (a.valence + 1.0) * 0.2);
            interaction::orbit(a, b, mood_range, mood_strength)
        }
        InteractionMode::IntegratePropose => {
            // IntegratePropose doesn't produce forces — handled via dwell timers
            interaction::InteractionForce::zero()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_creates_organism() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 6, 30.0);
        assert_eq!(reg.count(), 1);
        assert!(reg.get(id).is_some());
    }

    #[test]
    fn despawn_removes_organism() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 6, 30.0);
        reg.despawn(id);
        assert_eq!(reg.count(), 0);
        assert!(reg.get(id).is_none());
    }

    #[test]
    fn tick_advances_positions() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 6, 30.0);
        reg.get_mut(id).unwrap().velocity = [60.0, 0.0];
        reg.get_mut(id).unwrap().drag = 1.0;

        reg.tick(1.0 / 60.0);

        let org = reg.get(id).unwrap();
        assert!(org.position[0] > 100.0, "position should advance");
    }

    #[test]
    fn build_gpu_payload_produces_data() {
        let mut reg = OrganismRegistry::new();
        reg.spawn([100.0, 100.0], 6, 30.0);
        reg.spawn([300.0, 200.0], 4, 20.0);

        let cells = reg.build_gpu_payload();
        // Single-node: 1 CellData per organism
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].cell_id, 11);
        assert_eq!(cells[1].cell_id, 21);
    }

    #[test]
    fn fusion_merges_two_organisms() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 6, 30.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        // Set consent and dwell timer
        reg.get_mut(a_id).unwrap().consent_flags = 1;
        reg.get_mut(b_id).unwrap().consent_flags = 1;
        reg.get_mut(a_id)
            .unwrap()
            .integrate_timers
            .insert(b_id, 5.0);

        let fusions = reg.check_integrations(3.0);
        assert_eq!(fusions.len(), 1);

        // Parents should be gone, new organism should exist
        assert_eq!(reg.count(), 1);
        let (_, _, new_id) = fusions[0];
        let new_org = reg.get(new_id).unwrap();

        // Area-conserving radius with spawn noise applied to parents and child
        let r_a = 30.0_f32 * spawn_scale_factor(0);
        let r_b = 20.0_f32 * spawn_scale_factor(1);
        let fused_pre_noise = (r_a * r_a + r_b * r_b).sqrt();
        let expected_radius = fused_pre_noise * spawn_scale_factor(2);
        assert!(
            (new_org.core_radius - expected_radius).abs() < 0.1,
            "area-conserving radius: {} vs {}",
            new_org.core_radius,
            expected_radius
        );

        // Position should be centroid of parents
        assert!(
            (new_org.position[0] - 110.0).abs() < 0.1,
            "centroid x: {}",
            new_org.position[0]
        );
    }

    #[test]
    fn fusion_requires_consent() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 6, 30.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        // Only A consents
        reg.get_mut(a_id).unwrap().consent_flags = 1;
        reg.get_mut(a_id)
            .unwrap()
            .integrate_timers
            .insert(b_id, 5.0);

        let fusions = reg.check_integrations(3.0);
        assert_eq!(fusions.len(), 0, "fusion should not happen without mutual consent");
        assert_eq!(reg.count(), 2);
    }

    #[test]
    fn unique_ids_across_spawns() {
        let mut reg = OrganismRegistry::new();
        let id1 = reg.spawn([0.0, 0.0], 4, 20.0);
        let id2 = reg.spawn([0.0, 0.0], 4, 20.0);
        assert_ne!(id1, id2);
    }

    #[test]
    fn boundary_forces_push_inward() {
        let mut reg = OrganismRegistry::new();
        reg.world_bounds = [0.0, 0.0, 1000.0, 1000.0];
        reg.boundary_margin = 100.0;
        reg.boundary_force = 50.0;

        // Organism near left wall
        let id = reg.spawn([20.0, 500.0], 4, 20.0);
        reg.get_mut(id).unwrap().velocity = [0.0, 0.0];
        reg.get_mut(id).unwrap().drag = 1.0;

        reg.tick(1.0 / 60.0);

        let org = reg.get(id).unwrap();
        assert!(
            org.velocity[0] > 0.0,
            "boundary should push rightward: vx={}",
            org.velocity[0]
        );
    }

    #[test]
    fn repel_rule_pushes_organisms_apart() {
        use crate::organism::dna::{InteractionMode, InteractionRule};

        let mut reg = OrganismRegistry::new();
        reg.world_bounds = [0.0, 0.0, 2000.0, 2000.0];

        let a_id = reg.spawn([500.0, 500.0], 4, 20.0);
        let b_id = reg.spawn([510.0, 500.0], 4, 20.0);

        // Give A a repel rule against all species
        reg.get_mut(a_id).unwrap().species = "test".to_string();
        reg.get_mut(b_id).unwrap().species = "test".to_string();
        reg.get_mut(a_id).unwrap().interaction_rules = vec![InteractionRule {
            with_species: "*".to_string(),
            mode: InteractionMode::Repel,
            range: 50.0,
            strength: 10.0,
            dwell_secs: None,
            rest_length: None,
            break_force: None,
            break_distance: None,
            affinity_threshold: None,
        }];
        reg.get_mut(a_id).unwrap().drag = 1.0;
        reg.get_mut(b_id).unwrap().drag = 1.0;
        reg.get_mut(a_id).unwrap().velocity = [0.0, 0.0];
        reg.get_mut(b_id).unwrap().velocity = [0.0, 0.0];

        reg.tick(1.0 / 60.0);

        let a = reg.get(a_id).unwrap();
        let b = reg.get(b_id).unwrap();
        // A should be pushed left (away from B)
        assert!(a.velocity[0] < 0.0, "a should be pushed left: vx={}", a.velocity[0]);
        // B should be pushed right (away from A)
        assert!(b.velocity[0] > 0.0, "b should be pushed right: vx={}", b.velocity[0]);
    }

    #[test]
    fn attachment_computed_from_affinity() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);
        let c_id = reg.spawn([500.0, 500.0], 4, 20.0);

        // High affinity between A and B, low for C
        reg.update_affinities(&[
            (a_id, b_id, 0.8),
            (a_id, c_id, 0.1),  // below threshold
            (b_id, c_id, 0.2),
        ]);

        // Trigger attachment computation
        reg.compute_attachments();

        let ab = reg.attachment_between(a_id, b_id);
        let ac = reg.attachment_between(a_id, c_id);
        let bc = reg.attachment_between(b_id, c_id);

        assert!(ab > 0.5, "high affinity should produce strong attachment: {ab}");
        assert_eq!(ac, 0.0, "below-threshold affinity should produce zero attachment");
        assert!(bc > 0.0 && bc < ab, "moderate affinity should produce moderate attachment: {bc}");
    }

    #[test]
    fn attachment_from_affinity_curve_shape() {
        // Below threshold → 0
        assert_eq!(attachment_from_affinity(0.0), 0.0);
        assert_eq!(attachment_from_affinity(0.14), 0.0);

        // Just above threshold → small but nonzero
        let low = attachment_from_affinity(0.2);
        assert!(low > 0.0 && low < 0.3, "low affinity attachment: {low}");

        // Mid → moderate-to-strong (log curve is steep)
        let mid = attachment_from_affinity(0.5);
        assert!(mid > 0.5 && mid < 0.8, "mid affinity attachment: {mid}");

        // High → strong
        let high = attachment_from_affinity(0.9);
        assert!(high > 0.8, "high affinity attachment: {high}");

        // Max → 1.0
        assert!((attachment_from_affinity(1.0) - 1.0).abs() < 0.001);

        // Monotonically increasing
        assert!(attachment_from_affinity(0.3) < attachment_from_affinity(0.5));
        assert!(attachment_from_affinity(0.5) < attachment_from_affinity(0.8));
    }

    #[test]
    fn max_attachment_for_returns_strongest() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);
        let c_id = reg.spawn([300.0, 100.0], 4, 20.0);

        reg.update_affinities(&[
            (a_id, b_id, 0.5),
            (a_id, c_id, 0.8),
        ]);
        reg.compute_attachments();

        let max_a = reg.max_attachment_for(a_id);
        let att_ac = reg.attachment_between(a_id, c_id);
        assert!((max_a - att_ac).abs() < 0.001, "max should be the stronger pair: {max_a}");
    }

    #[test]
    fn build_gpu_payload_assigns_sequential_ids() {
        let mut reg = OrganismRegistry::new();
        reg.spawn([100.0, 100.0], 4, 20.0);
        reg.spawn([200.0, 100.0], 4, 20.0);

        let cells = reg.build_gpu_payload();
        // Single-node: 1 per organism
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].cell_id, 11);
        assert_eq!(cells[1].cell_id, 21);
        // Radius = noisy_core_radius * RADIUS_SCALE * energy_swell
        // core_radius=20, noise_factor(id=0), scale=12.0, energy=0
        let expected_radius = 20.0 * spawn_scale_factor(0) * 12.0;
        assert!((cells[0].radius - expected_radius).abs() < 0.001);
    }

    #[test]
    fn species_wildcard_matches() {
        assert!(super::species_matches("*", "anything"));
        assert!(super::species_matches("dron", "dron"));
        assert!(!super::species_matches("dron", "melo"));
    }

    #[test]
    fn high_affinity_weakens_repel() {
        use crate::organism::dna::{InteractionMode, InteractionRule};

        let mut reg = OrganismRegistry::new();
        reg.world_bounds = [0.0, 0.0, 2000.0, 2000.0];

        let a_id = reg.spawn([500.0, 500.0], 4, 20.0);
        let b_id = reg.spawn([510.0, 500.0], 4, 20.0);

        reg.get_mut(a_id).unwrap().species = "test".to_string();
        reg.get_mut(b_id).unwrap().species = "test".to_string();
        reg.get_mut(a_id).unwrap().interaction_rules = vec![InteractionRule {
            with_species: "*".to_string(),
            mode: InteractionMode::Repel,
            range: 50.0,
            strength: 10.0,
            dwell_secs: None,
            rest_length: None,
            break_force: None,
            break_distance: None,
            affinity_threshold: None,
        }];
        reg.get_mut(a_id).unwrap().drag = 1.0;
        reg.get_mut(b_id).unwrap().drag = 1.0;
        reg.get_mut(a_id).unwrap().velocity = [0.0, 0.0];
        reg.get_mut(b_id).unwrap().velocity = [0.0, 0.0];

        // High desire to connect
        reg.get_mut(a_id).unwrap().desire_to_connect = 0.9;
        reg.get_mut(b_id).unwrap().desire_to_connect = 0.9;

        // Record repel force WITHOUT affinity
        reg.tick(1.0 / 60.0);
        let repel_no_affinity = reg.get(a_id).unwrap().velocity[0];

        // Reset and try WITH high affinity
        let a_id2 = reg.spawn([500.0, 500.0], 4, 20.0);
        let b_id2 = reg.spawn([510.0, 500.0], 4, 20.0);
        reg.get_mut(a_id2).unwrap().species = "test".to_string();
        reg.get_mut(b_id2).unwrap().species = "test".to_string();
        reg.get_mut(a_id2).unwrap().interaction_rules = vec![InteractionRule {
            with_species: "*".to_string(),
            mode: InteractionMode::Repel,
            range: 50.0,
            strength: 10.0,
            dwell_secs: None,
            rest_length: None,
            break_force: None,
            break_distance: None,
            affinity_threshold: None,
        }];
        reg.get_mut(a_id2).unwrap().drag = 1.0;
        reg.get_mut(b_id2).unwrap().drag = 1.0;
        reg.get_mut(a_id2).unwrap().velocity = [0.0, 0.0];
        reg.get_mut(b_id2).unwrap().velocity = [0.0, 0.0];
        reg.get_mut(a_id2).unwrap().desire_to_connect = 0.9;
        reg.get_mut(b_id2).unwrap().desire_to_connect = 0.9;

        // Set high pairwise affinity
        reg.update_affinities(&[(a_id2, b_id2, 0.9)]);
        reg.tick(1.0 / 60.0);
        let repel_with_affinity = reg.get(a_id2).unwrap().velocity[0];

        // Without affinity: net force is repulsive (A pushed left, negative velocity)
        assert!(repel_no_affinity < 0.0, "no affinity → repulsion: {}", repel_no_affinity);
        // With high affinity + desire: attraction overcomes weakened repel → net attractive
        assert!(
            repel_with_affinity > repel_no_affinity,
            "high affinity should shift force toward attraction: with={}, without={}",
            repel_with_affinity,
            repel_no_affinity
        );
    }

    #[test]
    fn affinity_between_returns_stored_value() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);

        reg.update_affinities(&[(a_id, b_id, 0.75)]);
        assert!((reg.affinity_between(a_id, b_id) - 0.75).abs() < 0.001);
        // Reverse order should also work
        assert!((reg.affinity_between(b_id, a_id) - 0.75).abs() < 0.001);
        // Non-existent pair returns 0
        assert_eq!(reg.affinity_between(a_id, 999), 0.0);
    }

    // === S36: Fixed timestep accumulator tests ===

    #[test]
    fn accumulator_cap() {
        // Simulate a 200ms frame. With PHYS_DT=1/120, uncapped that's 24 substeps.
        // But capped at 100ms = 12 substeps max.
        const PHYS_DT: f32 = 1.0 / 120.0;
        const PHYS_MAX_ACCUM: f32 = 0.1;

        let mut accum: f32 = 0.0;
        accum += 0.2; // 200ms frame
        accum = accum.min(PHYS_MAX_ACCUM); // cap to 100ms

        let mut substeps = 0;
        while accum >= PHYS_DT {
            substeps += 1;
            accum -= PHYS_DT;
        }
        assert_eq!(substeps, 12, "should cap at 12 substeps, not 24");
    }

    #[test]
    fn accumulator_remainder_carries() {
        // Run two frames and verify leftover from frame 1 is used in frame 2.
        const PHYS_DT: f32 = 1.0 / 120.0;
        const PHYS_MAX_ACCUM: f32 = 0.1;

        let mut accum: f32 = 0.0;
        let frame_dt = 1.0 / 60.0; // ~16.67ms

        // Frame 1: 16.67ms → 2 substeps of 8.33ms each = 16.67ms consumed, ~0ms remainder
        accum += frame_dt;
        accum = accum.min(PHYS_MAX_ACCUM);
        let mut substeps_1 = 0;
        while accum >= PHYS_DT {
            substeps_1 += 1;
            accum -= PHYS_DT;
        }
        assert_eq!(substeps_1, 2);
        let remainder_1 = accum;
        assert!(remainder_1 >= 0.0 && remainder_1 < PHYS_DT);

        // Frame 2: another 16.67ms. With remainder, might get 2 substeps.
        accum += frame_dt;
        accum = accum.min(PHYS_MAX_ACCUM);
        let mut substeps_2 = 0;
        while accum >= PHYS_DT {
            substeps_2 += 1;
            accum -= PHYS_DT;
        }
        // Should be 2 (remainder is tiny for 60fps → 120Hz divides evenly)
        assert!(substeps_2 == 2, "frame 2 should run 2 substeps: got {substeps_2}");
    }

    #[test]
    fn split_methods_match_compat_wrapper() {
        // Verify that calling the split methods in sequence produces the same
        // result as the compat wrapper tick().
        let mut reg_split = OrganismRegistry::new();
        let mut reg_compat = OrganismRegistry::new();

        let id_s = reg_split.spawn([500.0, 500.0], 4, 20.0);
        let id_c = reg_compat.spawn([500.0, 500.0], 4, 20.0);

        // Set identical initial conditions
        for reg in [&mut reg_split, &mut reg_compat] {
            let id = reg.organisms()[0].id;
            let org = reg.get_mut(id).unwrap();
            org.velocity = [50.0, 20.0];
            org.drag = 0.95;
            org.arousal = 0.5;
            org.audio_energy = 0.3;
        }

        let dt = 1.0 / 60.0;

        // Compat wrapper
        reg_compat.tick(dt);

        // Split calls (same order as compat wrapper)
        reg_split.tick_frame(dt);
        reg_split.apply_audio_impulses();
        reg_split.tick_forces(dt);
        reg_split.tick_physics(dt);

        let org_s = reg_split.get(id_s).unwrap();
        let org_c = reg_compat.get(id_c).unwrap();

        assert!(
            (org_s.position[0] - org_c.position[0]).abs() < 0.001,
            "x mismatch: split={} compat={}",
            org_s.position[0], org_c.position[0]
        );
        assert!(
            (org_s.position[1] - org_c.position[1]).abs() < 0.001,
            "y mismatch: split={} compat={}",
            org_s.position[1], org_c.position[1]
        );
    }

    // ── Nutrient channel tests ──

    #[test]
    fn nutrient_depletion_over_time() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 3, 30.0);
        // Spawn a neighbor so proximity_energy > 0 (enables nutrient decay)
        let _neighbor = reg.spawn([300.0, 100.0], 3, 30.0);
        reg.get_mut(id).unwrap().nutrient_levels = [1.0, 1.0, 1.0];

        // Use tick() (not tick_frame) so sonar pings and detects the neighbor
        for _ in 0..600 {
            reg.tick(1.0 / 120.0);
        }

        let org = reg.get(id).unwrap();
        for ch in 0..3 {
            assert!(
                org.nutrient_levels[ch] < 1.0,
                "channel {} should deplete: {}",
                ch, org.nutrient_levels[ch]
            );
            assert!(
                org.nutrient_levels[ch] > 0.0,
                "channel {} should not be zero yet: {}",
                ch, org.nutrient_levels[ch]
            );
        }
    }

    #[test]
    fn species_nutrient_profiles_sum_to_one() {
        for species in &["dron", "hoso", "spgl", "acid", "tblk", "kkit", "isao"] {
            let p = species_nutrient_profile(species);
            let sum: f32 = p.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "{} profile sums to {}, expected 1.0",
                species, sum
            );
        }
    }

    #[test]
    fn deficiency_increases_force_toward_matching_host() {
        // An organism starved on ch0 should be more attracted to a DRON (ch0=0.7)
        // than to a KKIT (ch0=0.0)
        let dron_profile = species_nutrient_profile("dron");
        let kkit_profile = species_nutrient_profile("kkit");

        // Starved on ch0
        let nutrients = [0.0, 0.5, 0.5];

        let deficiency_toward_dron: f32 = (0..3).map(|ch| {
            let deficit = (NUTRIENT_DEFICIENCY_THRESHOLD - nutrients[ch]).max(0.0);
            deficit * dron_profile[ch]
        }).sum();

        let deficiency_toward_kkit: f32 = (0..3).map(|ch| {
            let deficit = (NUTRIENT_DEFICIENCY_THRESHOLD - nutrients[ch]).max(0.0);
            deficit * kkit_profile[ch]
        }).sum();

        let mul_dron = 1.0 + deficiency_toward_dron * NUTRIENT_FORCE_BONUS;
        let mul_kkit = 1.0 + deficiency_toward_kkit * NUTRIENT_FORCE_BONUS;

        assert!(
            mul_dron > mul_kkit,
            "DRON force should be stronger when starved on ch0: dron={} kkit={}",
            mul_dron, mul_kkit
        );
        assert!(
            mul_dron > 1.0,
            "DRON force bonus should be > 1.0: {}",
            mul_dron
        );
    }

    #[test]
    fn no_deficiency_no_force_bonus() {
        // Fully fed organism gets no bonus from any host
        let nutrients = [1.0, 1.0, 1.0];
        let profile = species_nutrient_profile("acid");

        let deficiency: f32 = (0..3).map(|ch| {
            let deficit = (NUTRIENT_DEFICIENCY_THRESHOLD - nutrients[ch]).max(0.0);
            deficit * profile[ch]
        }).sum();

        let mul = 1.0 + deficiency * NUTRIENT_FORCE_BONUS;
        assert!(
            (mul - 1.0).abs() < 0.001,
            "fully fed should have mul=1.0: {}",
            mul
        );
    }

    #[test]
    fn wanderlust_fires_when_satiated() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 3, 30.0);
        // Spawn a neighbor so sonar detects and proximity_energy > 0
        let _neighbor = reg.spawn([300.0, 100.0], 3, 30.0);
        {
            let org = reg.get_mut(id).unwrap();
            org.nutrient_levels = [0.8, 0.8, 0.8]; // well fed
            org.arousal = 0.15; // low arousal (equilibrium)
        }

        // Use tick() so sonar pings and enables nutrient decay
        let dt = 1.0 / 120.0;
        for _ in 0..2400 {
            reg.tick(dt);
        }

        let org = reg.get(id).unwrap();
        // After wanderlust pulse: nutrients should be halved, or depleted further
        let any_below_half = org.nutrient_levels.iter().any(|&n| n < 0.4);
        assert!(
            any_below_half,
            "wanderlust should deplete nutrients: {:?}",
            org.nutrient_levels
        );
    }

    #[test]
    fn diverse_feeding_prevents_wanderlust() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 3, 30.0);
        {
            let org = reg.get_mut(id).unwrap();
            org.nutrient_levels = [0.2, 0.2, 0.2]; // deficient → not "all fed"
            org.arousal = 0.4; // above 0.25 threshold
        }

        let dt = 1.0 / 120.0;
        for _ in 0..600 { // 5 seconds
            reg.tick_frame(dt);
        }

        let org = reg.get(id).unwrap();
        assert!(
            org.monotony_timer < 1.0,
            "monotony_timer should stay low when not fully fed: {}",
            org.monotony_timer
        );
    }

    #[test]
    fn solo_organism_nutrients_stable() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([100.0, 100.0], 3, 30.0);
        {
            let org = reg.get_mut(id).unwrap();
            org.nutrient_levels = [0.8, 0.8, 0.8];
            org.proximity_energy = 0.0; // no neighbors
        }

        let initial = {
            let org = reg.get(id).unwrap();
            org.nutrient_levels
        };

        // Tick for ~10 seconds at 120Hz
        let dt = 1.0 / 120.0;
        for _ in 0..1200 {
            reg.tick_frame(dt);
            // Keep proximity_energy at 0 (no neighbors detected by sonar)
            if let Some(org) = reg.get_mut(id) {
                org.proximity_energy = 0.0;
            }
        }

        let org = reg.get(id).unwrap();
        for ch in 0..3 {
            assert!(
                (org.nutrient_levels[ch] - initial[ch]).abs() < 0.01,
                "solo organism nutrient ch{} should be stable: {} vs {}",
                ch, org.nutrient_levels[ch], initial[ch]
            );
        }
    }

    // === Pre-union hardening tests ===

    #[test]
    fn despawn_cleans_pairwise_state() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);
        let c_id = reg.spawn([300.0, 100.0], 4, 20.0);

        // Build affinities between all pairs
        reg.update_affinities(&[
            (a_id, b_id, 0.8),
            (a_id, c_id, 0.6),
            (b_id, c_id, 0.5),
        ]);
        reg.compute_attachments();

        // Verify pairwise state exists
        assert!(reg.affinity_between(a_id, b_id) > 0.0);
        assert!(reg.attachment_between(a_id, c_id) > 0.0);

        // Despawn B
        reg.despawn(b_id);

        // All pairs involving B should be cleaned
        assert_eq!(reg.affinity_between(a_id, b_id), 0.0, "affinity A-B should be cleaned");
        assert_eq!(reg.affinity_between(b_id, c_id), 0.0, "affinity B-C should be cleaned");
        assert_eq!(reg.attachment_between(a_id, b_id), 0.0, "attachment A-B should be cleaned");
        assert_eq!(reg.attachment_between(b_id, c_id), 0.0, "attachment B-C should be cleaned");

        // A-C should still exist
        assert!(reg.affinity_between(a_id, c_id) > 0.0, "affinity A-C should survive");
    }

    #[test]
    fn fusion_merges_emotion_state() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        // Set known emotion state
        {
            let a = reg.get_mut(a_id).unwrap();
            a.arousal = 0.8;
            a.valence = 0.6;
            a.energy = 0.7;
            a.consent_flags = 1;
            a.integrate_timers.insert(b_id, 5.0);
        }
        {
            let b = reg.get_mut(b_id).unwrap();
            b.arousal = 0.4;
            b.valence = -0.2;
            b.energy = 0.3;
            b.consent_flags = 1;
        }

        let fusions = reg.check_integrations(3.0);
        assert_eq!(fusions.len(), 1);
        let (_, _, new_id) = fusions[0];
        let new_org = reg.get(new_id).unwrap();

        // Energy-weighted: wa = 0.7/1.0 = 0.7, wb = 0.3
        let wa = 0.7;
        let wb = 0.3;
        let expected_arousal = 0.8 * wa + 0.4 * wb;
        let expected_valence = 0.6 * wa + (-0.2) * wb;

        assert!(
            (new_org.arousal - expected_arousal).abs() < 0.01,
            "arousal: {} vs expected {}", new_org.arousal, expected_arousal
        );
        assert!(
            (new_org.valence - expected_valence).abs() < 0.01,
            "valence: {} vs expected {}", new_org.valence, expected_valence
        );
    }

    #[test]
    fn fusion_refractory_period() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        reg.get_mut(a_id).unwrap().consent_flags = 1;
        reg.get_mut(b_id).unwrap().consent_flags = 1;
        reg.get_mut(a_id).unwrap().integrate_timers.insert(b_id, 5.0);
        // Set high desire pre-fusion
        reg.get_mut(a_id).unwrap().desire_to_connect = 0.9;
        reg.get_mut(b_id).unwrap().desire_to_connect = 0.9;

        let fusions = reg.check_integrations(3.0);
        let (_, _, new_id) = fusions[0];
        let new_org = reg.get(new_id).unwrap();

        assert!(
            (new_org.desire_to_connect - 0.1).abs() < 0.01,
            "fused child should have refractory desire: {}", new_org.desire_to_connect
        );
    }

    #[test]
    fn node_well_merge_deduplicates() {
        use crate::organism::chladni::NodeWell;

        // Two wells at near-identical positions should be deduped
        let a_wells = vec![
            NodeWell { position: [100.0, 100.0], energy: 0.8, state: chladni::NodeState::Active },
            NodeWell { position: [200.0, 200.0], energy: 0.5, state: chladni::NodeState::Active },
        ];
        let b_wells = vec![
            NodeWell { position: [105.0, 105.0], energy: 0.9, state: chladni::NodeState::Active }, // overlaps a_wells[0]
            NodeWell { position: [300.0, 300.0], energy: 0.7, state: chladni::NodeState::Active },
        ];

        let merged = merge_node_wells(&a_wells, &b_wells);

        // Should have 3 wells (one overlap removed, higher-energy kept)
        assert_eq!(merged.len(), 3, "overlapping wells should be deduped: got {}", merged.len());
        // The 0.9 energy well should survive over the 0.8
        assert!(
            merged.iter().any(|w| (w.energy - 0.9).abs() < 0.01),
            "higher-energy overlapping well should survive"
        );
        assert!(
            !merged.iter().any(|w| (w.energy - 0.8).abs() < 0.01),
            "lower-energy overlapping well should be pruned"
        );
    }

    #[test]
    fn node_well_merge_caps_at_twelve() {
        use crate::organism::chladni::NodeWell;

        let make_wells = |count: usize, offset: f32| -> Vec<NodeWell> {
            (0..count).map(|i| NodeWell {
                position: [i as f32 * 50.0 + offset, 100.0],
                energy: 1.0 - i as f32 * 0.05,
                state: chladni::NodeState::Active,
            }).collect()
        };

        let a_wells = make_wells(8, 0.0);
        let b_wells = make_wells(8, 500.0); // no overlaps

        let merged = merge_node_wells(&a_wells, &b_wells);
        assert!(merged.len() <= 12, "merged wells should cap at 12: got {}", merged.len());
    }

    #[test]
    fn dual_root_initializes_on_fusion() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        // Set distinct roots
        reg.get_mut(a_id).unwrap().root_pitch_class = 0; // C
        reg.get_mut(b_id).unwrap().root_pitch_class = 7; // G
        reg.get_mut(a_id).unwrap().consent_flags = 1;
        reg.get_mut(b_id).unwrap().consent_flags = 1;
        reg.get_mut(a_id).unwrap().integrate_timers.insert(b_id, 5.0);

        let fusions = reg.check_integrations(3.0);
        let (_, _, new_id) = fusions[0];
        let new_org = reg.get(new_id).unwrap();

        assert_eq!(new_org.root_pitch_class, 0, "primary root should be parent A's root");
        assert_eq!(new_org.alt_root_pitch_class, Some(7), "alt root should be parent B's root");
        assert!((new_org.root_blend - 0.5).abs() < 0.01, "blend should start at 0.5");
    }

    #[test]
    fn dual_root_immediate_commit() {
        let mut reg = OrganismRegistry::new();
        let id = reg.spawn([500.0, 350.0], 4, 20.0);

        // Make it a hybrid: root=C(0), alt=G(7)
        {
            let org = reg.get_mut(id).unwrap();
            org.root_pitch_class = 0; // C
            org.alt_root_pitch_class = Some(7); // G
            org.root_blend = 0.5;
        }

        // In substrate paradigm, wells don't have harmonic identity.
        // Hybrids commit immediately to primary root.
        let wells = [(7_u8, [500.0, 350.0])];
        let dt = 1.0 / 60.0;
        reg.tick_dual_root(dt, &wells);

        let org = reg.get(id).unwrap();
        assert_eq!(org.root_pitch_class, 0, "should commit to primary root (C)");
        assert_eq!(org.alt_root_pitch_class, None, "alt should be cleared after commit");
    }

    #[test]
    fn gene_code_generation() {
        assert_eq!(gene_code_from_parents("ACID", "DRON"), "ACDR");
        assert_eq!(gene_code_from_parents("DRON", "HOSO"), "DRHO");
        assert_eq!(gene_code_from_parents("HOSO", "ACID"), "ACHO");
        assert_eq!(gene_code_from_parents("TBLK", "KKIT"), "KKTB");
    }

    #[test]
    fn fusion_sets_lineage() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([120.0, 100.0], 4, 20.0);

        reg.get_mut(a_id).unwrap().species = "ACID".to_string();
        reg.get_mut(b_id).unwrap().species = "DRON".to_string();
        reg.get_mut(a_id).unwrap().consent_flags = 1;
        reg.get_mut(b_id).unwrap().consent_flags = 1;
        reg.get_mut(a_id).unwrap().integrate_timers.insert(b_id, 5.0);

        let fusions = reg.check_integrations(3.0);
        let (_, _, new_id) = fusions[0];
        let new_org = reg.get(new_id).unwrap();

        assert_eq!(new_org.species, "ACDR", "species should be gene code from parents");
        assert_eq!(
            new_org.parent_codes,
            Some(("ACID".to_string(), "DRON".to_string())),
            "lineage should record parent codes"
        );
    }
}
