/// Raga definitions — mode structures with gravity weights, aroha/avaroha,
/// vadi/samvadi, and HSV hue for visual coloring.

/// A raga mode definition.
#[derive(Clone, Debug)]
pub struct RagaMode {
    pub name: String,
    /// Name of the tuning system (key into TuningRegistry).
    pub tuning: String,
    /// Ascending scale degree indices (0-based, into tuning cents).
    pub aroha: Vec<usize>,
    /// Descending scale degree indices.
    pub avaroha: Vec<usize>,
    /// Vadi (most important) degree index.
    pub vadi: usize,
    /// Samvadi (second most important) degree index.
    pub samvadi: usize,
    /// Per-degree gravity weights. Length must match tuning's cents.len().
    /// Higher = stronger pull toward that degree.
    pub gravity_weights: Vec<f32>,
    /// HSV hue [0, 360) for visual coloring of this raga.
    pub hue: f32,
}

/// Tracks melodic direction (ascending/descending) with hysteresis.
pub struct DirectionTracker {
    last_cents: f64,
    /// true = ascending, false = descending
    pub direction: bool,
    /// Minimum cents change before direction flips.
    pub hysteresis: f64,
}

impl DirectionTracker {
    pub fn new(hysteresis: f64) -> Self {
        Self {
            last_cents: 0.0,
            direction: true,
            hysteresis,
        }
    }

    /// Update with a new cents value. Flips direction only when
    /// the change exceeds the hysteresis threshold.
    pub fn update(&mut self, cents: f64) {
        let delta = cents - self.last_cents;
        if delta.abs() > self.hysteresis {
            self.direction = delta > 0.0;
            self.last_cents = cents;
        }
    }

    pub fn is_ascending(&self) -> bool {
        self.direction
    }
}

/// Registry of built-in raga definitions.
pub struct RagaRegistry {
    ragas: Vec<RagaMode>,
}

impl RagaRegistry {
    pub fn new() -> Self {
        let ragas = vec![
            // Bhairav: Sa re Ga Ma Pa dha Ni Sa
            // komal re, shuddha Ga, komal dha — morning raga, serious mood
            RagaMode {
                name: "bhairav".to_string(),
                tuning: "bhairav".to_string(),
                aroha: vec![0, 1, 2, 3, 4, 5, 6],
                avaroha: vec![6, 5, 4, 3, 2, 1, 0],
                vadi: 2,     // Ga
                samvadi: 5,  // dha
                // 8 weights: Sa Re Ga Ma Pa Dha Ni Sa'(period)
                gravity_weights: vec![1.5, 0.8, 2.0, 1.2, 1.5, 1.8, 1.0, 1.5],
                hue: 30.0, // warm orange
            },
            // Bhairavi: Sa re ga Ma Pa dha ni Sa
            // All komal — versatile, emotional, often concluding raga
            RagaMode {
                name: "bhairavi".to_string(),
                tuning: "bhairavi".to_string(),
                aroha: vec![0, 1, 2, 3, 4, 5, 6],
                avaroha: vec![6, 5, 4, 3, 2, 1, 0],
                vadi: 3,     // Ma
                samvadi: 0,  // Sa
                gravity_weights: vec![1.5, 1.0, 1.2, 2.0, 1.5, 1.0, 1.2, 1.5],
                hue: 0.0, // red
            },
            // Yaman: Sa Re Ga Ma# Pa Dha Ni Sa
            // tivra Ma — evening raga, serene and romantic
            RagaMode {
                name: "yaman".to_string(),
                tuning: "yaman".to_string(),
                aroha: vec![0, 1, 2, 3, 4, 5, 6],
                avaroha: vec![6, 5, 4, 3, 2, 1, 0],
                vadi: 2,     // Ga
                samvadi: 5,  // Dha (Ni in some traditions)
                gravity_weights: vec![1.5, 1.2, 2.0, 1.5, 1.5, 1.8, 1.2, 1.5],
                hue: 240.0, // blue
            },
            // Jog: Sa Re Ma Pa Dha Ni Sa (skips Ga)
            // Ambiguous, weak gravity, good for texture
            RagaMode {
                name: "jog".to_string(),
                tuning: "jog".to_string(),
                aroha: vec![0, 1, 2, 3, 4, 5],
                avaroha: vec![5, 4, 3, 2, 1, 0],
                vadi: 2,     // Ma
                samvadi: 4,  // Dha
                // 7 weights: jog.scl has 6 degrees + root = 7 cents entries
                gravity_weights: vec![1.5, 1.0, 1.8, 1.5, 1.2, 1.0, 1.5],
                hue: 160.0, // teal
            },
            // Kafi: Sa Re ga Ma Pa Dha ni Sa
            // komal Ga, komal Ni — playful, romantic
            RagaMode {
                name: "kafi".to_string(),
                tuning: "kafi".to_string(),
                aroha: vec![0, 1, 2, 3, 4, 5, 6],
                avaroha: vec![6, 5, 4, 3, 2, 1, 0],
                vadi: 4,     // Pa
                samvadi: 1,  // Re
                gravity_weights: vec![1.5, 1.5, 1.2, 1.5, 2.0, 1.2, 1.0, 1.5],
                hue: 120.0, // green
            },
        ];
        Self { ragas }
    }

    pub fn get(&self, name: &str) -> Option<&RagaMode> {
        self.ragas.iter().find(|r| r.name == name)
    }

    pub fn list(&self) -> Vec<&str> {
        self.ragas.iter().map(|r| r.name.as_str()).collect()
    }

    pub fn get_by_index(&self, index: usize) -> Option<&RagaMode> {
        self.ragas.get(index)
    }

    pub fn len(&self) -> usize {
        self.ragas.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::TuningRegistry;

    #[test]
    fn registry_has_5_ragas() {
        let reg = RagaRegistry::new();
        assert_eq!(reg.len(), 5);
    }

    #[test]
    fn registry_lookup() {
        let reg = RagaRegistry::new();
        assert!(reg.get("bhairav").is_some());
        assert!(reg.get("yaman").is_some());
        assert!(reg.get("jog").is_some());
        assert!(reg.get("kafi").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn weights_length_matches_tuning() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        for raga in &raga_reg.ragas {
            let tuning = tuning_reg.get(&raga.tuning).unwrap_or_else(|| {
                panic!("raga '{}' references unknown tuning '{}'", raga.name, raga.tuning)
            });
            assert_eq!(
                raga.gravity_weights.len(),
                tuning.cents.len(),
                "raga '{}' weights ({}) should match tuning '{}' cents ({})",
                raga.name,
                raga.gravity_weights.len(),
                raga.tuning,
                tuning.cents.len()
            );
        }
    }

    #[test]
    fn direction_hysteresis() {
        let mut tracker = DirectionTracker::new(50.0);
        tracker.update(100.0); // initial move up
        assert!(tracker.is_ascending());

        tracker.update(80.0); // small dip — within hysteresis
        assert!(tracker.is_ascending(), "should not flip within hysteresis");

        tracker.update(30.0); // large drop — exceeds hysteresis
        assert!(!tracker.is_ascending(), "should flip on large change");
    }

    #[test]
    fn direction_initial_state() {
        let tracker = DirectionTracker::new(50.0);
        assert!(tracker.is_ascending()); // default ascending
    }

    #[test]
    fn jog_has_7_weights() {
        let reg = RagaRegistry::new();
        let jog = reg.get("jog").unwrap();
        assert_eq!(
            jog.gravity_weights.len(),
            7,
            "jog should have 7 weights (6 degrees + root)"
        );
    }
}
