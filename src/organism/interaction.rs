#![allow(dead_code)]
/// Interaction physics between organisms.
///
/// Six interaction modes operate per neighboring organism pair each frame.
/// Multiple rules can match simultaneously and apply additively (except
/// IntegratePropose which fires a single CPU event).
///
/// Tag matching:
/// - Exact match on species
/// - Wildcard "*" matches all
/// - Match on any entry in affinity_tags
/// - Optional affinity_threshold check against runtime affinity

use super::sim::OrganismState;

/// Force vector result from an interaction.
#[derive(Debug, Clone, Copy)]
pub struct InteractionForce {
    pub force_a: [f32; 2],
    pub force_b: [f32; 2],
}

impl InteractionForce {
    pub fn zero() -> Self {
        Self {
            force_a: [0.0, 0.0],
            force_b: [0.0, 0.0],
        }
    }
}

/// Parameters for the Attach interaction mode.
#[derive(Debug, Clone)]
pub struct AttachParams {
    pub rest_length: f32,
    pub spring_k: f32,
    pub break_distance: f32,
    pub break_force: f32,
}

impl Default for AttachParams {
    fn default() -> Self {
        Self {
            rest_length: 80.0,
            spring_k: 5.0,
            break_distance: 200.0,
            break_force: 100.0,
        }
    }
}

/// Parameters for the Glob interaction mode.
#[derive(Debug, Clone)]
pub struct GlobParams {
    pub attraction_range: f32,
    pub attraction_strength: f32,
    pub viscosity: f32,
    pub centroid_pull: f32,
}

impl Default for GlobParams {
    fn default() -> Self {
        Self {
            attraction_range: 150.0,
            attraction_strength: 3.0,
            viscosity: 0.8,
            centroid_pull: 2.0,
        }
    }
}

// ============================================================================
// Utility
// ============================================================================

fn dist_between(a: &OrganismState, b: &OrganismState) -> f32 {
    let dx = b.position[0] - a.position[0];
    let dy = b.position[1] - a.position[1];
    (dx * dx + dy * dy).sqrt()
}

/// Public distance accessor for use in registry dwell timer checks.
pub fn dist_between_pub(a: &OrganismState, b: &OrganismState) -> f32 {
    dist_between(a, b)
}

fn direction(from: &OrganismState, to: &OrganismState) -> [f32; 2] {
    let dx = to.position[0] - from.position[0];
    let dy = to.position[1] - from.position[1];
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    [dx / len, dy / len]
}

// ============================================================================
// Interaction modes
// ============================================================================

/// Repel: outward force scaled by (1 - surface_dist/range)^2.
///
/// Uses surface-to-surface distance (center dist minus both visual radii)
/// so that `range` means "gap between visual surfaces", not center-to-center.
pub fn repel(a: &OrganismState, b: &OrganismState, range: f32, strength: f32) -> InteractionForce {
    let center_dist = dist_between(a, b);
    if center_dist < 0.001 {
        return InteractionForce::zero();
    }
    let surface_dist = (center_dist - a.visual_radius() - b.visual_radius()).max(0.0);
    if surface_dist >= range {
        return InteractionForce::zero();
    }

    let t = 1.0 - surface_dist / range;
    let magnitude = strength * t * t;
    let dir = direction(a, b);

    InteractionForce {
        force_a: [-dir[0] * magnitude, -dir[1] * magnitude],
        force_b: [dir[0] * magnitude, dir[1] * magnitude],
    }
}

/// Attract: inward force scaled by (1 - surface_dist/range)^2.
///
/// Mirror of repel() — pulls organisms together. Uses surface-to-surface distance.
pub fn attract(a: &OrganismState, b: &OrganismState, range: f32, strength: f32) -> InteractionForce {
    let center_dist = dist_between(a, b);
    if center_dist < 0.001 {
        return InteractionForce::zero();
    }
    let surface_dist = (center_dist - a.visual_radius() - b.visual_radius()).max(0.0);
    if surface_dist >= range {
        return InteractionForce::zero();
    }

    let t = 1.0 - surface_dist / range;
    let magnitude = strength * t * t;
    let dir = direction(a, b);

    InteractionForce {
        force_a: [dir[0] * magnitude, dir[1] * magnitude],     // toward b
        force_b: [-dir[0] * magnitude, -dir[1] * magnitude],   // toward a
    }
}

