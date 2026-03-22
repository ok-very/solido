#![allow(dead_code)]
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
            // None: Western-only mode — no microtonal overlay, no raga filtering
            RagaMode {
                name: "none".to_string(),
                tuning: "none".to_string(),
                aroha: vec![],
                avaroha: vec![],
                vadi: 0,
                samvadi: 0,
                gravity_weights: vec![1.0; 12], // Chromatic — all PCs equal
                hue: 180.0, // neutral gray-blue
            },
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

use crate::dsp::command::MAX_MICRO_DEGREES;
use crate::tuning::scala::TuningSystem;

/// Base weight multiplier for vadi (most important) degree.
pub const VADI_BOOST_BASE: f32 = 1.5;
/// Base weight multiplier for samvadi (second most important) degree.
pub const SAMVADI_BOOST_BASE: f32 = 1.3;
/// Additional vadi boost per unit arousal [0,1].
pub const VADI_AROUSAL_SCALE: f32 = 0.5;
/// Weight multiplier for non-aroha/avaroha degrees when direction is active.
pub const NON_PATH_REDUCTION: f32 = 0.2;

/// Convert a RagaMode + TuningSystem → fixed-size cents+weights arrays for audio thread.
///
/// Maps each raga degree to its exact cents position from the .scl tuning,
/// applies vadi/samvadi boosts. Returns (cents, weights, count).
pub fn raga_to_micro_tuning(
    raga: &RagaMode,
    tuning: &TuningSystem,
    vadi_boost: f32,
    samvadi_boost: f32,
) -> ([f32; MAX_MICRO_DEGREES], [f32; MAX_MICRO_DEGREES], u8) {
    let mut cents = [0.0f32; MAX_MICRO_DEGREES];
    let mut weights = [0.0f32; MAX_MICRO_DEGREES];

    // tuning.cents includes root at index 0 and period at last index.
    // raga.gravity_weights.len() == tuning.cents.len() (validated at registry creation).
    // Skip the last entry (period endpoint — same pitch as root).
    let degree_count = (tuning.cents.len() - 1).min(MAX_MICRO_DEGREES);

    for i in 0..degree_count {
        cents[i] = tuning.cents[i] as f32;
        let w = if i < raga.gravity_weights.len() {
            raga.gravity_weights[i]
        } else {
            1.0
        };

        // Apply vadi/samvadi boosts
        let boosted = if i == raga.vadi {
            w * vadi_boost
        } else if i == raga.samvadi {
            w * samvadi_boost
        } else {
            w
        };
        weights[i] = boosted;
    }

    (cents, weights, degree_count as u8)
}

/// Apply aroha/avaroha soft preference to micro weights based on melodic direction.
/// Non-path degrees get weight × NON_PATH_REDUCTION (still reachable, weakly).
pub fn apply_direction_preference(
    weights: &mut [f32; MAX_MICRO_DEGREES],
    count: u8,
    raga: &RagaMode,
    ascending: bool,
) {
    let path = if ascending { &raga.aroha } else { &raga.avaroha };
    for i in 0..count as usize {
        if !path.contains(&i) {
            weights[i] *= NON_PATH_REDUCTION;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::TuningRegistry;

    #[test]
    fn registry_has_6_entries() {
        let reg = RagaRegistry::new();
        assert_eq!(reg.len(), 6); // none + 5 ragas
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
            if raga.name == "none" { continue; } // No tuning file for "none"
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

    #[test]
    fn raga_to_micro_bhairav() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        let raga = raga_reg.get("bhairav").unwrap();
        let tuning = tuning_reg.get("bhairav").unwrap();
        let (cents, weights, count) = raga_to_micro_tuning(raga, tuning, VADI_BOOST_BASE, SAMVADI_BOOST_BASE);

        // Bhairav has 7 degrees (Sa Re Ga Ma Pa Dha Ni) — 8 entries in .scl but last is period
        assert_eq!(count, 7);
        // Sa = 0
        assert!((cents[0] - 0.0).abs() < 0.01);
        // komal Re = 112
        assert!((cents[1] - 112.0).abs() < 1.0, "komal Re should be ~112, got {}", cents[1]);
        // Ga = 386 (vadi, index 2) — should have boosted weight
        assert!((cents[2] - 386.0).abs() < 1.0, "Ga should be ~386, got {}", cents[2]);
        assert!(weights[2] > weights[1], "vadi Ga should have higher weight than Re");
        // Ma = 498
        assert!((cents[3] - 498.0).abs() < 1.0);
        // Pa = 702
        assert!((cents[4] - 702.0).abs() < 1.0);
        // dha = 814 (samvadi, index 5)
        assert!((cents[5] - 814.0).abs() < 1.0);
        assert!(weights[5] > weights[6], "samvadi dha should have higher weight than Ni");
        // Ni = 1088
        assert!((cents[6] - 1088.0).abs() < 1.0);
    }

    #[test]
    fn raga_to_micro_yaman() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        let raga = raga_reg.get("yaman").unwrap();
        let tuning = tuning_reg.get("yaman").unwrap();
        let (cents, _weights, count) = raga_to_micro_tuning(raga, tuning, VADI_BOOST_BASE, SAMVADI_BOOST_BASE);

        assert_eq!(count, 7);
        // Yaman has tivra Ma — should be between 500 and 600 cents
        let tivra_ma = cents[3]; // Ma is degree 3
        assert!(tivra_ma > 500.0 && tivra_ma < 650.0,
            "tivra Ma should be ~590 cents, got {}", tivra_ma);
    }

    #[test]
    fn raga_to_micro_jog() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        let raga = raga_reg.get("jog").unwrap();
        let tuning = tuning_reg.get("jog").unwrap();
        let (_cents, _weights, count) = raga_to_micro_tuning(raga, tuning, VADI_BOOST_BASE, SAMVADI_BOOST_BASE);

        // Jog has 6 degrees (skips Ga) — 7 entries in .scl but last is period
        assert_eq!(count, 6);
    }

    #[test]
    fn vadi_boost_applied() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        let raga = raga_reg.get("bhairav").unwrap();
        let tuning = tuning_reg.get("bhairav").unwrap();

        let (_, weights_boosted, _) = raga_to_micro_tuning(raga, tuning, 2.0, 1.0);
        let (_, weights_base, _) = raga_to_micro_tuning(raga, tuning, 1.0, 1.0);

        // Vadi (Ga, index 2) should be boosted 2x
        assert!(
            (weights_boosted[2] - weights_base[2] * 2.0).abs() < 0.01,
            "vadi weight should be 2x base"
        );
    }

    #[test]
    fn aroha_soft_preference() {
        let raga_reg = RagaRegistry::new();
        let mut tuning_reg = TuningRegistry::new();
        tuning_reg.load_builtins();

        let raga = raga_reg.get("bhairav").unwrap();
        let tuning = tuning_reg.get("bhairav").unwrap();
        let (_, mut weights, count) = raga_to_micro_tuning(raga, tuning, 1.0, 1.0);
        let original_w0 = weights[0];

        // For bhairav, all degrees are in aroha, so ascending reduces nothing.
        // But we can test the function mechanics: create a raga where aroha skips degree 1
        let mut fake_raga = raga.clone();
        fake_raga.aroha = vec![0, 2, 3, 4, 5, 6]; // skip degree 1

        apply_direction_preference(&mut weights, count, &fake_raga, true);
        assert!(
            (weights[0] - original_w0).abs() < 0.01,
            "degree 0 (in aroha) should keep weight"
        );
        assert!(
            weights[1] < original_w0 * 0.5,
            "degree 1 (not in aroha) should be reduced"
        );
    }
}
