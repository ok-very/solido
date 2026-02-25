use crate::affinity::emotion::ModuleEmotion;

/// Emotion-to-visual bridge.
///
/// Maps per-module emotional state to rendering and physics parameters.
/// High valence + low arousal = sharp edges, cool colors, locked raga.
/// Low valence + high arousal = soft glow, hot palette, free drift.
///
/// | Emotion State      | Gravity | Sound                      | Visual                          |
/// |--------------------|---------|----------------------------|---------------------------------|
/// | Calm, positive     | high    | Locked raga, clean tala    | Sharp blob edges, cool colors   |
/// | Slightly aroused   | medium  | Subtle gamaka, slight swing| Softening edges, warming        |
/// | High arousal       | low     | Pitch drifts, rhythm dissolves | Soft glowing blobs, hot palette |
/// | Panic              | zero    | Free spectral drone        | Diffuse glow, white-hot         |
#[derive(Debug, Clone)]
pub struct GravityState {
    /// How strongly pitch is pulled toward scale degrees [0, 1].
    pub pitch_gravity: f32,
    /// How strongly rhythm is pulled toward tala grid [0, 1].
    pub rhythm_gravity: f32,
    /// Ornamental pitch deviation depth [0, 1].
    pub gamaka_depth: f32,
    /// Lobe morphing speed multiplier [0.1, 2.0].
    pub morph_speed: f32,
}

impl GravityState {
    /// Derive gravity state from a module's emotional state.
    ///
    /// Positive valence + low arousal -> high gravity (stable, tonal).
    /// Negative valence + high arousal -> low gravity (chaotic, textural).
    pub fn from_emotion(emotion: &ModuleEmotion) -> Self {
        let base_gravity = emotion.valence * 0.5 + 0.5;
        let arousal_pull = emotion.arousal * 0.6;
        Self {
            pitch_gravity: (base_gravity - arousal_pull).clamp(0.0, 1.0),
            rhythm_gravity: (base_gravity - arousal_pull * 0.5).clamp(0.0, 1.0),
            gamaka_depth: emotion.arousal.clamp(0.0, 1.0),
            morph_speed: (-emotion.valence * 0.5 + 0.5).clamp(0.1, 2.0),
        }
    }

    /// Default gravity state (neutral emotion).
    pub fn neutral() -> Self {
        Self {
            pitch_gravity: 0.5,
            rhythm_gravity: 0.5,
            gamaka_depth: 0.0,
            morph_speed: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calm_positive_yields_high_gravity() {
        let mut emotion = ModuleEmotion::new(5.0);
        emotion.valence = 0.8;
        emotion.arousal = 0.1;
        let gs = GravityState::from_emotion(&emotion);
        assert!(gs.pitch_gravity > 0.7, "pitch_gravity={}", gs.pitch_gravity);
        assert!(gs.rhythm_gravity > 0.7, "rhythm_gravity={}", gs.rhythm_gravity);
        assert!(gs.gamaka_depth < 0.2, "gamaka_depth={}", gs.gamaka_depth);
    }

    #[test]
    fn high_arousal_yields_low_gravity() {
        let mut emotion = ModuleEmotion::new(5.0);
        emotion.valence = -0.5;
        emotion.arousal = 0.9;
        let gs = GravityState::from_emotion(&emotion);
        assert!(gs.pitch_gravity < 0.3, "pitch_gravity={}", gs.pitch_gravity);
        assert!(gs.gamaka_depth > 0.8, "gamaka_depth={}", gs.gamaka_depth);
    }

    #[test]
    fn panic_state_yields_near_zero_gravity() {
        let mut emotion = ModuleEmotion::new(5.0);
        emotion.valence = -1.0;
        emotion.arousal = 1.0;
        let gs = GravityState::from_emotion(&emotion);
        assert!(gs.pitch_gravity == 0.0, "pitch_gravity={}", gs.pitch_gravity);
        assert!(gs.gamaka_depth == 1.0, "gamaka_depth={}", gs.gamaka_depth);
        assert!(gs.morph_speed >= 0.9, "morph_speed={}", gs.morph_speed);
    }

    #[test]
    fn neutral_emotion_yields_moderate_gravity() {
        let emotion = ModuleEmotion::new(5.0);
        let gs = GravityState::from_emotion(&emotion);
        // valence=0, arousal=0 -> base=0.5, pull=0 -> pitch_gravity=0.5
        assert!((gs.pitch_gravity - 0.5).abs() < 0.01, "pitch_gravity={}", gs.pitch_gravity);
        assert!((gs.rhythm_gravity - 0.5).abs() < 0.01, "rhythm_gravity={}", gs.rhythm_gravity);
        assert!(gs.gamaka_depth == 0.0, "gamaka_depth={}", gs.gamaka_depth);
    }

    #[test]
    fn morph_speed_increases_with_negative_valence() {
        let mut positive = ModuleEmotion::new(5.0);
        positive.valence = 0.8;
        let gs_pos = GravityState::from_emotion(&positive);

        let mut negative = ModuleEmotion::new(5.0);
        negative.valence = -0.8;
        let gs_neg = GravityState::from_emotion(&negative);

        assert!(gs_neg.morph_speed > gs_pos.morph_speed,
            "neg morph_speed={} should > pos morph_speed={}",
            gs_neg.morph_speed, gs_pos.morph_speed);
    }

    #[test]
    fn all_values_clamped() {
        // Extreme values should still be in bounds
        let mut emotion = ModuleEmotion::new(5.0);
        emotion.valence = -2.0; // out of normal range
        emotion.arousal = 3.0;  // out of normal range
        let gs = GravityState::from_emotion(&emotion);
        assert!(gs.pitch_gravity >= 0.0 && gs.pitch_gravity <= 1.0);
        assert!(gs.rhythm_gravity >= 0.0 && gs.rhythm_gravity <= 1.0);
        assert!(gs.gamaka_depth >= 0.0 && gs.gamaka_depth <= 1.0);
        assert!(gs.morph_speed >= 0.1 && gs.morph_speed <= 2.0);
    }
}