/// Bounce: repel + velocity projection onto collision normal + friction.
pub fn bounce(
    a: &OrganismState,
    b: &OrganismState,
    range: f32,
    strength: f32,
    friction: f32,
) -> InteractionForce {
    let dist = dist_between(a, b);
    if dist >= range || dist < 0.001 {
        return InteractionForce::zero();
    }

    // Base repulsion
    let repel_force = repel(a, b, range, strength);
    let dir = direction(a, b);

    // Velocity projection — reflect the component along collision normal
    let rel_vel = [
        a.velocity[0] - b.velocity[0],
        a.velocity[1] - b.velocity[1],
    ];
    let vel_dot_normal = rel_vel[0] * dir[0] + rel_vel[1] * dir[1];

    // Only bounce if approaching
    if vel_dot_normal >= 0.0 {
        return repel_force;
    }

    let bounce_impulse = -vel_dot_normal * (1.0 + friction);

    InteractionForce {
        force_a: [
            repel_force.force_a[0] - dir[0] * bounce_impulse * 0.5,
            repel_force.force_a[1] - dir[1] * bounce_impulse * 0.5,
        ],
        force_b: [
            repel_force.force_b[0] + dir[0] * bounce_impulse * 0.5,
            repel_force.force_b[1] + dir[1] * bounce_impulse * 0.5,
        ],
    }
}

/// Slow: viscous drag on relative velocity proportional to overlap.
pub fn slow(
    a: &OrganismState,
    b: &OrganismState,
    range: f32,
    viscosity: f32,
) -> InteractionForce {
    let dist = dist_between(a, b);
    if dist >= range {
        return InteractionForce::zero();
    }

    let overlap = 1.0 - dist / range;
    let rel_vel = [
        a.velocity[0] - b.velocity[0],
        a.velocity[1] - b.velocity[1],
    ];
    let drag = viscosity * overlap;

    InteractionForce {
        force_a: [-rel_vel[0] * drag, -rel_vel[1] * drag],
        force_b: [rel_vel[0] * drag, rel_vel[1] * drag],
    }
}

/// Attach: spring force toward rest_length.
///
/// Returns the force and whether the tether should break.
pub fn attach(
    a: &OrganismState,
    b: &OrganismState,
    params: &AttachParams,
) -> (InteractionForce, bool) {
    let dist = dist_between(a, b);
    let dir = direction(a, b);

    // Spring displacement from rest length
    let displacement = dist - params.rest_length;
    let force_magnitude = displacement * params.spring_k;

    // Check break conditions
    let should_break =
        dist > params.break_distance || force_magnitude.abs() > params.break_force;

    let force = InteractionForce {
        force_a: [dir[0] * force_magnitude, dir[1] * force_magnitude],
        force_b: [-dir[0] * force_magnitude, -dir[1] * force_magnitude],
    };

    (force, should_break)
}

/// Glob: mid-band attraction + viscosity + centroid pull.
///
/// When organisms share a glob group, they are pulled toward the group centroid
/// with viscous damping to prevent oscillation.
pub fn glob(
    a: &OrganismState,
    b: &OrganismState,
    params: &GlobParams,
    centroid: [f32; 2],
) -> InteractionForce {
    let dist = dist_between(a, b);
    if dist >= params.attraction_range {
        return InteractionForce::zero();
    }

    // Mid-band attraction: strongest at half the range
    let t = dist / params.attraction_range;
    let attraction = params.attraction_strength * t * (1.0 - t) * 4.0;
    let dir = direction(a, b);

    // Mutual attraction
    let mut force_a = [dir[0] * attraction, dir[1] * attraction];
    let mut force_b = [-dir[0] * attraction, -dir[1] * attraction];

    // Centroid pull — both organisms pulled toward group centroid
    let to_centroid_a = [
        centroid[0] - a.position[0],
        centroid[1] - a.position[1],
    ];
    let to_centroid_b = [
        centroid[0] - b.position[0],
        centroid[1] - b.position[1],
    ];

    force_a[0] += to_centroid_a[0] * params.centroid_pull;
    force_a[1] += to_centroid_a[1] * params.centroid_pull;
    force_b[0] += to_centroid_b[0] * params.centroid_pull;
    force_b[1] += to_centroid_b[1] * params.centroid_pull;

    // Viscous drag on relative velocity
    let rel_vel = [
        a.velocity[0] - b.velocity[0],
        a.velocity[1] - b.velocity[1],
    ];
    force_a[0] -= rel_vel[0] * params.viscosity;
    force_a[1] -= rel_vel[1] * params.viscosity;
    force_b[0] += rel_vel[0] * params.viscosity;
    force_b[1] += rel_vel[1] * params.viscosity;

    InteractionForce { force_a, force_b }
}

