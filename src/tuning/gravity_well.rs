#![allow(dead_code)]
/// Gravity Wells — spatial harmonic fields that create localized tonal centers.
///
/// Each well has a position, root pitch class, and radius of influence.
/// Organisms near a well hear the active scale transposed to that well's root.
/// Combined with per-organism `root_pitch_class` (DNA field), this gives every
/// organism a unique harmonic perspective based on spatial position.

/// A spatial harmonic attractor with a position and tonal center.
#[derive(Clone, Debug)]
pub struct GravityWell {
    pub id: u32,
    pub position: [f32; 2],
    pub root_pitch_class: u8, // 0-11 (C=0, A=9, etc.)
    pub radius: f32,          // influence radius in pixels
    pub strength: f32,        // [0, 1]
    pub hue: f32,             // visual color (0-360)
}

/// Collection of gravity wells forming a spatial harmonic field.
pub struct GravityField {
    wells: Vec<GravityWell>,
    next_id: u32,
}

/// Result of computing effective weights for a single organism.
pub struct EffectiveWeights {
    pub weights: [f32; 12],
    pub total_influence: f32,
}

impl GravityField {
    pub fn new() -> Self {
        Self {
            wells: Vec::new(),
            next_id: 0,
        }
    }

    /// Generate wells with roots from the circle of fifths.
    /// Deterministic positions from seed (matches `seeded_spawn_pos` pattern).
    pub fn generate(count: usize, bounds: [f32; 4], seed: u64) -> Self {
        // Circle of fifths: C, G, D, A, E, B — consonant neighboring wells
        let fifths: [u8; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
        // Hues for visual variety
        let hues: [f32; 6] = [0.0, 60.0, 120.0, 200.0, 280.0, 330.0];

        let [min_x, min_y, max_x, max_y] = bounds;
        let margin = 100.0;

        let mut wells = Vec::with_capacity(count);
        for i in 0..count {
            let h1 = seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(0x6c62272e07bb0142);
            let h2 = h1
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(0x6c62272e07bb0142);
            let nx = (h1 >> 16 & 0xFFFF) as f32 / 65535.0;
            let ny = (h2 >> 16 & 0xFFFF) as f32 / 65535.0;

            let x = min_x + margin + nx * (max_x - min_x - 2.0 * margin);
            let y = min_y + margin + ny * (max_y - min_y - 2.0 * margin);

            // Radius: 200-350px based on hash
            let h3 = h2
                .wrapping_mul(0xbf58476d1ce4e5b9)
                .wrapping_add(0x94d049bb133111eb);
            let radius = 200.0 + ((h3 >> 32 & 0xFFFF) as f32 / 65535.0) * 150.0;

            wells.push(GravityWell {
                id: i as u32,
                position: [x, y],
                root_pitch_class: fifths[i % 12],
                radius,
                strength: 0.6,
                hue: hues[i % hues.len()],
            });
        }

        Self {
            wells,
            next_id: count as u32,
        }
    }

    pub fn wells(&self) -> &[GravityWell] {
        &self.wells
    }

    pub fn wells_mut(&mut self) -> &mut [GravityWell] {
        &mut self.wells
    }

    pub fn well_mut(&mut self, id: u32) -> Option<&mut GravityWell> {
        self.wells.iter_mut().find(|w| w.id == id)
    }

    /// Regenerate wells with a new count, preserving deterministic layout.
    pub fn regenerate(&mut self, count: usize, bounds: [f32; 4], seed: u64) {
        *self = GravityField::generate(count, bounds, seed);
    }

    pub fn len(&self) -> usize {
        self.wells.len()
    }

    /// Compute effective weights for an organism at a given position with a root pitch class.
    ///
    /// 1. Transpose base weights to organism's root_pitch_class
    /// 2. For each well within range: quadratic falloff, transpose to well root, blend
    /// 3. Normalize if any weight exceeds 3.0
    pub fn effective_weights(
        &self,
        org_pos: [f32; 2],
        org_root: u8,
        base_weights: &[f32; 12],
    ) -> EffectiveWeights {
        // Start with base weights transposed to organism's root
        let mut weights = transpose_weights(base_weights, org_root);
        let mut total_influence = 0.0f32;

        for well in &self.wells {
            let dx = org_pos[0] - well.position[0];
            let dy = org_pos[1] - well.position[1];
            let dist = (dx * dx + dy * dy).sqrt();

            if dist >= well.radius {
                continue;
            }

            // Quadratic falloff: strength × (1 - (dist/radius)²)
            let norm_dist = dist / well.radius;
            let influence = well.strength * (1.0 - norm_dist * norm_dist);
            total_influence += influence;

            // Transpose base weights to well's root and blend in
            let well_weights = transpose_weights(base_weights, well.root_pitch_class);
            for i in 0..12 {
                weights[i] += influence * well_weights[i];
            }
        }

        // Normalize if any weight exceeds 3.0 (prevents runaway from overlapping wells)
        let max_w = weights.iter().copied().fold(0.0f32, f32::max);
        if max_w > 3.0 {
            let scale = 3.0 / max_w;
            for w in &mut weights {
                *w *= scale;
            }
        }

        EffectiveWeights {
            weights,
            total_influence,
        }
    }
}

/// Map pitch class number (0–11) to note name.
pub fn pitch_class_name(pc: u8) -> &'static str {
    match pc % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => unreachable!(),
    }
}

