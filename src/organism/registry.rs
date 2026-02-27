/// Organism registry: owns all organisms, drives simulation, builds GPU payload.
///
/// Manages the lifecycle: spawn, despawn, tick, and GPU data extraction.
/// Integration (fusion) is triggered when IntegratePropose dwell timers exceed
/// threshold and both organisms consent.

use super::interaction::{self, AttachParams, GlobParams};
use super::sim::{OrganismId, OrganismState};
use super::sonar::Sonar;
use crate::organism::dna::InteractionMode;
use crate::renderer::blob_renderer::{BlobOrgData, LobeGpu};

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
}

impl OrganismRegistry {
    pub fn new() -> Self {
        Self {
            organisms: Vec::new(),
            next_id: 0,
            world_bounds: [0.0, 0.0, 1200.0, 700.0],
            boundary_force: 50.0,
            boundary_margin: 80.0,
            sonar: Sonar::new(),
        }
    }

    /// Spawn a new organism at the given position.
    pub fn spawn(&mut self, position: [f32; 2], lobe_count: u8, core_radius: f32) -> OrganismId {
        let id = self.next_id;
        self.next_id += 1;
        let org = OrganismState::new(id, position, lobe_count, core_radius);
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

        // Apply pairwise interaction forces from DNA rules
        self.apply_interactions(dt);

        // Tick each organism
        for org in &mut self.organisms {
            org.tick(dt);
        }
    }

    /// Apply pairwise interaction forces based on each organism's DNA rules.
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
            idx: usize,
        }
        let snaps: Vec<OrgSnap> = self
            .organisms
            .iter()
            .enumerate()
            .map(|(idx, o)| OrgSnap {
                state: o.clone(),
                idx,
            })
            .collect();

        // Accumulate forces per organism index
        let mut forces: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];

        for i in 0..n {
            for j in (i + 1)..n {
                let a = &snaps[i].state;
                let b = &snaps[j].state;

                // Check a's rules against b
                for rule in &a.interaction_rules {
                    if !species_matches(&rule.with_species, &b.species) {
                        continue;
                    }
                    let f = dispatch_interaction(a, b, rule, dt);
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
                    let f = dispatch_interaction(b, a, rule, dt);
                    forces[j][0] += f.force_a[0];
                    forces[j][1] += f.force_a[1];
                    forces[i][0] += f.force_b[0];
                    forces[i][1] += f.force_b[1];
                }
            }
        }

        // Apply accumulated forces
        for (idx, force) in forces.iter().enumerate() {
            self.organisms[idx].apply_force(*force);
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

    /// Build GPU payload for the blob renderer.
    ///
    /// Returns organism data and a flat lobe buffer. Each organism's `lobe_start`
    /// indexes into the lobe buffer.
    pub fn build_gpu_payload(
        &self,
        beat_phase: f32,
    ) -> (Vec<BlobOrgData>, Vec<LobeGpu>) {
        let mut org_data = Vec::with_capacity(self.organisms.len());
        let mut lobe_data = Vec::new();

        for org in &self.organisms {
            let lobe_start = lobe_data.len() as u32;

            // Collect lobe GPU data for active lobes
            for (i, lobe) in org.lobes.iter().enumerate() {
                if i >= org.lobe_count as usize {
                    break;
                }
                // Skip lobes with zero radius
                if lobe.radius < 0.001 {
                    continue;
                }
                lobe_data.push(LobeGpu {
                    offset: lobe.offset,
                    radius: lobe.radius,
                    _pad: 0.0,
                });
            }

            let actual_lobe_count = lobe_data.len() as u32 - lobe_start;

            // Per-organism emotion drives visual params (AD-1)
            let thermal_temp = org.arousal.clamp(0.0, 1.0);
            let hue_shift = org.base_hue + org.valence * 0.1;
            let glow = (org.base_glow + org.arousal * 0.5).clamp(0.0, 1.0);

            org_data.push(BlobOrgData {
                pos: org.position,
                smin_k: org.smin_k,
                edge_softness: org.edge_softness,
                thermal_temp,
                hue_shift,
                pulse_phase: beat_phase,
                pulse_amp: org.pulse_response,
                glow,
                lobe_start,
                lobe_count: actual_lobe_count,
                glob_group: org.glob_group.unwrap_or(0xFFFFFFFF),
            });
        }

        (org_data, lobe_data)
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

/// Check if a rule's `with_species` tag matches a target species.
fn species_matches(rule_species: &str, target_species: &str) -> bool {
    rule_species == "*" || rule_species == target_species
}

/// Dispatch an interaction rule to the appropriate physics function.
///
/// `a` is the rule owner, `b` is the other organism.
fn dispatch_interaction(
    a: &OrganismState,
    b: &OrganismState,
    rule: &crate::organism::dna::InteractionRule,
    _dt: f32,
) -> interaction::InteractionForce {
    // DNA ranges are in body units — sonar curiosity handles macro-scale attraction
    let range = rule.range;
    let strength = rule.strength;

    match rule.mode {
        InteractionMode::Repel => interaction::repel(a, b, range, strength),
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

        let (orgs, lobes) = reg.build_gpu_payload(0.0);
        assert_eq!(orgs.len(), 2);
        assert!(!lobes.is_empty());
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

        // Area-conserving radius: sqrt(30^2 + 20^2) = sqrt(1300) ~= 36.06
        let expected_radius = (30.0_f32 * 30.0 + 20.0 * 20.0).sqrt();
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
    fn gpu_payload_contains_glob_group() {
        let mut reg = OrganismRegistry::new();
        let a_id = reg.spawn([100.0, 100.0], 4, 20.0);
        let b_id = reg.spawn([200.0, 100.0], 4, 20.0);

        reg.update_glob_groups(&[(a_id, b_id, 0.9)], 0.65);

        let (orgs, _) = reg.build_gpu_payload(0.0);
        assert_eq!(orgs.len(), 2);
        // Both should have the same glob group (not sentinel)
        assert_ne!(orgs[0].glob_group, 0xFFFFFFFF);
        assert_eq!(orgs[0].glob_group, orgs[1].glob_group);
    }

    #[test]
    fn species_wildcard_matches() {
        assert!(super::species_matches("*", "anything"));
        assert!(super::species_matches("dron", "dron"));
        assert!(!super::species_matches("dron", "melo"));
    }
}
