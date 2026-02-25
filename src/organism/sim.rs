/// Organism simulation state and lobe dynamics.
///
/// Each organism is a soft body with 1-12 lobes (circle metaballs). Core lobes
/// form the stable mass; pseudopod lobes extend along the heading direction.
/// Lobes lerp toward target offsets/radii each frame, producing amoeba-like
/// silhouette changes without meshes.
///
/// DNA parameters are referenced as f32 fields directly (not OrganismDna types)
/// so this module can run independently of S12.

use std::collections::HashMap;

/// Unique organism identifier.
pub type OrganismId = u32;

/// Per-lobe simulation state.
#[derive(Debug, Clone)]
pub struct LobeState {
    /// Current offset from organism centroid.
    pub offset: [f32; 2],
    /// Current radius.
    pub radius: f32,
    /// Target offset (lerp destination).
    pub target_offset: [f32; 2],
    /// Target radius (lerp destination).
    pub target_radius: f32,
}

impl LobeState {
    pub fn new(offset: [f32; 2], radius: f32) -> Self {
        Self {
            offset,
            radius,
            target_offset: offset,
            target_radius: radius,
        }
    }

    /// Advance lobe toward its target by `dt` seconds.
    pub fn tick(&mut self, dt: f32, extension_speed: f32, retraction_speed: f32) {
        // Determine if extending (growing) or retracting (shrinking)
        let target_mag = (self.target_offset[0] * self.target_offset[0]
            + self.target_offset[1] * self.target_offset[1])
            .sqrt();
        let current_mag = (self.offset[0] * self.offset[0]
            + self.offset[1] * self.offset[1])
            .sqrt();
        let speed = if target_mag > current_mag {
            extension_speed
        } else {
            retraction_speed
        };

        let t = (speed * dt).clamp(0.0, 1.0);
        self.offset[0] += (self.target_offset[0] - self.offset[0]) * t;
        self.offset[1] += (self.target_offset[1] - self.offset[1]) * t;
        self.radius += (self.target_radius - self.radius) * t;
    }
}

/// Per-organism simulation state.
#[derive(Debug, Clone)]
pub struct OrganismState {
    pub id: OrganismId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub heading: f32,
    pub energy: f32,
    pub lobes: Vec<LobeState>,
    /// Bit 0: consent for IntegratePropose
    pub consent_flags: u8,
    /// Timers tracking IntegratePropose dwell with other organisms.
    pub integrate_timers: HashMap<OrganismId, f32>,
    /// Glob group membership (None = independent).
    pub glob_group: Option<u32>,

    // DNA-sourced params (f32 directly, independent of S12 types)
    pub core_radius: f32,
    pub lobe_count: u8,
    pub pseudopod_gain: f32,
    pub extension_speed: f32,
    pub retraction_speed: f32,
    pub drag: f32,
    pub max_speed: f32,
    pub mass: f32,

    // Render params
    pub smin_k: f32,
    pub edge_softness: f32,
    pub base_hue: f32,
    pub base_glow: f32,
    pub pulse_response: f32,
}

impl OrganismState {
    /// Create a new organism with default parameters.
    pub fn new(id: OrganismId, position: [f32; 2], lobe_count: u8, core_radius: f32) -> Self {
        let mut lobes = Vec::with_capacity(lobe_count as usize);

        // Place lobes in a ring around the centroid
        for i in 0..lobe_count {
            let angle = (i as f32 / lobe_count as f32) * std::f32::consts::TAU;
            let dist = core_radius * 0.3;
            let offset = [angle.cos() * dist, angle.sin() * dist];
            let radius = core_radius * if i < 3 { 0.8 } else { 0.5 };
            lobes.push(LobeState::new(offset, radius));
        }

        Self {
            id,
            position,
            velocity: [0.0, 0.0],
            heading: 0.0,
            energy: 0.5,
            lobes,
            consent_flags: 0,
            integrate_timers: HashMap::new(),
            glob_group: None,
            core_radius,
            lobe_count,
            pseudopod_gain: 0.5,
            extension_speed: 4.0,
            retraction_speed: 6.0,
            drag: 0.95,
            max_speed: 200.0,
            mass: 1.0,
            smin_k: 0.3,
            edge_softness: 2.0,
            base_hue: 0.0,
            base_glow: 0.3,
            pulse_response: 0.2,
        }
    }

    /// Advance organism simulation by `dt` seconds.
    ///
    /// Updates position from velocity, applies drag, and drives lobe targets
    /// based on heading and energy.
    pub fn tick(&mut self, dt: f32) {
        // Apply velocity with drag
        self.velocity[0] *= self.drag;
        self.velocity[1] *= self.drag;

        // Clamp speed
        let speed = (self.velocity[0] * self.velocity[0]
            + self.velocity[1] * self.velocity[1])
            .sqrt();
        if speed > self.max_speed {
            let scale = self.max_speed / speed;
            self.velocity[0] *= scale;
            self.velocity[1] *= scale;
        }

        // Update position
        self.position[0] += self.velocity[0] * dt;
        self.position[1] += self.velocity[1] * dt;

        // Update heading from velocity (if moving)
        if speed > 1.0 {
            self.heading = self.velocity[1].atan2(self.velocity[0]);
        }

        // Drive lobe targets
        self.update_lobe_targets();

        // Tick each lobe
        for lobe in &mut self.lobes {
            lobe.tick(dt, self.extension_speed, self.retraction_speed);
        }
    }

