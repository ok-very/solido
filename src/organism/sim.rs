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

use crate::organism::dna::InteractionRule;

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
    // DNA-sourced params (f32 directly, independent of S12 types)
    pub core_radius: f32,
    pub lobe_count: u8,
    pub pseudopod_gain: f32,
    pub extension_speed: f32,
    pub retraction_speed: f32,
    pub drag: f32,
    pub max_speed: f32,
    pub mass: f32,
    pub viscosity: f32,

    // Render params
    pub smin_k: f32,
    pub edge_softness: f32,
    pub base_hue: f32,
    pub base_glow: f32,
    pub pulse_response: f32,

    // Per-organism emotion (fed from OrganismModule each frame)
    pub arousal: f32,   // [0,1] — controls thermal palette position
    pub valence: f32,   // [-1,1] — controls hue shift direction

    // Audio energy (fed from OrganismModule RMS each frame)
    pub audio_energy: f32, // [0,1] — drives BioField third band brightness + radius swell

    // Proximity energy (computed from sonar detections each tick)
    pub proximity_energy: f32, // [0,1] — interaction strength with nearby organisms

    // Base send levels from DNA (for proximity-based boost computation)
    pub reverb_send_base: f32,
    pub tape_delay_send_base: f32,

    // Connection drive (adapts from valence — happy organisms seek connection)
    pub desire_to_connect: f32, // [0, 1]

    // Pitch drift — smoothed blend toward gravity well influence
    pub scale_drift_blend: f32,  // current smoothed blend (starts 0.0)
    pub scale_drift_target: f32, // target from well influence computation

    // CPU-accumulated ring animation phase (avoids jitter from energy*time in shader)
    pub ring_phase: f32,

    // DNA-driven dissolve shape (from BodyDna)
    pub shape_amplitude: f32,  // how far noise pushes body edge (pixels)
    pub shape_frequency: f32,  // noise spatial scale (lower = fewer, larger lobes)

    // Reaction-diffusion trail parameters (from BodyDna)
    pub rd_reactivity: f32,  // 0.0–1.0, how reactive trail slime is
    pub rd_feed: f32,        // Gray-Scott f, controls pattern topology
    pub rd_kill: f32,        // Gray-Scott k, controls pattern topology
    pub rd_scale: f32,       // RD pattern scale, energy-modulated

    // Angular harmonics + velocity elongation (from BodyDna)
    pub harmonic_count: f32, // lobe count: 2=ellipse, 3=trefoil, 5=starfish
    pub harmonic_amp: f32,   // lobe amplitude [0, 0.4]
    pub elongation: f32,     // velocity stretch sensitivity [0, 1]
    pub chladni_m: f32,      // Chladni modal m (2–5)
    pub chladni_n: f32,      // Chladni modal n (1–3)

    // Smoothed visual heading (avoids angle-wrapping glitches)
    pub visual_dir: [f32; 2],      // smoothed direction unit vector for GPU/rendering
    pub smooth_speed: f32,          // smoothed speed scalar
    pub prev_audio_energy: f32,     // previous frame's audio_energy for RMS delta detection

    // Identity + interaction
    pub species: String,
    pub interaction_rules: Vec<InteractionRule>,

    // DNA-sourced steering params
    pub scale_affinity: f32,        // [0,1] — attraction to gravity wells
    pub root_pitch_class: u8,       // 0-11 — organism's tonal root
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
            core_radius,
            lobe_count,
            pseudopod_gain: 0.5,
            extension_speed: 4.0,
            retraction_speed: 6.0,
            drag: 0.95,
            max_speed: 200.0,
            mass: 1.0,
            viscosity: 0.5,
            smin_k: 0.3,
            edge_softness: 2.0,
            base_hue: 0.0,
            base_glow: 0.3,
            pulse_response: 0.2,
            arousal: 0.3,
            valence: 0.0,
            audio_energy: 0.0,
            proximity_energy: 0.0,
            reverb_send_base: 0.0,
            tape_delay_send_base: 0.0,
            desire_to_connect: 0.3,
            scale_drift_blend: 0.0,
            scale_drift_target: 0.0,
            ring_phase: 0.0,
            shape_amplitude: 30.0,
            shape_frequency: 0.012,
            rd_reactivity: 0.5,
            rd_feed: 0.035,
            rd_kill: 0.065,
            rd_scale: 2.0,
            harmonic_count: 0.0,
            harmonic_amp: 0.0,
            elongation: 0.0,
            chladni_m: 2.0,
            chladni_n: 1.0,
            visual_dir: [1.0, 0.0],
            smooth_speed: 0.0,
            prev_audio_energy: 0.0,
            species: String::from("unknown"),
            interaction_rules: Vec::new(),
            scale_affinity: 0.0,
            root_pitch_class: 0,
        }
    }

    /// Advance organism simulation by `dt` seconds.
    ///
    /// Updates position from velocity, applies drag, and drives lobe targets
    /// based on heading and energy.
    pub fn tick(&mut self, dt: f32, world_bounds: [f32; 4]) {
        // Derive energy from arousal — drives pseudopod extension
        let target_energy = 0.3 + self.arousal * 0.7;
        self.energy += (target_energy - self.energy) * dt * 2.0;

        // Desire adapts from valence: happy organisms want connection
        let desire_target = (0.2 + self.valence.max(0.0) * 0.7).clamp(0.0, 1.0);
        self.desire_to_connect += (desire_target - self.desire_to_connect) * dt * 0.5;

        // Apply velocity with frame-rate independent drag.
        // drag=0.9 means 10% velocity loss *per second*, not per frame.
        let frame_drag = self.drag.powf(dt);
        self.velocity[0] *= frame_drag;
        self.velocity[1] *= frame_drag;

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

        // Update heading from velocity (if moving), else wander slowly
        // Arousal modulates wander speed: excited organisms explore more
        if speed > 5.0 {
            self.heading = self.velocity[1].atan2(self.velocity[0]);
        } else {
            self.heading += dt * (0.3 + self.arousal * 0.5);
        }

        // Wander thrust: arousal-modulated push along heading.
        // Low arousal (DRON 0.3): 15 * 0.75 = 11.25 — sluggish drift
        // High arousal (ACID 0.8): 15 * 1.5 = 22.5 — energetic exploration
        let wander_strength = 15.0 * (0.3 + self.arousal * 1.5);
        self.velocity[0] += self.heading.cos() * wander_strength * dt;
        self.velocity[1] += self.heading.sin() * wander_strength * dt;

        // Audio → kinetic pulses: RMS spikes push organism along heading
        let rms_delta = (self.audio_energy - self.prev_audio_energy).max(0.0);
        self.prev_audio_energy = self.audio_energy;
        if rms_delta > 0.05 {
            let impulse = rms_delta * 80.0;
            self.velocity[0] += self.heading.cos() * impulse;
            self.velocity[1] += self.heading.sin() * impulse;
        }

        // Smooth visual direction (2D unit vector — no angle wrapping issues)
        let actual_speed = (self.velocity[0] * self.velocity[0]
            + self.velocity[1] * self.velocity[1])
            .sqrt();
        if actual_speed > 2.0 {
            let target_dir = [self.velocity[0] / actual_speed, self.velocity[1] / actual_speed];
            let rate = (6.0 * dt).min(1.0);
            self.visual_dir[0] += (target_dir[0] - self.visual_dir[0]) * rate;
            self.visual_dir[1] += (target_dir[1] - self.visual_dir[1]) * rate;
            // Renormalize
            let len = (self.visual_dir[0] * self.visual_dir[0]
                + self.visual_dir[1] * self.visual_dir[1])
                .sqrt();
            if len > 0.001 {
                self.visual_dir[0] /= len;
                self.visual_dir[1] /= len;
            }
        }
        // Smooth speed scalar
        self.smooth_speed += (actual_speed - self.smooth_speed) * (8.0 * dt).min(1.0);

        // Heading deflection near walls — steer away to prevent corner trapping
        let [min_x, min_y, max_x, max_y] = world_bounds;
        let wall_margin = 150.0;
        let deflect = 2.0 * dt;
        if self.position[0] < min_x + wall_margin && self.heading.cos() < 0.0 {
            self.heading += deflect;
        }
        if self.position[0] > max_x - wall_margin && self.heading.cos() > 0.0 {
            self.heading -= deflect;
        }
        if self.position[1] < min_y + wall_margin && self.heading.sin() < 0.0 {
            self.heading += deflect;
        }
        if self.position[1] > max_y - wall_margin && self.heading.sin() > 0.0 {
            self.heading -= deflect;
        }

        // Pitch drift: exponential approach toward well influence target
        // ~90% convergence in ~5.7s (alpha = 0.4 × dt per frame)
        self.scale_drift_blend += (self.scale_drift_target - self.scale_drift_blend) * 0.4 * dt;

        // Speed-driven crawl phase + audio dance modulation
        // Idle: 0.05 rad/s (gentle breathing), walking: ~1 rad/s, fast: 2.0 (clamped)
        let crawl_rate = (self.smooth_speed / 50.0).clamp(0.05, 2.0);
        let audio_mod = 1.0 + self.audio_energy * 0.8;
        self.ring_phase += dt * crawl_rate * audio_mod;

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

    /// Visual radius in screen pixels (core_radius × RADIUS_SCALE).
    /// Matches the GPU payload scale factor in `registry.rs::build_gpu_payload()`.
    pub fn visual_radius(&self) -> f32 {
        self.core_radius * 12.0
    }

    /// Apply an external force to the organism.
    pub fn apply_force(&mut self, force: [f32; 2]) {
        let inv_mass = 1.0 / self.mass.max(0.01);
        self.velocity[0] += force[0] * inv_mass;
        self.velocity[1] += force[1] * inv_mass;
    }

    /// Whether this organism consents to integration (union state).
    ///
    /// Dynamic: organism consents when desire is high and mood is positive.
    /// The static consent_flags bit acts as a hard override (always consent if set).
    pub fn consents_to_integrate(&self) -> bool {
        (self.consent_flags & 1 != 0)
            || (self.desire_to_connect > 0.7 && self.valence > 0.2)
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
        org.tick(1.0 / 60.0, [0.0, 0.0, 2000.0, 2000.0]);

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
        // Strong drag over a full second so the effect dominates wander thrust.
        // drag=0.1 → 90% loss per second; powf(1.0) = 0.1 → velocity ~10.
        org.drag = 0.1;

        let initial_speed = org.velocity[0];
        org.tick(1.0, [0.0, 0.0, 2000.0, 2000.0]);

        let speed =
            (org.velocity[0] * org.velocity[0] + org.velocity[1] * org.velocity[1]).sqrt();
        assert!(
            speed < initial_speed,
            "velocity should decrease with drag: {}",
            speed
        );
    }

    #[test]
    fn max_speed_clamped() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 6, 30.0);
        org.velocity = [10000.0, 0.0];
        org.max_speed = 200.0;
        org.drag = 1.0;
        org.tick(1.0 / 60.0, [0.0, 0.0, 2000.0, 2000.0]);

        // After clamping to max_speed, wander thrust (15.0 * dt) adds a small
        // amount along heading. Allow tolerance for that post-clamp push.
        let speed =
            (org.velocity[0] * org.velocity[0] + org.velocity[1] * org.velocity[1]).sqrt();
        assert!(
            speed <= 201.0,
            "speed should be clamped near max_speed: {}",
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
        // Static override
        org.consent_flags = 1;
        assert!(org.consents_to_integrate());
    }

    #[test]
    fn dynamic_consent_from_desire_and_valence() {
        let mut org = OrganismState::new(0, [0.0, 0.0], 4, 20.0);
        assert!(!org.consents_to_integrate());
        // High desire + positive valence → consents dynamically
        org.desire_to_connect = 0.8;
        org.valence = 0.5;
        assert!(org.consents_to_integrate());
        // Low desire → no consent (even with positive valence)
        org.desire_to_connect = 0.3;
        assert!(!org.consents_to_integrate());
    }

    #[test]
    fn desire_adapts_from_valence() {
        let mut org = OrganismState::new(0, [100.0, 100.0], 4, 20.0);
        org.desire_to_connect = 0.3;
        org.valence = 1.0; // very happy → target = 0.2 + 0.7 = 0.9
        org.drag = 1.0;

        // Tick for several seconds — desire should climb toward 0.9
        for _ in 0..300 {
            org.tick(1.0 / 60.0, [0.0, 0.0, 2000.0, 2000.0]);
        }
        assert!(
            org.desire_to_connect > 0.7,
            "desire should rise with positive valence: {}",
            org.desire_to_connect
        );
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
