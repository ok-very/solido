/// Gain staging constants — single source of truth for the audio pipeline.
///
/// Headroom budget (6 organisms summing):
///   Per-organism cell output: normalized to [-1, 1] (via tanh soft-clip)
///   Per-organism channel gain: species-specific (0.45–0.70)
///   Master gain: 0.65 — raised from 0.5 now that all 6 organisms are active
///
/// Budget: worst case sum = 6 × 0.70 × 0.65 = 2.73, but organisms rarely all
/// peak simultaneously. Transient peaks handled by MasterBus limiter.

/// Drone organism channel gain on VoiceBus. Background pad — sits below most.
pub const DRON_GAIN: f32 = 0.6;

/// ACID organism channel gain. Lower than DRON because the diode filter's
/// resonance and accent envelope produce high transient peaks that need headroom
/// even though the filter output is tanh-bounded to [-1, 1].
pub const ACID_GAIN: f32 = 0.5;

/// HOSO organism channel gain. Sequenced melodic line — needs presence.
pub const HOSO_GAIN: f32 = 0.65;

/// SPGL organism channel gain. Slow generative texture — background role.
pub const SPGL_GAIN: f32 = 0.45;

/// TBLK organism channel gain. Organic tabla percussion — transient peaks.
pub const TBLK_GAIN: f32 = 0.65;

/// KKIT organism channel gain. Mechanical drum kit — needs punch.
pub const KKIT_GAIN: f32 = 0.70;

/// Fallback gain for unknown species.
pub const DEFAULT_ORG_GAIN: f32 = 0.6;

/// Master bus gain applied after channel strip sum.
/// Raised from 0.5 to 0.65 now that all six organisms are active.
pub const MASTER_GAIN: f32 = 0.65;

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
    fn species_gain_covers_all_six() {
        let cases = [
            ("dron-alpha", DRON_GAIN),
            ("acid-kinoko", ACID_GAIN),
            ("hoso-malabar", HOSO_GAIN),
            ("spgl-kepler", SPGL_GAIN),
            ("tblk-dha", TBLK_GAIN),
            ("kkit-909", KKIT_GAIN),
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