/// Orbit: tangential force perpendicular to the line between organisms.
///
/// Creates circular orbiting motion. Force is strongest at mid-range
/// (half the interaction range) and fades at close/far distances.
/// Both organisms get equal tangential push in the same rotational direction.
pub fn orbit(
    a: &OrganismState,
    b: &OrganismState,
    range: f32,
    strength: f32,
) -> InteractionForce {
    let dist = dist_between(a, b);
    if dist >= range || dist < 0.001 {
        return InteractionForce::zero();
    }

    // Bell curve: strongest at mid-range, zero at edges
    let t = dist / range;
    let magnitude = strength * t * (1.0 - t) * 4.0;

    let dir = direction(a, b);
    // Perpendicular (CCW): rotate dir by 90 degrees
    let perp = [-dir[1], dir[0]];

    InteractionForce {
        force_a: [perp[0] * magnitude, perp[1] * magnitude],
        force_b: [perp[0] * magnitude, perp[1] * magnitude],
    }
}

/// Continuous pull: smooth attachment-driven attraction + relative damping.
///
/// Replaces binary glob physics. Pull strength is quadratic in attachment
/// for snappy lock-in. Orbit range compresses as attachment increases.
/// Relative velocity damping prevents oscillation at high attachment.
pub fn continuous_pull(
    a: &OrganismState,
    b: &OrganismState,
    attachment: f32,
    base_orbit_range: f32,
    max_pull: f32,
    desire_avg: f32,
) -> InteractionForce {
    if attachment < 0.01 {
        return InteractionForce::zero();
    }

    let center_dist = dist_between(a, b);
    if center_dist < 0.001 {
        return InteractionForce::zero();
    }

    // Orbit range compression: tighter orbits at higher attachment
    let orbit_range = base_orbit_range * (1.0 - attachment * 0.6);
    let surface_dist = (center_dist - a.visual_radius() - b.visual_radius()).max(0.0);
    if surface_dist >= orbit_range {
        return InteractionForce::zero();
    }

    // Quadratic pull: attachment² × max_pull × desire
    let pull_strength = attachment * attachment * max_pull * desire_avg;
    let t = 1.0 - surface_dist / orbit_range;
    let magnitude = pull_strength * t;
    let dir = direction(a, b);

    let mut force_a = [dir[0] * magnitude, dir[1] * magnitude];
    let mut force_b = [-dir[0] * magnitude, -dir[1] * magnitude];

    // Relative velocity damping: prevents oscillation at high attachment
    let damping = attachment * 0.3;
    let rel_vel = [
        a.velocity[0] - b.velocity[0],
        a.velocity[1] - b.velocity[1],
    ];
    force_a[0] -= rel_vel[0] * damping;
    force_a[1] -= rel_vel[1] * damping;
    force_b[0] += rel_vel[0] * damping;
    force_b[1] += rel_vel[1] * damping;

    InteractionForce { force_a, force_b }
}