    /// Set lobe targets based on heading, energy, and pseudopod gain.
    ///
    /// - Leading pseudopod lobe (index >= core count) extends along heading.
    /// - Core lobes (first 2-3) stay close to centroid.
    /// - Trailing lobes retract toward core_radius.
    fn update_lobe_targets(&mut self) {
        let core_count = (self.lobe_count as usize).min(3);
        let extension = self.pseudopod_gain * self.energy;

        for (i, lobe) in self.lobes.iter_mut().enumerate() {
            if i >= self.lobe_count as usize {
                // Unused lobe — fade to zero
                lobe.target_radius = 0.0;
                lobe.target_offset = [0.0, 0.0];
                continue;
            }

            if i < core_count {
                // Core lobe: stay close to centroid with stable radius
                let angle = (i as f32 / core_count as f32) * std::f32::consts::TAU;
                let dist = self.core_radius * 0.2;
                lobe.target_offset = [angle.cos() * dist, angle.sin() * dist];
                lobe.target_radius = self.core_radius * 0.8;
            } else {
                // Pseudopod lobe
                let lobe_idx = (i - core_count) as f32;
                let total_pseudo = (self.lobe_count as usize - core_count).max(1) as f32;
                let spread = (lobe_idx / total_pseudo - 0.5) * 1.0;

                // Rotate heading for spread
                let angle = self.heading + spread;
                let dir = [angle.cos(), angle.sin()];

                // Leading pseudopod extends further
                let distance = if i == core_count {
                    // First pseudopod = leading
                    self.core_radius * (0.5 + extension * 1.5)
                } else {
                    self.core_radius * (0.3 + extension * 0.5)
                };

                lobe.target_offset = [dir[0] * distance, dir[1] * distance];
                lobe.target_radius = self.core_radius * (0.4 + extension * 0.3);
            }
        }
    }

    /// Apply an external force to the organism.
    pub fn apply_force(&mut self, force: [f32; 2]) {
        let inv_mass = 1.0 / self.mass.max(0.01);
        self.velocity[0] += force[0] * inv_mass;
        self.velocity[1] += force[1] * inv_mass;
    }

    /// Whether this organism consents to IntegratePropose.
    pub fn consents_to_integrate(&self) -> bool {
        self.consent_flags & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_organism_has_correct_lobe_count() {
        let org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        assert_eq!(org.lobes.len(), 6);
    }

    #[test]
    fn lobes_converge_toward_targets() {
        let mut lobe = LobeState::new([0.0, 0.0], 10.0);
        lobe.target_offset = [20.0, 0.0];
        lobe.target_radius = 15.0;

        for _ in 0..60 {
            lobe.tick(1.0 / 60.0, 4.0, 6.0);
        }

        // After 60 frames at 4x speed, should be close to target
        assert!(
            (lobe.offset[0] - 20.0).abs() < 2.0,
            "offset[0]={}, expected ~20.0",
            lobe.offset[0]
        );
        assert!(
            (lobe.radius - 15.0).abs() < 1.0,
            "radius={}, expected ~15.0",
            lobe.radius
        );
    }

    #[test]
    fn pseudopod_extends_in_heading_direction() {
        let mut org = OrganismState::new(0, [200.0, 200.0], 6, 30.0);
        org.heading = 0.0; // pointing right
        org.energy = 1.0;
        org.pseudopod_gain = 0.8;

        // Update targets
        org.update_lobe_targets();

        // First pseudopod (index 3) should extend rightward
        let pseudo = &org.lobes[3];
        assert!(
            pseudo.target_offset[0] > 0.0,
            "leading pseudopod should extend right: offset=({}, {})",
            pseudo.target_offset[0],
            pseudo.target_offset[1]
        );
    }

    #[test]
    fn organism_tick_advances_position() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        org.velocity = [60.0, 0.0];
        org.drag = 1.0; // no drag for test
        org.tick(1.0 / 60.0);

        assert!(
            org.position[0] > 100.0,
            "position should advance: {}",
            org.position[0]
        );
    }

    #[test]
    fn drag_slows_organism() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        org.velocity = [100.0, 0.0];
        org.drag = 0.9;

        let initial_speed = org.velocity[0];
        org.tick(1.0 / 60.0);

        assert!(
            org.velocity[0] < initial_speed,
            "velocity should decrease with drag: {}",
            org.velocity[0]
        );
    }

    #[test]
    fn max_speed_clamped() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        org.velocity = [10000.0, 0.0];
        org.max_speed = 200.0;
        org.drag = 1.0;
        org.tick(1.0 / 60.0);

        let speed =
            (org.velocity[0] * org.velocity[0] + org.velocity[1] * org.velocity[1]).sqrt();
        assert!(
            speed <= 200.1,
            "speed should be clamped to max_speed: {}",
            speed
        );
    }

    #[test]
    fn apply_force_changes_velocity() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        org.apply_force([10.0, 5.0]);
        assert!(org.velocity[0] > 0.0);
        assert!(org.velocity[1] > 0.0);
    }

    #[test]
    fn consent_flags() {
        let mut org = OrganismState::new(0, [0.0, 0.0], 4, 20.0);
        assert!(!org.consents_to_integrate());
        org.consent_flags = 1;
        assert!(org.consents_to_integrate());
    }

    #[test]
    fn unused_lobes_fade_to_zero() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        // Reduce lobe count to 3 but keep 6 LobeState entries
        org.lobe_count = 3;
        org.update_lobe_targets();

        // Lobes 3-5 should target zero radius
        for i in 3..6 {
            assert_eq!(
                org.lobes[i].target_radius, 0.0,
                "lobe {} should target zero radius",
                i
            );
        }
    }
}
