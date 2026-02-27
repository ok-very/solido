/// Gain staging constants — single source of truth for the audio pipeline.
///
/// Headroom budget (3 organisms + VoicePool):
///   Per-organism cell output: normalized to [-1, 1] (via tanh soft-clip)
///   Per-species channel gain: TBLK=0.7, DRON=0.4, MELO=0.6
///   VoicePool channel gain: 0.8
///   Worst-case simultaneous sum: 0.7 + 0.4 + 0.6 + 0.8 = 2.5
///   Master gain: 0.4 → worst-case peak ≈ 1.0
///   MasterBus limiter catches transients above 1.0

/// VoicePool channel gain on VoiceBus.
pub const VOICE_POOL_GAIN: f32 = 0.8;

/// Per-species organism channel gains on VoiceBus.
pub const TBLK_GAIN: f32 = 0.7;
pub const DRON_GAIN: f32 = 0.4;
pub const MELO_GAIN: f32 = 0.6;

/// Fallback gain for unknown species.
pub const DEFAULT_ORG_GAIN: f32 = 0.6;

/// Master bus gain applied after channel strip sum.
pub const MASTER_GAIN: f32 = 0.4;

/// Look up the default channel gain for an organism by species name.
pub fn species_gain(name: &str) -> f32 {
    let upper = name.to_uppercase();
    if upper.contains("TBLK") {
        TBLK_GAIN
    } else if upper.contains("DRON") {
        DRON_GAIN
    } else if upper.contains("MELO") {
        MELO_GAIN
    } else {
        DEFAULT_ORG_GAIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_gain_lookup() {
        assert!((species_gain("tblk-alpha") - TBLK_GAIN).abs() < 0.001);
        assert!((species_gain("DRON-bass") - DRON_GAIN).abs() < 0.001);
        assert!((species_gain("melo-lead") - MELO_GAIN).abs() < 0.001);
        assert!((species_gain("unknown") - DEFAULT_ORG_GAIN).abs() < 0.001);
    }

    #[test]
    fn headroom_budget_within_unity() {
        let worst_case = TBLK_GAIN + DRON_GAIN + MELO_GAIN + VOICE_POOL_GAIN;
        let final_peak = worst_case * MASTER_GAIN;
        assert!(
            final_peak <= 1.1,
            "Worst-case peak {final_peak} should be near unity"
        );
    }
}
