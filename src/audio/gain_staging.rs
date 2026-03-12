/// Gain staging constants — single source of truth for the audio pipeline.
///
/// Headroom budget (6 organisms summing):
///   Per-organism cell output: normalized to ~±1.0 (frequency-dependent sqrt(N)/C)
///   Per-organism DNA gain: 0.3–0.85 (set in DNA params, applied per-cell)
///   Per-organism peak after soft_clip: typically 0.10–0.20 (osc) or 0.70–0.80 (perc)
///   Species channel gain: 0.70–1.00 (below, applied in VoiceBus)
///   Constant-power pan at center: ×0.707
///   Master gain: 1.0 static, dynamic scales with organism count
///
/// Budget example (6 orgs, center pan):
///   Sum of 6 × ~0.15 avg peak × 0.85 avg species × 0.707 pan × 0.80 master
///   ≈ 0.43 peak (before effects). Limiter catches anything above 1.0.

/// Drone organism channel gain on VoiceBus. Background pad — sits below most.
pub const DRON_GAIN: f32 = 0.85;

/// ACID organism channel gain. Diode filter resonance + accent envelope
/// produce transient peaks, but output is tanh-bounded to ±1.0.
pub const ACID_GAIN: f32 = 0.75;

/// HOSO organism channel gain. Sequenced melodic line — needs presence.
pub const HOSO_GAIN: f32 = 0.90;

/// SPGL organism channel gain. Slow generative texture — background role.
pub const SPGL_GAIN: f32 = 0.70;

/// TBLK organism channel gain. Organic tabla percussion — transient peaks.
pub const TBLK_GAIN: f32 = 0.90;

/// KKIT organism channel gain. Mechanical drum kit — needs punch.
pub const KKIT_GAIN: f32 = 1.00;

/// ISAO organism channel gain. Tomita melodic lead — needs presence through
/// saw_bank → diode_filter chain. Gain chain is inherently quiet (0.5 × 0.6 × 0.8)
/// so species gain must compensate.
pub const ISAO_GAIN: f32 = 0.90;

/// Fallback gain for unknown species.
pub const DEFAULT_ORG_GAIN: f32 = 0.85;

/// Master bus gain applied after channel strip sum.
/// Set to 1.0 — dynamic_master_gain overrides this based on organism count.
pub const MASTER_GAIN: f32 = 1.0;

/// Dynamic master gain based on active organism count.
///
/// Scales inversely with active count to maintain consistent output level.
/// Per-organism peaks are modest (0.10-0.20 for oscillators, 0.70-0.80 for
/// percussion) after frequency-dependent normalization. These values were
/// recalibrated for the corrected oscillator output levels.
pub fn dynamic_master_gain(active_count: usize) -> f32 {
    match active_count {
        0..=2 => 1.0,
        3..=4 => 0.90,
        5..=6 => 0.80,
        _ => 0.70,
    }
}

/// Look up the default channel gain for an organism by species name.
pub fn species_gain(name: &str) -> f32 {
    let upper = name.to_uppercase();
    if upper.contains("DRON") {
        DRON_GAIN
    } else if upper.contains("ACID") {
        ACID_GAIN
    } else if upper.contains("HOSO") {
        HOSO_GAIN
    } else if upper.contains("SPGL") {
        SPGL_GAIN
    } else if upper.contains("TBLK") {
        TBLK_GAIN
    } else if upper.contains("KKIT") {
        KKIT_GAIN
    } else if upper.contains("ISAO") {
        ISAO_GAIN
    } else {
        DEFAULT_ORG_GAIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_gain_lookup() {
        assert!((species_gain("DRON-bass") - DRON_GAIN).abs() < 0.001);
        assert!((species_gain("unknown") - DEFAULT_ORG_GAIN).abs() < 0.001);
    }

    #[test]
    fn gains_are_above_minimum() {
        // After oscillator normalization, species gains must be high enough
        // to produce audible output through the attenuation stack.
        assert!(DRON_GAIN >= 0.7, "DRON gain too low: {}", DRON_GAIN);
        assert!(MASTER_GAIN >= 0.8, "Master gain too low: {}", MASTER_GAIN);
        assert!(dynamic_master_gain(6) >= 0.7, "Dynamic gain for 6 orgs too low");
    }

    #[test]
    fn species_gain_covers_all_seven() {
        let cases = [
            ("dron-alpha", DRON_GAIN),
            ("acid-kinoko", ACID_GAIN),
            ("hoso-malabar", HOSO_GAIN),
            ("spgl-kepler", SPGL_GAIN),
            ("tblk-dha", TBLK_GAIN),
            ("kkit-909", KKIT_GAIN),
            ("isao-tomita", ISAO_GAIN),
        ];
        for (name, expected) in cases {
            let got = species_gain(name);
            assert!(
                (got - expected).abs() < 0.001,
                "species_gain({name}) = {got}, expected {expected}"
            );
            assert!(
                (got - DEFAULT_ORG_GAIN).abs() > 0.001 || expected == DEFAULT_ORG_GAIN,
                "species_gain({name}) fell through to DEFAULT — add a branch"
            );
        }
    }
}
