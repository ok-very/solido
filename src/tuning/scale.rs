/// Western scale definitions — 12-class chromatic gravity weights for use with
/// `quantize_to_scale_fast`. Inspired by Laurie Spiegel's Music Mouse modes.

/// A Western scale mode definition.
#[derive(Clone, Debug)]
pub struct ScaleMode {
    pub name: &'static str,
    /// Per-pitch-class gravity weights [C, C#, D, D#, E, F, F#, G, G#, A, A#, B].
    /// Higher = stronger pull. Zero = skip.
    pub gravity_weights: [f32; 12],
    /// HSV hue [0, 360) for visual coloring.
    pub hue: f32,
}

/// Registry of built-in Western scale definitions.
pub struct ScaleRegistry {
    scales: Vec<ScaleMode>,
}

impl ScaleRegistry {
    pub fn new() -> Self {
        let scales = vec![
            ScaleMode {
                name: "Chromatic",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                hue: 180.0, // cyan
            },
            ScaleMode {
                name: "Diatonic",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0],
                hue: 60.0, // yellow
            },
            ScaleMode {
                name: "Pentatonic",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [2.0, 0.0, 1.2, 0.0, 1.5, 0.0, 0.0, 1.8, 0.0, 1.2, 0.0, 0.0],
                hue: 300.0, // magenta
            },
            ScaleMode {
                name: "Octatonic",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [1.5, 1.0, 0.0, 1.0, 1.5, 0.0, 1.0, 1.5, 0.0, 1.0, 1.5, 0.0],
                hue: 270.0, // violet
            },
            ScaleMode {
                name: "Middle Eastern",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [2.0, 1.2, 0.0, 0.0, 1.5, 1.0, 0.0, 1.8, 1.2, 0.0, 0.0, 1.0],
                hue: 30.0, // orange
            },
            ScaleMode {
                name: "Quartal",
                //       C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights: [1.8, 0.0, 0.0, 1.2, 0.0, 1.5, 0.0, 0.0, 1.2, 0.0, 1.5, 0.0],
                hue: 210.0, // sky
            },
        ];
        Self { scales }
    }

    #[allow(dead_code)]
    pub fn get(&self, name: &str) -> Option<&ScaleMode> {
        self.scales.iter().find(|s| s.name == name)
    }

    pub fn get_by_index(&self, index: usize) -> Option<&ScaleMode> {
        self.scales.get(index)
    }

    pub fn list(&self) -> Vec<&str> {
        self.scales.iter().map(|s| s.name).collect()
    }

    pub fn len(&self) -> usize {
        self.scales.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_6_scales() {
        let reg = ScaleRegistry::new();
        assert_eq!(reg.len(), 6);
    }

    #[test]
    fn registry_lookup_by_name() {
        let reg = ScaleRegistry::new();
        assert!(reg.get("Chromatic").is_some());
        assert!(reg.get("Diatonic").is_some());
        assert!(reg.get("Pentatonic").is_some());
        assert!(reg.get("Octatonic").is_some());
        assert!(reg.get("Middle Eastern").is_some());
        assert!(reg.get("Quartal").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_lookup_by_index() {
        let reg = ScaleRegistry::new();
        assert_eq!(reg.get_by_index(0).unwrap().name, "Chromatic");
        assert_eq!(reg.get_by_index(5).unwrap().name, "Quartal");
        assert!(reg.get_by_index(6).is_none());
    }

    #[test]
    fn all_weights_are_12_elements() {
        let reg = ScaleRegistry::new();
        for scale in &reg.scales {
            assert_eq!(scale.gravity_weights.len(), 12, "{} should have 12 weights", scale.name);
        }
    }

    #[test]
    fn chromatic_has_equal_weights() {
        let reg = ScaleRegistry::new();
        let c = reg.get("Chromatic").unwrap();
        for w in &c.gravity_weights {
            assert!((w - 1.0).abs() < 1e-5, "chromatic weights should all be 1.0");
        }
    }

    #[test]
    fn pentatonic_has_5_active_degrees() {
        let reg = ScaleRegistry::new();
        let p = reg.get("Pentatonic").unwrap();
        let active = p.gravity_weights.iter().filter(|w| **w > 0.1).count();
        assert_eq!(active, 5, "pentatonic should have 5 active degrees");
    }

    #[test]
    fn diatonic_has_7_active_degrees() {
        let reg = ScaleRegistry::new();
        let d = reg.get("Diatonic").unwrap();
        let active = d.gravity_weights.iter().filter(|w| **w > 0.1).count();
        assert_eq!(active, 7, "diatonic should have 7 active degrees");
    }

    #[test]
    fn octatonic_has_8_active_degrees() {
        let reg = ScaleRegistry::new();
        let o = reg.get("Octatonic").unwrap();
        let active = o.gravity_weights.iter().filter(|w| **w > 0.1).count();
        assert_eq!(active, 8, "octatonic should have 8 active degrees");
    }

    #[test]
    fn list_returns_all_names() {
        let reg = ScaleRegistry::new();
        let names = reg.list();
        assert_eq!(names.len(), 6);
        assert_eq!(names[0], "Chromatic");
    }

    #[test]
    fn hues_are_in_range() {
        let reg = ScaleRegistry::new();
        for scale in &reg.scales {
            assert!(scale.hue >= 0.0 && scale.hue < 360.0, "{} hue out of range: {}", scale.name, scale.hue);
        }
    }
}
