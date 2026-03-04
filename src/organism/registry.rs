/// Organism registry: owns all organisms, drives simulation, builds GPU payload.
///
/// Manages the lifecycle: spawn, despawn, tick, and GPU data extraction.
/// Integration (fusion) is triggered when IntegratePropose dwell timers exceed
/// threshold and both organisms consent.

use std::collections::HashMap;

use super::interaction::{self, AttachParams, GlobParams};
use super::sim::{OrganismId, OrganismState};
use super::sonar::Sonar;
use crate::organism::dna::InteractionMode;
use crate::renderer::biofield_renderer::CellData;

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
    pub fn count(&self) -> usize {
        self.organisms.len()
    }

    /// Read-only access to all organisms.
    pub fn organisms(&self) -> &[OrganismState] {
        &self.organisms
    }

    /// Advance all organisms by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        // Apply world boundary forces
        self.apply_boundary_forces();

        // Sonar: periodic neighbor detection + curiosity attraction
        self.sonar.tick(dt, &self.organisms);
        let curiosity = self.sonar.curiosity_forces();
        for (org_id, force) in &curiosity {
            if let Some(org) = self.organisms.iter_mut().find(|o| o.id == *org_id) {
                org.apply_force(*force);
            }
        }

        // Compute emergent pairwise affinities from proximity + audio + desire
        self.compute_emergent_affinities(dt);

        // Update glob groups from emergent affinities (visual merging)
        self.refresh_glob_groups(0.25);

        // Apply pairwise interaction forces from DNA rules
        self.apply_interactions(dt);

        // Compute per-organism proximity_energy from sonar detections
        self.compute_proximity_energy();

        // Tick each organism
        for org in &mut self.organisms {
            org.tick(dt);
        }

        // Hard boundary clamp + velocity kill — prevents corner trapping
        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        for org in &mut self.organisms {
            let clamp_margin = org.core_radius * 2.0;
            org.position[0] = org.position[0].clamp(min_x + clamp_margin, max_x - clamp_margin);
            org.position[1] = org.position[1].clamp(min_y + clamp_margin, max_y - clamp_margin);
            // Zero outward velocity at walls
            if org.position[0] <= min_x + clamp_margin && org.velocity[0] < 0.0 {
                org.velocity[0] = 0.0;
            }
            if org.position[0] >= max_x - clamp_margin && org.velocity[0] > 0.0 {
                org.velocity[0] = 0.0;
            }
            if org.position[1] <= min_y + clamp_margin && org.velocity[1] < 0.0 {
                org.velocity[1] = 0.0;
            }
            if org.position[1] >= max_y - clamp_margin && org.velocity[1] > 0.0 {
                org.velocity[1] = 0.0;
            }
        }

        // Check for union state (fusion) — 5 second dwell threshold
        let _fusions = self.check_integrations(5.0);
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

        // Accumulate forces per organism index
        let mut forces: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];
        // Pairs that qualify for union state dwell timer ticking
        let mut dwell_pairs: Vec<(usize, usize, f32)> = Vec::new();

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

                // Emergent attraction: affinity + mutual desire → pull together
                if affinity > 0.2 {
                    let desire_avg = (a.desire_to_connect + b.desire_to_connect) * 0.5;
                    let attract_strength = (affinity - 0.2) * desire_avg * 10.0;
                    let f = interaction::attract(a, b, 500.0, attract_strength);
                    forces[i][0] += f.force_a[0];
                    forces[i][1] += f.force_a[1];
                    forces[j][0] += f.force_b[0];
                    forces[j][1] += f.force_b[1];
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

        // Apply accumulated forces
        for (idx, force) in forces.iter().enumerate() {
            self.organisms[idx].apply_force(*force);
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
    fn apply_boundary_forces(&mut self) {
        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        let margin = self.boundary_margin;
        let force = self.boundary_force;

        for org in &mut self.organisms {
            // Left wall
            if org.position[0] < min_x + margin {
                let penetration = (min_x + margin - org.position[0]) / margin;
                org.velocity[0] += force * penetration;
            }
            // Right wall
            if org.position[0] > max_x - margin {
                let penetration = (org.position[0] - (max_x - margin)) / margin;
                org.velocity[0] -= force * penetration;
            }
            // Top wall
            if org.position[1] < min_y + margin {
                let penetration = (min_y + margin - org.position[1]) / margin;
                org.velocity[1] += force * penetration;
            }
            // Bottom wall
            if org.position[1] > max_y - margin {
                let penetration = (org.position[1] - (max_y - margin)) / margin;
                org.velocity[1] -= force * penetration;
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
    /// Formula: proximity × (0.3 + audio_correlation × 0.4 + desire_avg × 0.3)
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

                let target = proximity * (0.3 + audio_corr * 0.4 + desire_avg * 0.3);

                // Exponential smoothing: affinity tracks target, rises and decays
                let existing = self.pairwise_affinities.get(&key).copied().unwrap_or(0.0);
                let smoothed = existing + (target - existing) * smooth_rate;
                self.pairwise_affinities.insert(key, smoothed.clamp(0.0, 1.0));
            }
        }
    }

    /// Refresh glob groups from stored pairwise affinities (called after emergent computation).
    fn refresh_glob_groups(&mut self, threshold: f32) {
        for org in &mut self.organisms {
            org.glob_group = None;
        }

        let mut next_group: u32 = 0;
        // Collect pairs above threshold from stored affinities
        let pairs: Vec<(OrganismId, OrganismId, f32)> = self
            .pairwise_affinities
            .iter()
            .filter(|(_, &w)| w >= threshold)
            .map(|(&(a, b), &w)| (a, b, w))
            .collect();

        for (org_a, org_b, _weight) in pairs {
            let group_a = self.get(org_a).and_then(|o| o.glob_group);
            let group_b = self.get(org_b).and_then(|o| o.glob_group);

            let group = match (group_a, group_b) {
                (Some(g), _) | (_, Some(g)) => g,
                (None, None) => {
                    let g = next_group;
                    next_group += 1;
                    g
                }
            };

            if let Some(org) = self.get_mut(org_a) {
                org.glob_group = Some(group);
            }
            if let Some(org) = self.get_mut(org_b) {
                org.glob_group = Some(group);
            }
        }
    }

    /// Update glob groups from external affinity data.
    ///
    /// Organisms with pairwise affinity above the threshold are placed in the
    /// same glob group, causing visual merging in the shader.
    pub fn update_glob_groups(&mut self, affinities: &[(OrganismId, OrganismId, f32)], threshold: f32) {
        // Clear all groups
        for org in &mut self.organisms {
            org.glob_group = None;
        }

        let mut next_group: u32 = 0;

        for &(org_a, org_b, weight) in affinities {
            if weight < threshold {
                continue;
            }

            let group_a = self.get(org_a).and_then(|o| o.glob_group);
            let group_b = self.get(org_b).and_then(|o| o.glob_group);

            let group = match (group_a, group_b) {
                (Some(g), _) | (_, Some(g)) => g,
                (None, None) => {
                    let g = next_group;
                    next_group += 1;
                    g
                }
            };

            if let Some(org) = self.get_mut(org_a) {
                org.glob_group = Some(group);
            }
            if let Some(org) = self.get_mut(org_b) {
                org.glob_group = Some(group);
            }
        }
    }

    /// Store pairwise affinities from the Hebbian graph for use in interaction dispatch.
    pub fn update_affinities(&mut self, affinities: &[(OrganismId, OrganismId, f32)]) {
        self.pairwise_affinities.clear();
        for &(a, b, w) in affinities {
            let key = if a < b { (a, b) } else { (b, a) };
            self.pairwise_affinities.insert(key, w);
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
            let energy_swell = 1.0 + org.audio_energy * 0.3;
            cells.push(CellData {
                pos:          org.position,
                radius:       org.core_radius * RADIUS_SCALE * energy_swell,
                audio_energy: org.audio_energy,
                cell_id,
                hue:          org.base_hue,
                vel:          org.velocity,
            });
        }
        cells
    }

    /// Check and execute integration (fusion) events.
    ///
    /// When two organisms have been within range for dwell_threshold seconds
    /// and both consent, merge them into a new organism.
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

        // Apply energy-weighted averaging for render params
        if let Some(new_org) = self.get_mut(new_id) {
            let total_energy = a.energy + b.energy;
            let wa = if total_energy > 0.001 {
                a.energy / total_energy
            } else {
                0.5
            };
            let wb = 1.0 - wa;

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
        }

        // Despawn parents
        self.despawn(a_id);
        self.despawn(b_id);

        Some(new_id)
    }
}

