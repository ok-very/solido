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

use crate::organism::chladni::NodeWell;
use crate::organism::dna::InteractionRule;

/// Inline xorshift32 PRNG — RT-safe, no allocation, no syscall.
#[inline]
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Convert xorshift output to [0.0, 1.0) float.
#[inline]
fn rand_f32(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32)
}

// S37 Animation constants
const CRAWL_GAIN: f32 = 0.7;
const CRAWL_REF: f32 = 30.0;
const RING_PHASE_WRAP: f32 = 256.0 * std::f32::consts::TAU;
const SMOOTH_SPEED_LO: f32 = 6.0;
const SMOOTH_SPEED_HI: f32 = 20.0;
const DIR_SMOOTH_LO: f32 = 4.0;
const DIR_SMOOTH_HI: f32 = 14.0;
const SPEED_ADAPT_REF: f32 = 100.0;

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

    // CPU-accumulated animation phases (decoupled for S38 Chladni separation)
    pub ring_phase: f32,     // audio-driven: concentric ring scrolling
    pub chladni_phase: f32,  // speed-driven: Chladni lobe animation

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

    // Stasis detection — burst when stuck
    pub stasis_timer: f32,          // seconds at near-zero speed

    // Brownian noise PRNG state (xorshift32, seeded from organism id)
    pub rng_state: u32,

    // DNA-sourced steering params
    pub scale_affinity: f32,        // [0,1] — attraction to gravity wells
    pub root_pitch_class: u8,       // 0-11 — organism's tonal root

    // Substrate navigation tracking (replaces WellTracker)
    pub previous_local_energy: f32,  // For discovery detection
    pub grazing_run_timer: f32,      // Sustained moving+eating timer
    pub grazing_run_energy: f32,     // Energy consumed during grazing run
    pub starvation_timer: f32,       // Time stuck in depleted substrate
    pub transition_cooldown: f32,    // Prevent rapid transition spam

    // S40: Harmonic interaction — live sequencer pitch for pairwise consonance
    pub current_seq_pitch_hz: f32,  // Hz, 0.0 if not playing

    // Chladni node wells — sub-nodes as mini gravity wells
    pub node_wells: Vec<NodeWell>,       // one per sub-node, allocated at spawn
    pub node_energy_balance: f32,        // net energy delta this frame
    pub node_drain_rate: f32,            // DNA: how fast nodes deplete (host generosity)
    pub node_absorption_rate: f32,       // DNA: how much energy gained visiting others
    pub node_regen_rate: f32,            // DNA: solitude recovery speed

    // Nutrient channels — 3 abstract types that deplete over time and are
    // replenished by feeding from organisms with different species profiles.
    pub nutrient_levels: [f32; 3],       // [0,1] per channel
    pub monotony_timer: f32,             // seconds of sustained satiation + low arousal

    // Substrate pitch histogram — rolling record of consumed pitch classes.
    // Drives SetScaleWeights: what you ate IS your scale.
    pub pitch_histogram: [f32; 12],
    /// EWMA decay for pitch histogram [0.95, 0.995]. Slow = long memory.
    pub pitch_histogram_decay: f32,

    // Dual-root discovery (fusion hybrids)
    /// Secondary root pitch class from fusion parent (None for non-hybrid organisms).
    pub alt_root_pitch_class: Option<u8>,
    /// Blend weight [0,1] between root (0.0) and alt_root (1.0).
    /// Drifts based on consonance feedback. Starts at 0.5 for hybrids.
    pub root_blend: f32,
    /// Seconds that root_blend has been past the commit threshold (>0.8 or <0.2).
    pub root_blend_commit_timer: f32,

    // Lineage
    /// Parent gene codes for hybrid organisms. None for base species.
    pub parent_codes: Option<(String, String)>,
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
            chladni_phase: 0.0,
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
            stasis_timer: 0.0,
            rng_state: (id.wrapping_mul(2654435761)) | 1, // ensure non-zero for xorshift
            scale_affinity: 0.0,
            root_pitch_class: 0,
            previous_local_energy: 0.5,
            grazing_run_timer: 0.0,
            grazing_run_energy: 0.0,
            starvation_timer: 0.0,
            transition_cooldown: 0.0,
            current_seq_pitch_hz: 0.0,
            node_wells: Vec::new(),
            node_energy_balance: 0.0,
            node_drain_rate: 0.005,
            node_absorption_rate: 0.004,
            node_regen_rate: 0.015,
            nutrient_levels: [0.5; 3],
            monotony_timer: 0.0,
            pitch_histogram: [0.0; 12],
            pitch_histogram_decay: 0.98,
            alt_root_pitch_class: None,
            root_blend: 0.0,
            root_blend_commit_timer: 0.0,
            parent_codes: None,
        }
    }

    /// Apply audio-to-kinetic impulse: RMS spikes push organism along heading.
    /// Called once per frame (not per substep) — discrete event from audio thread.
    pub fn apply_audio_impulse(&mut self) {
        let rms_delta = (self.audio_energy - self.prev_audio_energy).max(0.0);
        self.prev_audio_energy = self.audio_energy;
        if rms_delta > 0.05 {
            let impulse = rms_delta * 80.0;
            self.velocity[0] += self.heading.cos() * impulse;
            self.velocity[1] += self.heading.sin() * impulse;
        }
    }

    /// Fixed-timestep physics integration: drag, speed clamp, position update,
    /// heading, wander thrust, and wall heading deflection.
    /// Called per substep at PHYS_DT (1/120s).
    pub fn tick_physics(&mut self, dt: f32, world_bounds: [f32; 4]) {
        // Apply velocity with frame-rate independent drag.
        // drag=0.9 means 10% velocity loss *per second*, not per frame.
        let frame_drag = self.drag.powf(dt);
        self.velocity[0] *= frame_drag;
        self.velocity[1] *= frame_drag;

        // Proximity drag: organisms near others experience increased damping.
        // viscosity (DNA "sliminess") × proximity_energy (sonar-derived [0,1]).
        // At viscosity=0.8 + full proximity: ~50% extra damping/s at 120Hz.
        let prox_damp = (1.0 - self.viscosity * self.proximity_energy * dt * 8.0).clamp(0.5, 1.0);
        self.velocity[0] *= prox_damp;
        self.velocity[1] *= prox_damp;

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
        // Low arousal (DRON 0.3): 40 * 0.75 = 30 — noticeable drift
        // High arousal (ACID 0.8): 40 * 1.5 = 60 — energetic exploration
        let wander_strength = 40.0 * (0.3 + self.arousal * 1.5);
        self.velocity[0] += self.heading.cos() * wander_strength * dt;
        self.velocity[1] += self.heading.sin() * wander_strength * dt;

        // Brownian thermal noise: ambient perturbation prevents zero-velocity equilibrium.
        // ~3 px/s per axis per substep — gentle, invisible, but prevents heat death.
        const BROWNIAN_STRENGTH: f32 = 3.0;
        let nx = (rand_f32(&mut self.rng_state) - 0.5) * 2.0 * BROWNIAN_STRENGTH;
        let ny = (rand_f32(&mut self.rng_state) - 0.5) * 2.0 * BROWNIAN_STRENGTH;
        self.velocity[0] += nx;
        self.velocity[1] += ny;

        // Stasis burst: if slow for 0.8+ seconds, kick in a new direction.
        // Threshold 10.0 catches equilibrium orbits from Orbit+proximity drag balance.
        if speed < 10.0 {
            self.stasis_timer += dt;
            if self.stasis_timer >= 0.8 {
                let burst_angle = (self.id as f32 * 0.618).fract() * std::f32::consts::TAU
                    + self.stasis_timer * 1.7; // vary per burst
                self.velocity[0] += burst_angle.cos() * 90.0;
                self.velocity[1] += burst_angle.sin() * 90.0;
                self.stasis_timer = 0.0;
            }
        } else {
            self.stasis_timer = 0.0;
        }

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
    }

    /// Visual-only updates: energy/desire convergence, visual_dir smoothing,
    /// smooth_speed, scale_drift, ring_phase, lobe targets+tick.
    /// Called once per frame (not per substep).
    pub fn tick_visual(&mut self, dt: f32) {
        // Derive energy from arousal + average node well energy.
        // Node depletion visually shrinks the organism.
        let avg_node_energy = if self.node_wells.is_empty() {
            1.0
        } else {
            let sum: f32 = self.node_wells.iter().map(|w| w.energy).sum();
            sum / self.node_wells.len() as f32
        };
        let target_energy = 0.3 + self.arousal * 0.4 + avg_node_energy * 0.3;
        self.energy += (target_energy - self.energy) * dt * 2.0;

        // Desire adapts from valence: happy organisms want connection
        let desire_target = (0.2 + self.valence.max(0.0) * 0.7).clamp(0.0, 1.0);
        self.desire_to_connect += (desire_target - self.desire_to_connect) * dt * 0.5;

        // Speed-adaptive smoothing: faster movement = faster visual tracking
        let actual_speed = (self.velocity[0] * self.velocity[0]
            + self.velocity[1] * self.velocity[1])
            .sqrt();
        let speed_ratio = (actual_speed / SPEED_ADAPT_REF).min(1.0);

        // Smooth visual direction (2D unit vector — no angle wrapping issues)
        if actual_speed > 2.0 {
            let target_dir = [self.velocity[0] / actual_speed, self.velocity[1] / actual_speed];
            let dir_alpha = DIR_SMOOTH_LO + (DIR_SMOOTH_HI - DIR_SMOOTH_LO) * speed_ratio;
            let rate = (dir_alpha * dt).min(1.0);
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
        // Smooth speed scalar (speed-adaptive)
        let speed_alpha = SMOOTH_SPEED_LO + (SMOOTH_SPEED_HI - SMOOTH_SPEED_LO) * speed_ratio;
        self.smooth_speed += (actual_speed - self.smooth_speed) * (speed_alpha * dt).min(1.0);

        // Pitch drift: exponential approach toward well influence target
        // ~90% convergence in ~5.7s (alpha = 0.4 × dt per frame)
        self.scale_drift_blend += (self.scale_drift_target - self.scale_drift_blend) * 0.4 * dt;

        // Concentric rings: audio-driven breathing (decoupled from Chladni)
        let ring_crawl = 0.3 + self.audio_energy * 1.5;
        self.ring_phase += dt * ring_crawl;

        // Chladni lobes: speed-driven locomotion expression (log compression)
        let crawl_rate = 0.05 + CRAWL_GAIN * (1.0 + self.smooth_speed / CRAWL_REF).ln();
        let audio_mod = 1.0 + self.audio_energy * 0.8;
        self.chladni_phase += dt * crawl_rate * audio_mod;

        // Phase wrapping — prevent f32 precision loss after long runtime
        if self.ring_phase > RING_PHASE_WRAP { self.ring_phase -= RING_PHASE_WRAP; }
        if self.chladni_phase > RING_PHASE_WRAP { self.chladni_phase -= RING_PHASE_WRAP; }

        // Drive lobe targets
        self.update_lobe_targets();

        // Tick each lobe
        for lobe in &mut self.lobes {
            lobe.tick(dt, self.extension_speed, self.retraction_speed);
        }
    }

    /// Advance organism simulation by `dt` seconds (compat wrapper).
    ///
    /// Calls apply_audio_impulse + tick_physics + tick_visual in sequence.
    /// Used by tests and legacy call sites.
    #[allow(dead_code)]
    pub fn tick(&mut self, dt: f32, world_bounds: [f32; 4]) {
        self.apply_audio_impulse();
        self.tick_physics(dt, world_bounds);
        self.tick_visual(dt);
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

    // === S36: Fixed timestep tests ===

    #[test]
    fn substep_consistency() {
        // Two organisms with identical initial conditions.
        // One ticked with 120 substeps of 1/120, the other with 60 substeps of 1/60.
        // Both cover 1 second. Positions should be close (within 5% tolerance).
        let bounds = [0.0, 0.0, 2000.0, 2000.0];
        let make = || {
            let mut org = OrganismState::new(0, [500.0, 500.0], 4, 20.0);
            org.velocity = [80.0, 30.0];
            org.drag = 0.95;
            org.arousal = 0.5;
            org
        };

        let mut org_120 = make();
        for _ in 0..120 {
            org_120.tick_physics(1.0 / 120.0, bounds);
        }

        let mut org_60 = make();
        for _ in 0..60 {
            org_60.tick_physics(1.0 / 60.0, bounds);
        }

        let dx = (org_120.position[0] - org_60.position[0]).abs();
        let dy = (org_120.position[1] - org_60.position[1]).abs();
        let dist = (dx * dx + dy * dy).sqrt();
        // Both cover 1 second of integration. Euler error means they differ,
        // plus Brownian noise adds per-substep divergence (~sqrt(N) scaling).
        // 15% tolerance accounts for both Euler error and stochastic noise.
        let travel_120 = ((org_120.position[0] - 500.0).powi(2)
            + (org_120.position[1] - 500.0).powi(2))
            .sqrt();
        assert!(
            dist < travel_120 * 0.15 + 2.0, // +2 for near-zero travel edge case
            "positions should be within 15%: dist={dist}, travel={travel_120}"
        );
    }

    #[test]
    fn audio_impulse_fires_once() {
        let mut org = OrganismState::new(0, [500.0, 500.0], 4, 20.0);
        org.heading = 0.0;
        org.prev_audio_energy = 0.0;
        org.audio_energy = 0.2; // rms_delta = 0.2, above 0.05 threshold

        org.apply_audio_impulse();
        let v_after_first = org.velocity[0];
        assert!(v_after_first > 0.0, "impulse should add velocity: {v_after_first}");

        // Call again without updating audio_energy — prev now equals current
        org.apply_audio_impulse();
        assert!(
            (org.velocity[0] - v_after_first).abs() < 0.001,
            "second call should not add more velocity: {} vs {v_after_first}",
            org.velocity[0]
        );
    }

    #[test]
    fn brownian_noise_prevents_zero_velocity() {
        let mut org = OrganismState::new(42, [500.0, 500.0], 4, 20.0);
        org.velocity = [0.0, 0.0];
        org.arousal = 0.0; // zero arousal — wander would be minimal without Brownian
        org.drag = 1.0; // no drag — isolate Brownian effect
        let bounds = [0.0, 0.0, 2000.0, 2000.0];

        // Run one substep — Brownian noise should add non-zero velocity
        org.tick_physics(1.0 / 120.0, bounds);
        let speed = (org.velocity[0] * org.velocity[0] + org.velocity[1] * org.velocity[1]).sqrt();
        assert!(
            speed > 0.1,
            "Brownian noise should prevent zero velocity: speed={}",
            speed
        );
    }

    #[test]
    fn stasis_burst_breaks_equilibrium() {
        let mut org = OrganismState::new(0, [500.0, 500.0], 4, 20.0);
        org.velocity = [0.0, 0.0];
        org.arousal = 0.3;
        org.drag = 0.95;
        let bounds = [0.0, 0.0, 2000.0, 2000.0];

        // Tick 2 seconds at 120Hz — stasis burst should fire within 1s
        for _ in 0..240 {
            org.tick_physics(1.0 / 120.0, bounds);
        }

        let speed =
            (org.velocity[0] * org.velocity[0] + org.velocity[1] * org.velocity[1]).sqrt();
        assert!(
            speed > 5.0,
            "organism should break free of stasis via burst: speed={speed}"
        );
    }

    #[test]
    fn forces_apply_once_not_per_substep() {
        let mut org = OrganismState::new(0, [500.0, 500.0], 4, 20.0);
        org.drag = 1.0; // no drag
        org.max_speed = 10000.0;

        // Apply a known force once
        org.apply_force([100.0, 0.0]);
        let v_after_force = org.velocity[0];

        // Run 2 substeps — velocity should NOT double from force
        // (force was applied once; only drag/wander/integration run per substep)
        let bounds = [0.0, 0.0, 2000.0, 2000.0];
        org.tick_physics(1.0 / 120.0, bounds);
        org.tick_physics(1.0 / 120.0, bounds);

        // Velocity should be close to v_after_force plus small wander thrust,
        // NOT 2× or 3× v_after_force.
        assert!(
            org.velocity[0] < v_after_force * 1.5,
            "force should not multiply with substeps: v={}, expected ~{v_after_force}",
            org.velocity[0]
        );
    }
}
