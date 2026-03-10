/// Sonar — periodic neighbor detection infrastructure for organisms.
///
/// Organisms don't sense each other continuously. Sonar pings at a fixed rate,
/// detecting all organism pairs within `max_range`. Between pings, organisms
/// act on cached detection results.
///
/// On detection, a gentle "curiosity" attraction pulls organisms toward each
/// other at macro scale. DNA interaction rules (repel, bounce, attach, etc.)
/// then fire at their natural short ranges when organisms are close enough.
///
/// Sonar is infrastructure — it lives at the registry level, not per-organism.
/// All organisms share the same ping rate and detection range.

use super::sim::{OrganismId, OrganismState};

/// A single sonar detection between two organisms.
#[derive(Debug, Clone, Copy)]
pub struct SonarDetection {
    pub org_a: OrganismId,
    pub org_b: OrganismId,
    /// Pixel distance between organism centroids.
    pub distance: f32,
    /// Unit vector from A toward B.
    pub direction: [f32; 2],
}

/// Periodic neighbor detection service.
pub struct Sonar {
    /// Seconds between pings.
    pub ping_interval: f32,
    /// Maximum detection range in pixels.
    pub max_range: f32,
    /// Gentle attraction strength for detected pairs.
    pub curiosity_strength: f32,
    /// Timer accumulator since last ping.
    timer: f32,
    /// Current detections (refreshed each ping).
    detections: Vec<SonarDetection>,
}

impl Sonar {
    pub fn new() -> Self {
        Self {
            ping_interval: 0.15, // ~7 Hz
            max_range: 400.0,
            curiosity_strength: 8.0,
            timer: 0.0,
            detections: Vec::new(),
        }
    }

    /// Advance sonar timer. Fires a ping when interval elapses.
    pub fn tick(&mut self, dt: f32, organisms: &[OrganismState]) {
        self.timer += dt;
        if self.timer >= self.ping_interval {
            self.timer -= self.ping_interval;
            self.ping(organisms);
        }
    }

    /// Instant detection: find all pairs within max_range.
    fn ping(&mut self, organisms: &[OrganismState]) {
        self.detections.clear();
        let n = organisms.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = organisms[j].position[0] - organisms[i].position[0];
                let dy = organisms[j].position[1] - organisms[i].position[1];
                let dist_sq = dx * dx + dy * dy;
                let max_sq = self.max_range * self.max_range;
                if dist_sq < max_sq && dist_sq > 0.001 {
                    let dist = dist_sq.sqrt();
                    let inv = 1.0 / dist;
                    self.detections.push(SonarDetection {
                        org_a: organisms[i].id,
                        org_b: organisms[j].id,
                        distance: dist,
                        direction: [dx * inv, dy * inv],
                    });
                }
            }
        }
    }

    /// Current detection results (valid until next ping).
    pub fn detections(&self) -> &[SonarDetection] {
        &self.detections
    }

    /// Compute curiosity forces for all detected pairs.
    ///
    /// Returns (org_id, force_x, force_y) triples. Caller accumulates into
    /// organism velocity. Force falls off quadratically with distance.
    ///
    /// `audio_energies` maps organism IDs to their current audio energy [0,1].
    /// Active (loud) organisms are more interesting — silent ones attract at 30%.
    pub fn curiosity_forces(
        &self,
        audio_energies: &std::collections::HashMap<OrganismId, f32>,
    ) -> Vec<(OrganismId, [f32; 2])> {
        let mut forces = Vec::with_capacity(self.detections.len() * 2);
        for det in &self.detections {
            let proximity = 1.0 - det.distance / self.max_range;

            // Audio-weighted interest: loud neighbors are more attractive
            let b_energy = audio_energies.get(&det.org_b).copied().unwrap_or(0.0);
            let a_energy = audio_energies.get(&det.org_a).copied().unwrap_or(0.0);
            let interest_a_toward_b = 0.3 + b_energy * 0.7; // A's interest in B
            let interest_b_toward_a = 0.3 + a_energy * 0.7; // B's interest in A

            // A pulled toward B
            let mag_a = self.curiosity_strength * proximity * proximity * interest_a_toward_b;
            forces.push((det.org_a, [det.direction[0] * mag_a, det.direction[1] * mag_a]));

            // B pulled toward A
            let mag_b = self.curiosity_strength * proximity * proximity * interest_b_toward_a;
            forces.push((det.org_b, [-det.direction[0] * mag_b, -det.direction[1] * mag_b]));
        }
        forces
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism::sim::OrganismState;

    #[test]
    fn ping_detects_close_pair() {
        let mut sonar = Sonar::new();
        sonar.max_range = 200.0;
        let organisms = vec![
            OrganismState::new(0, [100.0, 100.0], 4, 20.0),
            OrganismState::new(1, [200.0, 100.0], 4, 20.0),
        ];
        sonar.tick(0.2, &organisms); // triggers ping
        assert_eq!(sonar.detections().len(), 1);
        let det = &sonar.detections()[0];
        assert!((det.distance - 100.0).abs() < 0.1);
        assert!(det.direction[0] > 0.9); // pointing right
    }

    #[test]
    fn no_detection_beyond_range() {
        let mut sonar = Sonar::new();
        sonar.max_range = 50.0;
        let organisms = vec![
            OrganismState::new(0, [0.0, 0.0], 4, 20.0),
            OrganismState::new(1, [200.0, 0.0], 4, 20.0),
        ];
        sonar.tick(0.2, &organisms);
        assert!(sonar.detections().is_empty());
    }

    #[test]
    fn no_ping_before_interval() {
        let mut sonar = Sonar::new();
        sonar.ping_interval = 1.0;
        sonar.max_range = 500.0;
        let organisms = vec![
            OrganismState::new(0, [0.0, 0.0], 4, 20.0),
            OrganismState::new(1, [100.0, 0.0], 4, 20.0),
        ];
        sonar.tick(0.5, &organisms); // half interval — no ping yet
        assert!(sonar.detections().is_empty());
        sonar.tick(0.6, &organisms); // past interval — ping fires
        assert_eq!(sonar.detections().len(), 1);
    }

    #[test]
    fn curiosity_forces_attract() {
        use std::collections::HashMap;
        let mut sonar = Sonar::new();
        sonar.max_range = 500.0;
        sonar.curiosity_strength = 10.0;
        let organisms = vec![
            OrganismState::new(0, [0.0, 0.0], 4, 20.0),
            OrganismState::new(1, [100.0, 0.0], 4, 20.0),
        ];
        sonar.tick(0.2, &organisms);
        let audio: HashMap<OrganismId, f32> = HashMap::new();
        let forces = sonar.curiosity_forces(&audio);
        // org 0 should be pulled right (toward org 1)
        let f0 = forces.iter().find(|(id, _)| *id == 0).unwrap();
        assert!(f0.1[0] > 0.0, "org 0 should be pulled right");
        // org 1 should be pulled left (toward org 0)
        let f1 = forces.iter().find(|(id, _)| *id == 1).unwrap();
        assert!(f1.1[0] < 0.0, "org 1 should be pulled left");
    }
}