// ============================================================================
// Free helpers for interaction dispatch
// ============================================================================

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
        InteractionMode::Slow => interaction::slow(a, b, range, strength),
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
            let centroid = [
                (a.position[0] + b.position[0]) * 0.5,
                (a.position[1] + b.position[1]) * 0.5,
            ];
            let params = GlobParams {
                attraction_range: range,
                attraction_strength: strength,
                viscosity: 0.8,
                centroid_pull: 2.0,
            };
            interaction::glob(a, b, &params, centroid)
        }
        InteractionMode::Orbit => interaction::orbit(a, b, range, strength),
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
    fn glob_groups_assigned_by_affinity() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);
        let c_id = reg.spawn([500.0, 500.0], 4, 20.0);

        // High affinity between A and B, low for C
        let affinities = vec![
            (a_id, b_id, 0.8),
            (a_id, c_id, 0.2),
            (b_id, c_id, 0.1),
        ];

        reg.update_glob_groups(&affinities, 0.65);

        let a = reg.get(a_id).unwrap();
        let b = reg.get(b_id).unwrap();
        let c = reg.get(c_id).unwrap();

        assert!(a.glob_group.is_some(), "A should be in a glob group");
        assert_eq!(a.glob_group, b.glob_group, "A and B should share glob group");
        assert!(c.glob_group.is_none(), "C should not be in a glob group");
    }

    #[test]
    fn build_gpu_payload_assigns_sequential_ids() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);

        reg.update_glob_groups(&[(a_id, b_id, 0.9)], 0.65);

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
}
