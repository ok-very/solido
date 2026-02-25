/// Organism registry: owns all organisms, drives simulation, builds GPU payload.
///
/// Manages the lifecycle: spawn, despawn, tick, and GPU data extraction.
/// Integration (fusion) is triggered when IntegratePropose dwell timers exceed
/// threshold and both organisms consent.

use super::sim::{LobeState, OrganismId, OrganismState};
use crate::renderer::blob_renderer::{BlobOrgData, LobeGpu};

/// Central owner of all organisms in the simulation.
pub struct OrganismRegistry {
    organisms: Vec<OrganismState>,
    next_id: OrganismId,

    // World boundary (soft wall)
    pub world_bounds: [f32; 4], // [min_x, min_y, max_x, max_y]
    pub boundary_force: f32,
    pub boundary_margin: f32,
}

impl OrganismRegistry {
    pub fn new() -> Self {
        Self {
            organisms: Vec::new(),
            next_id: 0,
            world_bounds: [0.0, 0.0, 1200.0, 700.0],
            boundary_force: 50.0,
            boundary_margin: 80.0,
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

        // Tick each organism
        for org in &mut self.organisms {
            org.tick(dt);
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

    /// Build GPU payload for the blob renderer.
    ///
    /// Returns organism data and a flat lobe buffer. Each organism's `lobe_start`
    /// indexes into the lobe buffer.
    pub fn build_gpu_payload(
        &self,
        beat_phase: f32,
        valence: f32,
        arousal: f32,
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

            // Compute thermal temperature from arousal
            let thermal_temp = arousal.clamp(0.0, 1.0);

            // Hue shift from valence + base hue
            let hue_shift = org.base_hue + valence * 0.1;

            // Glow from arousal + base glow
            let glow = (org.base_glow + arousal * 0.5).clamp(0.0, 1.0);

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
                _pad: 0.0,
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

        let (orgs, lobes) = reg.build_gpu_payload(0.0, 0.0, 0.5);
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
}
