use rand::Rng;

use super::dna::OrganismDna;
use crate::dsp::cell::{cell_type_ranges, find_range};

/// Mutate an organism's DNA in-place.
///
/// With probability `rate`, each numeric parameter is perturbed by up to
/// +/-10% of its current absolute value. Cell params are clamped to valid
/// ranges defined in each cell's PARAM_RANGES. Non-numeric fields (strings,
/// bools) are left unchanged. The mutation never produces NaN or infinity.
pub fn mutate(dna: &mut OrganismDna, rng: &mut impl Rng, rate: f32) {
    let rate = rate.clamp(0.0, 1.0);

    // Mutate cell params (clamped to valid ranges)
    for cell in &mut dna.cells {
        let ranges = cell_type_ranges(&cell.cell_type);
        let keys: Vec<String> = cell.params.keys().cloned().collect();
        for key in keys {
            if rng.gen::<f32>() < rate {
                if let Some(val) = cell.params.get_mut(&key) {
                    *val = perturb(*val, rng);
                    // Clamp to cell type's PARAM_RANGES if available
                    if let Some((min, max)) = find_range(ranges, &key) {
                        *val = val.clamp(min, max);
                    }
                }
            }
        }
    }

    // Mutate body params
    if rng.gen::<f32>() < rate {
        dna.body.core_radius = perturb(dna.body.core_radius, rng).max(1.0);
    }
    if rng.gen::<f32>() < rate {
        dna.body.pseudopod_gain = perturb(dna.body.pseudopod_gain, rng).clamp(0.0, 2.0);
    }
    if rng.gen::<f32>() < rate {
        dna.body.extension_speed = perturb(dna.body.extension_speed, rng).max(0.1);
    }
    if rng.gen::<f32>() < rate {
        dna.body.retraction_speed = perturb(dna.body.retraction_speed, rng).max(0.1);
    }

    // Mutate render params
    if rng.gen::<f32>() < rate {
        dna.render.smin_k = perturb(dna.render.smin_k, rng).clamp(0.01, 2.0);
    }
    if rng.gen::<f32>() < rate {
        dna.render.edge_softness = perturb(dna.render.edge_softness, rng).clamp(0.1, 10.0);
    }
    if rng.gen::<f32>() < rate {
        dna.render.glow = perturb(dna.render.glow, rng).clamp(0.0, 1.0);
    }
    if rng.gen::<f32>() < rate {
        dna.render.hue = perturb(dna.render.hue, rng).clamp(0.0, 1.0);
    }
    if rng.gen::<f32>() < rate {
        dna.render.pulse_response = perturb(dna.render.pulse_response, rng).clamp(0.0, 1.0);
    }

    // Mutate physics params
    if rng.gen::<f32>() < rate {
        dna.physics.mass = perturb(dna.physics.mass, rng).max(0.01);
    }
    if rng.gen::<f32>() < rate {
        dna.physics.drag = perturb(dna.physics.drag, rng).clamp(0.0, 1.0);
    }
    if rng.gen::<f32>() < rate {
        dna.physics.max_speed = perturb(dna.physics.max_speed, rng).max(1.0);
    }

    // Mutate emotion params
    if rng.gen::<f32>() < rate {
        dna.emotion.base_valence = perturb(dna.emotion.base_valence, rng).clamp(-1.0, 1.0);
    }
    if rng.gen::<f32>() < rate {
        dna.emotion.base_arousal = perturb(dna.emotion.base_arousal, rng).clamp(0.0, 1.0);
    }
}

/// Perturb a value by +/-10% of its absolute value.
/// Handles zero by using a small fixed offset.
/// Never returns NaN or infinity.
fn perturb(val: f32, rng: &mut impl Rng) -> f32 {
    let magnitude = val.abs().max(0.01); // minimum perturbation scale
    let delta = magnitude * 0.1 * (rng.gen::<f32>() * 2.0 - 1.0);
    let result = val + delta;
    if result.is_nan() || result.is_infinite() {
        val // safety: return original
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism::dna::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use std::collections::BTreeMap;

    fn make_test_dna() -> OrganismDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), 180.0);
        params.insert("det".into(), 10.0);
        params.insert("gain".into(), 0.3);

        let mut string_params = BTreeMap::new();
        string_params.insert("wtype".into(), "soft_saw".into());

        OrganismDna {
            name: "mut-test".into(),
            species: "test".into(),
            active: true,
            seed: 42,
            version: 4,
            cells: vec![CellDna {
                cell_type: "osc_cell".into(),
                params,
                string_params,
            }],
            cell_wiring: vec![],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.5,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
        }
    }

    #[test]
    fn mutation_changes_values() {
        let original = make_test_dna();
        let mut dna = original.clone();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(123);
        mutate(&mut dna, &mut rng, 1.0); // rate=1.0 ensures all params mutate

        // At least some cell params should have changed
        let orig_freq = original.cells[0].params.get("freq").unwrap();
        let new_freq = dna.cells[0].params.get("freq").unwrap();
        let orig_det = original.cells[0].params.get("det").unwrap();
        let new_det = dna.cells[0].params.get("det").unwrap();

        let any_changed = (orig_freq - new_freq).abs() > 0.001
            || (orig_det - new_det).abs() > 0.001;
        assert!(any_changed, "Mutation at rate=1.0 should change some values");
    }

    #[test]
    fn mutation_never_produces_nan_or_inf() {
        let mut dna = make_test_dna();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(456);

        // Run many mutations
        for _ in 0..100 {
            mutate(&mut dna, &mut rng, 1.0);

            // Check all cell params
            for cell in &dna.cells {
                for (name, val) in &cell.params {
                    assert!(
                        !val.is_nan() && !val.is_infinite(),
                        "Param {} = {} is NaN/inf after mutation",
                        name,
                        val
                    );
                }
            }

            // Check body/render/physics
            assert!(!dna.body.core_radius.is_nan());
            assert!(!dna.render.glow.is_nan());
            assert!(!dna.physics.mass.is_nan());
            assert!(!dna.emotion.base_valence.is_nan());
        }
    }

    #[test]
    fn mutation_respects_param_ranges() {
        let mut dna = make_test_dna();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(321);

        // After many mutations, params should stay within registered ranges
        for _ in 0..200 {
            mutate(&mut dna, &mut rng, 1.0);
        }

        let freq = *dna.cells[0].params.get("freq").unwrap();
        assert!(
            freq >= 20.0 && freq <= 20000.0,
            "freq should stay in [20, 20000] after bounded mutation, got {freq}"
        );

        let det = *dna.cells[0].params.get("det").unwrap();
        assert!(
            det >= 0.0 && det <= 100.0,
            "det should stay in [0, 100] after bounded mutation, got {det}"
        );

        let gain = *dna.cells[0].params.get("gain").unwrap();
        assert!(
            gain >= 0.0 && gain <= 1.0,
            "gain should stay in [0, 1] after bounded mutation, got {gain}"
        );
    }

    #[test]
    fn mutation_rate_zero_changes_nothing() {
        let original = make_test_dna();
        let mut dna = original.clone();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(789);
        mutate(&mut dna, &mut rng, 0.0);

        for key in original.cells[0].params.keys() {
            let orig = original.cells[0].params.get(key).unwrap();
            let mutated = dna.cells[0].params.get(key).unwrap();
            assert!(
                (orig - mutated).abs() < f32::EPSILON,
                "Rate=0 should not change params: {} was {} now {}",
                key,
                orig,
                mutated,
            );
        }
    }
}
