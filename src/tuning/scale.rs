/// Western scale definitions — 12-class chromatic gravity weights for pitch gravity.
/// Expanded from Laurie Spiegel's Music Mouse modes to full western vocabulary.

/// A Western scale mode definition.
#[derive(Clone, Debug)]
pub struct ScaleMode {
    pub name: &'static str,
    /// Short label for chip buttons (3-4 chars).
    pub short: &'static str,
    /// Group for UI organization.
    pub group: &'static str,
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
        //                             C    C#   D    D#   E    F    F#   G    G#   A    A#   B
        let scales = vec![
            // ── Existing 6 (indices 0-5, backward compat) ───────────────────
            ScaleMode {
                name: "Chromatic",     short: "Chr", group: "Other",
                gravity_weights:      [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                hue: 180.0,
            },
            ScaleMode {
                name: "Diatonic",      short: "MAJ", group: "Major/Minor",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0],
                hue: 60.0,
            },
            ScaleMode {
                name: "Pentatonic",    short: "PnM", group: "Other",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 0.0, 1.5, 0.0, 0.0, 1.8, 0.0, 1.2, 0.0, 0.0],
                hue: 300.0,
            },
            ScaleMode {
                name: "Octatonic",     short: "Dim", group: "Symmetric",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [1.5, 1.0, 0.0, 1.0, 1.5, 0.0, 1.0, 1.5, 0.0, 1.0, 1.5, 0.0],
                hue: 270.0,
            },
            ScaleMode {
                name: "Middle Eastern", short: "MdE", group: "Other",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 1.2, 0.0, 0.0, 1.5, 1.0, 0.0, 1.8, 1.2, 0.0, 0.0, 1.0],
                hue: 30.0,
            },
            ScaleMode {
                name: "Quartal",       short: "Qrt", group: "Other",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [1.8, 0.0, 0.0, 1.2, 0.0, 1.5, 0.0, 0.0, 1.2, 0.0, 1.5, 0.0],
                hue: 210.0,
            },
            // ── Major/Minor family ──────────────────────────────────────────
            ScaleMode {
                name: "Natural Minor", short: "min", group: "Major/Minor",
                //                     C    C#   D    Eb   E    F    F#   G    Ab   A    Bb   B
                gravity_weights:      [2.0, 0.0, 1.2, 1.5, 0.0, 1.0, 0.0, 1.8, 1.2, 0.0, 1.0, 0.0],
                hue: 220.0,
            },
            ScaleMode {
                name: "Harmonic Minor", short: "Hmi", group: "Major/Minor",
                //                     C    C#   D    Eb   E    F    F#   G    Ab   A    Bb   B
                gravity_weights:      [2.0, 0.0, 1.2, 1.5, 0.0, 1.0, 0.0, 1.8, 1.2, 0.0, 0.0, 1.3],
                hue: 240.0,
            },
            ScaleMode {
                name: "Melodic Minor", short: "Mmi", group: "Major/Minor",
                //                     C    C#   D    Eb   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 1.5, 0.0, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0],
                hue: 200.0,
            },
            // ── Modes ───────────────────────────────────────────────────────
            ScaleMode {
                name: "Dorian",        short: "Dor", group: "Modes",
                //                     C    C#   D    Eb   E    F    F#   G    G#   A    Bb   B
                gravity_weights:      [2.0, 0.0, 1.2, 1.5, 0.0, 1.0, 0.0, 1.8, 0.0, 1.3, 1.0, 0.0],
                hue: 120.0,
            },
            ScaleMode {
                name: "Phrygian",      short: "Phr", group: "Modes",
                //                     C    Db   D    Eb   E    F    F#   G    Ab   A    Bb   B
                gravity_weights:      [2.0, 1.3, 0.0, 1.5, 0.0, 1.0, 0.0, 1.8, 1.2, 0.0, 1.0, 0.0],
                hue: 340.0,
            },
            ScaleMode {
                name: "Lydian",        short: "Lyd", group: "Modes",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 0.0, 1.5, 0.0, 1.3, 1.8, 0.0, 1.2, 0.0, 1.0],
                hue: 80.0,
            },
            ScaleMode {
                name: "Mixolydian",    short: "Mix", group: "Modes",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    Bb   B
                gravity_weights:      [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 1.0, 0.0],
                hue: 100.0,
            },
            ScaleMode {
                name: "Locrian",       short: "Loc", group: "Modes",
                //                     C    Db   D    Eb   E    F    Gb   G    Ab   A    Bb   B
                gravity_weights:      [2.0, 1.3, 0.0, 1.5, 0.0, 1.0, 1.5, 0.0, 1.2, 0.0, 1.0, 0.0],
                hue: 320.0,
            },
            // ── Other ───────────────────────────────────────────────────────
            ScaleMode {
                name: "Blues",         short: "Blu", group: "Other",
                //                     C    C#   D    Eb   E    F    Gb   G    G#   A    Bb   B
                gravity_weights:      [2.0, 0.0, 0.0, 1.5, 0.0, 1.0, 1.0, 1.8, 0.0, 0.0, 1.2, 0.0],
                hue: 20.0,
            },
            ScaleMode {
                name: "Pentatonic Minor", short: "Pnm", group: "Other",
                //                     C    C#   D    Eb   E    F    F#   G    G#   A    Bb   B
                gravity_weights:      [2.0, 0.0, 0.0, 1.5, 0.0, 1.0, 0.0, 1.8, 0.0, 0.0, 1.2, 0.0],
                hue: 280.0,
            },
            ScaleMode {
                name: "Hungarian Minor", short: "Hun", group: "Other",
                //                     C    C#   D    Eb   E    F    F#   G    Ab   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 1.5, 0.0, 0.0, 1.3, 1.8, 1.2, 0.0, 0.0, 1.0],
                hue: 40.0,
            },
            // ── Symmetric ───────────────────────────────────────────────────
            ScaleMode {
                name: "Whole Tone",    short: "WTn", group: "Symmetric",
                //                     C    C#   D    D#   E    F    F#   G    G#   A    A#   B
                gravity_weights:      [2.0, 0.0, 1.2, 0.0, 1.2, 0.0, 1.2, 0.0, 1.2, 0.0, 1.2, 0.0],
                hue: 160.0,
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

    pub fn all(&self) -> &[ScaleMode] {
        &self.scales
    }

    /// Unique group names in display order.
    pub fn groups(&self) -> Vec<&'static str> {
        let order = ["Major/Minor", "Modes", "Other", "Symmetric"];
        let mut out = Vec::new();
        for &g in &order {
            if self.scales.iter().any(|s| s.group == g) {
                out.push(g);
            }
        }
        out
    }

    /// Scales in a group, as (index, &ScaleMode) pairs.
    pub fn scales_in_group(&self, group: &str) -> Vec<(usize, &ScaleMode)> {
        self.scales.iter().enumerate()
            .filter(|(_, s)| s.group == group)
            .collect()
    }

    /// Find index by name.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.scales.iter().position(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_18_scales() {
        let reg = ScaleRegistry::new();
        assert_eq!(reg.len(), 18);
    }

    #[test]
    fn registry_lookup_by_name() {
        let reg = ScaleRegistry::new();
        // Original 6
        assert!(reg.get("Chromatic").is_some());
        assert!(reg.get("Diatonic").is_some());
        assert!(reg.get("Pentatonic").is_some());
        assert!(reg.get("Octatonic").is_some());
        assert!(reg.get("Middle Eastern").is_some());
        assert!(reg.get("Quartal").is_some());
        // New scales
        assert!(reg.get("Natural Minor").is_some());
        assert!(reg.get("Harmonic Minor").is_some());
        assert!(reg.get("Melodic Minor").is_some());
        assert!(reg.get("Dorian").is_some());
        assert!(reg.get("Phrygian").is_some());
        assert!(reg.get("Lydian").is_some());
        assert!(reg.get("Mixolydian").is_some());
        assert!(reg.get("Locrian").is_some());
        assert!(reg.get("Blues").is_some());
        assert!(reg.get("Pentatonic Minor").is_some());
        assert!(reg.get("Hungarian Minor").is_some());
        assert!(reg.get("Whole Tone").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_lookup_by_index() {
        let reg = ScaleRegistry::new();
        // Original indices preserved
        assert_eq!(reg.get_by_index(0).unwrap().name, "Chromatic");
        assert_eq!(reg.get_by_index(1).unwrap().name, "Diatonic");
        assert_eq!(reg.get_by_index(5).unwrap().name, "Quartal");
        assert!(reg.get_by_index(18).is_none());
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
    fn active_degree_counts() {
        let reg = ScaleRegistry::new();
        let expected = [
            ("Chromatic", 12), ("Diatonic", 7), ("Pentatonic", 5), ("Octatonic", 8),
            ("Natural Minor", 7), ("Harmonic Minor", 7), ("Melodic Minor", 7),
            ("Dorian", 7), ("Phrygian", 7), ("Lydian", 7), ("Mixolydian", 7), ("Locrian", 7),
            ("Blues", 6), ("Pentatonic Minor", 5), ("Hungarian Minor", 7), ("Whole Tone", 6),
        ];
        for (name, count) in expected {
            let s = reg.get(name).unwrap_or_else(|| panic!("missing scale: {}", name));
            let active = s.gravity_weights.iter().filter(|w| **w > 0.1).count();
            assert_eq!(active, count, "{} should have {} active degrees, got {}", name, count, active);
        }
    }

    #[test]
    fn all_shorts_unique() {
        let reg = ScaleRegistry::new();
        let shorts: Vec<&str> = reg.scales.iter().map(|s| s.short).collect();
        for (i, a) in shorts.iter().enumerate() {
            for (j, b) in shorts.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "duplicate short label: {}", a);
                }
            }
        }
    }

    #[test]
    fn groups_returns_ordered() {
        let reg = ScaleRegistry::new();
        let groups = reg.groups();
        assert_eq!(groups, vec!["Major/Minor", "Modes", "Other", "Symmetric"]);
    }

    #[test]
    fn scales_in_group_counts() {
        let reg = ScaleRegistry::new();
        assert_eq!(reg.scales_in_group("Major/Minor").len(), 4); // Diatonic, NatMin, HarmMin, MelMin
        assert_eq!(reg.scales_in_group("Modes").len(), 5);       // Dor, Phr, Lyd, Mix, Loc
        assert!(reg.scales_in_group("Other").len() >= 6);        // Chr, Pen, MdE, Qrt, Blu, Pnm, Hun
        assert_eq!(reg.scales_in_group("Symmetric").len(), 2);   // Oct, WTn
    }

    #[test]
    fn hues_are_in_range() {
        let reg = ScaleRegistry::new();
        for scale in &reg.scales {
            assert!(scale.hue >= 0.0 && scale.hue < 360.0, "{} hue out of range: {}", scale.name, scale.hue);
        }
    }

    #[test]
    fn list_returns_all_names() {
        let reg = ScaleRegistry::new();
        let names = reg.list();
        assert_eq!(names.len(), 18);
        assert_eq!(names[0], "Chromatic");
    }

    #[test]
    fn index_of_finds_scales() {
        let reg = ScaleRegistry::new();
        assert_eq!(reg.index_of("Chromatic"), Some(0));
        assert_eq!(reg.index_of("Diatonic"), Some(1));
        assert_eq!(reg.index_of("Blues"), Some(14));
        assert_eq!(reg.index_of("nonexistent"), None);
    }
}