/// IntegratePropose: accumulate dwell timer.
///
/// Returns the accumulated dwell time. When this exceeds `dwell_threshold`,
/// the caller should fire a fusion event (if both organisms consent).
pub fn integrate_propose_tick(
    a: &mut OrganismState,
    b_id: u32,
    dist: f32,
    range: f32,
    dt: f32,
) -> f32 {
    if dist > range {
        // Reset timer if out of range
        a.integrate_timers.remove(&b_id);
        return 0.0;
    }

    let timer = a.integrate_timers.entry(b_id).or_insert(0.0);
    *timer += dt;
    *timer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org_at(id: u32, x: f32, y: f32) -> OrganismState {
        OrganismState::new(id, [x, y], 4, 20.0)
    }

    #[test]
    fn repel_produces_outward_force() {
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 130.0, 100.0);
        let f = repel(&a, &b, 100.0, 10.0);

        // A should be pushed left (negative x)
        assert!(f.force_a[0] < 0.0, "a force_x={}", f.force_a[0]);
        // B should be pushed right (positive x)
        assert!(f.force_b[0] > 0.0, "b force_x={}", f.force_b[0]);
    }

    #[test]
    fn repel_zero_outside_range() {
        // visual_radius = 20 * 12 = 240 each, so surface gap opens at center_dist > 480
        // Place organisms 600px apart → surface_dist = 600 - 240 - 240 = 120, range = 50 → outside
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 700.0, 100.0);
        let f = repel(&a, &b, 50.0, 10.0);

        assert_eq!(f.force_a[0], 0.0);
        assert_eq!(f.force_b[0], 0.0);
    }

    #[test]
    fn attach_creates_spring_toward_rest_length() {
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 200.0, 100.0);
        let params = AttachParams {
            rest_length: 80.0,
            spring_k: 5.0,
            break_distance: 300.0,
            break_force: 500.0,
        };

        let (f, should_break) = attach(&a, &b, &params);

        // Distance (100) > rest_length (80), so A is pulled toward B
        assert!(f.force_a[0] > 0.0, "a should be pulled right: {}", f.force_a[0]);
        assert!(f.force_b[0] < 0.0, "b should be pulled left: {}", f.force_b[0]);
        assert!(!should_break);
    }

    #[test]
    fn attach_breaks_at_distance() {
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 400.0, 100.0);
        let params = AttachParams {
            rest_length: 80.0,
            spring_k: 5.0,
            break_distance: 200.0,
            break_force: 5000.0,
        };

        let (_, should_break) = attach(&a, &b, &params);
        assert!(should_break, "tether should break at distance 300 > 200");
    }

    #[test]
    fn glob_produces_centroid_pull() {
        let a = org_at(0, 50.0, 100.0);
        let b = org_at(1, 150.0, 100.0);
        let centroid = [100.0, 100.0];
        let params = GlobParams::default();

        let f = glob(&a, &b, &params, centroid);

        // A is left of centroid, should be pulled right
        assert!(f.force_a[0] > 0.0, "a force_x={}", f.force_a[0]);
        // B is right of centroid, should be pulled left
        assert!(f.force_b[0] < 0.0, "b force_x={}", f.force_b[0]);
    }

    #[test]
    fn integrate_propose_accumulates_timer() {
        let mut a = org_at(0, 100.0, 100.0);
        let b_id = 1;

        let t1 = integrate_propose_tick(&mut a, b_id, 50.0, 100.0, 0.016);
        assert!(t1 > 0.0);

        let t2 = integrate_propose_tick(&mut a, b_id, 50.0, 100.0, 0.016);
        assert!(t2 > t1, "timer should accumulate: {} > {}", t2, t1);
    }

    #[test]
    fn integrate_propose_resets_outside_range() {
        let mut a = org_at(0, 100.0, 100.0);
        let b_id = 1;

        // Accumulate some time
        integrate_propose_tick(&mut a, b_id, 50.0, 100.0, 1.0);
        assert!(a.integrate_timers.contains_key(&b_id));

        // Move out of range
        let t = integrate_propose_tick(&mut a, b_id, 150.0, 100.0, 0.016);
        assert_eq!(t, 0.0);
        assert!(!a.integrate_timers.contains_key(&b_id));
    }

    #[test]
    fn slow_produces_drag_on_relative_velocity() {
        let mut a = org_at(0, 100.0, 100.0);
        a.velocity = [50.0, 0.0];
        let mut b = org_at(1, 130.0, 100.0);
        b.velocity = [0.0, 0.0];

        let f = slow(&a, &b, 100.0, 5.0);

        // A is moving right relative to B, drag should push A left
        assert!(f.force_a[0] < 0.0, "drag should oppose A's velocity: {}", f.force_a[0]);
        // And push B right (equalize velocities)
        assert!(f.force_b[0] > 0.0, "drag should push B toward A's velocity: {}", f.force_b[0]);
    }

    #[test]
    fn bounce_reflects_approaching_velocity() {
        let mut a = org_at(0, 100.0, 100.0);
        a.velocity = [50.0, 0.0]; // moving right toward B
        let b = org_at(1, 130.0, 100.0); // B is to the right

        let f = bounce(&a, &b, 100.0, 10.0, 0.5);

        // A should get a leftward (negative) impulse
        assert!(f.force_a[0] < 0.0, "bounce should push A away: {}", f.force_a[0]);
    }

    #[test]
    fn attract_pulls_organisms_together() {
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 130.0, 100.0);
        let f = attract(&a, &b, 500.0, 10.0);

        // A should be pulled right (toward B)
        assert!(f.force_a[0] > 0.0, "a should be pulled right: {}", f.force_a[0]);
        // B should be pulled left (toward A)
        assert!(f.force_b[0] < 0.0, "b should be pulled left: {}", f.force_b[0]);
    }

    #[test]
    fn attract_zero_outside_range() {
        // visual_radius = 20 * 12 = 240 each, surface gap opens at center_dist > 480
        let a = org_at(0, 100.0, 100.0);
        let b = org_at(1, 700.0, 100.0); // surface_dist = 600 - 480 = 120, range = 50
        let f = attract(&a, &b, 50.0, 10.0);

        assert_eq!(f.force_a[0], 0.0);
        assert_eq!(f.force_b[0], 0.0);
    }
}
