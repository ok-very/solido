/// Gain staging constants — single source of truth for the audio pipeline.
///
/// Headroom budget (1 drone organism + VoicePool):
///   Per-organism cell output: normalized to [-1, 1] (via tanh soft-clip)
///   Drone channel gain: 0.6
///   ACID channel gain: 0.5 (squelchy resonance peaks need extra headroom)
///   VoicePool channel gain: 0.8
///   Master gain: 0.5 → comfortable headroom

/// Drone organism channel gain on VoiceBus.
pub const DRON_GAIN: f32 = 0.6;

/// ACID organism channel gain. Lower than DRON because the diode filter's
/// resonance and accent envelope produce high transient peaks that need headroom
/// even though the filter output is tanh-bounded to [-1, 1].
pub const ACID_GAIN: f32 = 0.5;

/// Fallback gain for unknown species.
pub const DEFAULT_ORG_GAIN: f32 = 0.6;

/// Master bus gain applied after channel strip sum.
pub const MASTER_GAIN: f32 = 0.5;

/// Look up the default channel gain for an organism by species name.
pub fn species_gain(name: &str) -> f32 {
    let upper = name.to_uppercase();
    if upper.contains("DRON") {
        DRON_GAIN
    } else if upper.contains("ACID") {
        ACID_GAIN
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
}