/// Shift a C-rooted weight array so the root falls on a different pitch class.
///
/// `transpose_weights(c_major, 9)` moves the root weight (index 0) to index 9 (A),
/// so the organism hears A-major intervals.
pub fn transpose_weights(weights: &[f32; 12], root: u8) -> [f32; 12] {
    let shift = root as usize % 12;
    let mut out = [0.0f32; 12];
    for i in 0..12 {
        out[(i + shift) % 12] = weights[i];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diatonic_weights() -> [f32; 12] {
        // C-rooted diatonic: C D E F G A B
        [2.0, 0.0, 1.2, 0.0, 1.5, 1.0, 0.0, 1.8, 0.0, 1.2, 0.0, 1.0]
    }

    #[test]
    fn transpose_identity() {
        let w = diatonic_weights();
        let t = transpose_weights(&w, 0);
        for i in 0..12 {
            assert!((t[i] - w[i]).abs() < 1e-6, "index {}: {} != {}", i, t[i], w[i]);
        }
    }

    #[test]
    fn transpose_to_a() {
        let w = diatonic_weights();
        let t = transpose_weights(&w, 9);
        // Root weight (2.0) should now be at index 9 (A)
        assert!((t[9] - 2.0).abs() < 1e-6, "A should have root weight 2.0, got {}", t[9]);
        // Fifth weight (1.8, was at index 7=G) should now be at (7+9)%12 = 4 (E)
        assert!((t[4] - 1.8).abs() < 1e-6, "E should have fifth weight 1.8, got {}", t[4]);
    }

    #[test]
    fn transpose_preserves_sum() {
        let w = diatonic_weights();
        let sum_orig: f32 = w.iter().sum();
        for root in 0..12u8 {
            let t = transpose_weights(&w, root);
            let sum_t: f32 = t.iter().sum();
            assert!(
                (sum_t - sum_orig).abs() < 1e-5,
                "root {}: sum {} != {}",
                root,
                sum_t,
                sum_orig
            );
        }
    }

    #[test]
    fn generate_creates_correct_count() {
        let field = GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42);
        assert_eq!(field.wells().len(), 3);
    }

    #[test]
    fn generate_wells_within_bounds() {
        let bounds = [0.0, 0.0, 1200.0, 700.0];
        let field = GravityField::generate(5, bounds, 123);
        for well in field.wells() {
            assert!(
                well.position[0] >= 0.0 && well.position[0] <= 1200.0,
                "x out of bounds: {}",
                well.position[0]
            );
            assert!(
                well.position[1] >= 0.0 && well.position[1] <= 700.0,
                "y out of bounds: {}",
                well.position[1]
            );
        }
    }

    #[test]
    fn generate_deterministic() {
        let a = GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42);
        let b = GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42);
        for (wa, wb) in a.wells().iter().zip(b.wells()) {
            assert_eq!(wa.position, wb.position);
            assert_eq!(wa.root_pitch_class, wb.root_pitch_class);
        }
    }

    #[test]
    fn generate_different_seeds_different_positions() {
        let a = GravityField::generate(1, [0.0, 0.0, 1200.0, 700.0], 1);
        let b = GravityField::generate(1, [0.0, 0.0, 1200.0, 700.0], 2);
        assert_ne!(a.wells()[0].position, b.wells()[0].position);
    }

    #[test]
    fn effective_weights_no_wells() {
        let field = GravityField::new();
        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 0, &base);
        // With no wells and root=0, should just be base weights
        for i in 0..12 {
            assert!(
                (result.weights[i] - base[i]).abs() < 1e-6,
                "no wells: index {} differs",
                i
            );
        }
        assert!(result.total_influence < 1e-6);
    }

    #[test]
    fn effective_weights_with_root_offset() {
        let field = GravityField::new();
        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 9, &base);
        // Should be transposed to A
        let expected = transpose_weights(&base, 9);
        for i in 0..12 {
            assert!(
                (result.weights[i] - expected[i]).abs() < 1e-6,
                "root offset: index {} differs",
                i
            );
        }
    }

    #[test]
    fn effective_weights_organism_at_well_center() {
        let mut field = GravityField::new();
        field.wells.push(GravityWell {
            id: 0,
            position: [500.0, 350.0],
            root_pitch_class: 7, // G
            radius: 300.0,
            strength: 1.0,
            hue: 0.0,
        });

        let base = diatonic_weights();
        // Organism at well center, root=0
        let result = field.effective_weights([500.0, 350.0], 0, &base);

        // Should have influence = 1.0 (at center, dist=0, quadratic=1.0×strength)
        assert!(
            (result.total_influence - 1.0).abs() < 1e-5,
            "influence at center should be 1.0, got {}",
            result.total_influence
        );

        // Weights should be base (C-root) + 1.0 × G-transposed weights, then normalized
        let g_weights = transpose_weights(&base, 7);
        let mut expected = [0.0f32; 12];
        for i in 0..12 {
            expected[i] = base[i] + g_weights[i];
        }
        // Apply same normalization as effective_weights
        let max_e = expected.iter().copied().fold(0.0f32, f32::max);
        if max_e > 3.0 {
            let scale = 3.0 / max_e;
            for e in &mut expected {
                *e *= scale;
            }
        }
        for i in 0..12 {
            assert!(
                (result.weights[i] - expected[i]).abs() < 0.1,
                "index {}: expected ~{}, got {}",
                i,
                expected[i],
                result.weights[i]
            );
        }
    }

    #[test]
    fn effective_weights_organism_outside_well() {
        let mut field = GravityField::new();
        field.wells.push(GravityWell {
            id: 0,
            position: [100.0, 100.0],
            root_pitch_class: 7,
            radius: 200.0,
            strength: 1.0,
            hue: 0.0,
        });

        let base = diatonic_weights();
        // Organism far outside well radius
        let result = field.effective_weights([900.0, 900.0], 0, &base);

        // Should be just base weights (no well influence)
        assert!(result.total_influence < 1e-6);
        for i in 0..12 {
            assert!(
                (result.weights[i] - base[i]).abs() < 1e-6,
                "outside well: index {} should be base",
                i
            );
        }
    }

    #[test]
    fn normalization_prevents_runaway() {
        let mut field = GravityField::new();
        // Two overlapping wells at the same position
        for id in 0..3u32 {
            field.wells.push(GravityWell {
                id,
                position: [500.0, 350.0],
                root_pitch_class: 0,
                radius: 300.0,
                strength: 1.0,
                hue: 0.0,
            });
        }

        let base = diatonic_weights();
        let result = field.effective_weights([500.0, 350.0], 0, &base);

        let max_w = result.weights.iter().copied().fold(0.0f32, f32::max);
        assert!(
            max_w <= 3.01,
            "weights should be normalized to max 3.0, got {}",
            max_w
        );
    }

    #[test]
    fn wells_mut_modifies() {
        let mut field = GravityField::generate(2, [0.0, 0.0, 1200.0, 700.0], 42);
        field.wells_mut()[0].strength = 0.0;
        assert!((field.wells()[0].strength - 0.0).abs() < 1e-6);
    }

    #[test]
    fn well_mut_by_id() {
        let mut field = GravityField::generate(3, [0.0, 0.0, 1200.0, 700.0], 42);
        assert!(field.well_mut(1).is_some());
        field.well_mut(1).unwrap().radius = 999.0;
        assert!((field.wells()[1].radius - 999.0).abs() < 1e-6);
        assert!(field.well_mut(99).is_none());
    }

    #[test]
    fn regenerate_changes_count() {
        let mut field = GravityField::generate(2, [0.0, 0.0, 1200.0, 700.0], 42);
        assert_eq!(field.len(), 2);
        field.regenerate(5, [0.0, 0.0, 1200.0, 700.0], 42);
        assert_eq!(field.len(), 5);
    }

    #[test]
    fn pitch_class_name_all() {
        assert_eq!(pitch_class_name(0), "C");
        assert_eq!(pitch_class_name(9), "A");
        assert_eq!(pitch_class_name(11), "B");
        assert_eq!(pitch_class_name(12), "C"); // wraps
    }

    #[test]
    fn well_radius_in_generate_range() {
        let field = GravityField::generate(10, [0.0, 0.0, 2000.0, 1000.0], 99);
        for well in field.wells() {
            assert!(
                well.radius >= 200.0 && well.radius <= 350.0,
                "radius out of range: {}",
                well.radius
            );
        }
    }
}
