/// Solido key abstraction, decoupled from egui for headless support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SolidoKey {
    // Note keys (1-7 → scale degrees)
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    // Navigation
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    // Actions
    Space,
    R,
    T,
    P,
    D,
    E,
    S,
    L,
    Escape,
    // Panel toggles
    F1,
    F2,
    F3,
    // BPM nudge
    OpenBracket,
    CloseBracket,
}

impl SolidoKey {
    /// Map number keys 1-7 to a normalized pitch value [0.0, 1.0].
    /// Returns `None` for non-number keys.
    pub fn pitch_value(&self) -> Option<f32> {
        match self {
            SolidoKey::Num1 => Some(0.0),
            SolidoKey::Num2 => Some(1.0 / 6.0),
            SolidoKey::Num3 => Some(2.0 / 6.0),
            SolidoKey::Num4 => Some(3.0 / 6.0),
            SolidoKey::Num5 => Some(4.0 / 6.0),
            SolidoKey::Num6 => Some(5.0 / 6.0),
            SolidoKey::Num7 => Some(1.0),
            _ => None,
        }
    }

    /// Gravity delta for arrow up/down keys.
    /// Up = +0.1 (increase gravity), Down = -0.1 (decrease gravity).
    pub fn gravity_delta(&self) -> Option<f32> {
        match self {
            SolidoKey::ArrowUp => Some(0.1),
            SolidoKey::ArrowDown => Some(-0.1),
            _ => None,
        }
    }

    /// Map number keys 1-7 to a note number (0-6).
    /// Returns `None` for non-number keys.
    pub fn note_number(&self) -> Option<u8> {
        match self {
            SolidoKey::Num1 => Some(0),
            SolidoKey::Num2 => Some(1),
            SolidoKey::Num3 => Some(2),
            SolidoKey::Num4 => Some(3),
            SolidoKey::Num5 => Some(4),
            SolidoKey::Num6 => Some(5),
            SolidoKey::Num7 => Some(6),
            _ => None,
        }
    }

    /// Tempo delta for arrow left/right keys.
    /// Right = +5.0 (speed up), Left = -5.0 (slow down).
    pub fn tempo_delta(&self) -> Option<f32> {
        match self {
            SolidoKey::ArrowRight => Some(5.0),
            SolidoKey::ArrowLeft => Some(-5.0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_values_cover_range() {
        assert_eq!(SolidoKey::Num1.pitch_value(), Some(0.0));
        assert_eq!(SolidoKey::Num7.pitch_value(), Some(1.0));
        // Middle values are evenly spaced
        let v4 = SolidoKey::Num4.pitch_value().unwrap();
        assert!((v4 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn non_note_keys_return_none() {
        assert_eq!(SolidoKey::Space.pitch_value(), None);
        assert_eq!(SolidoKey::ArrowUp.pitch_value(), None);
        assert_eq!(SolidoKey::Escape.pitch_value(), None);
    }

    #[test]
    fn gravity_delta_values() {
        assert_eq!(SolidoKey::ArrowUp.gravity_delta(), Some(0.1));
        assert_eq!(SolidoKey::ArrowDown.gravity_delta(), Some(-0.1));
        assert_eq!(SolidoKey::Num1.gravity_delta(), None);
    }

    #[test]
    fn note_numbers() {
        assert_eq!(SolidoKey::Num1.note_number(), Some(0));
        assert_eq!(SolidoKey::Num4.note_number(), Some(3));
        assert_eq!(SolidoKey::Num7.note_number(), Some(6));
        assert_eq!(SolidoKey::Space.note_number(), None);
        assert_eq!(SolidoKey::ArrowUp.note_number(), None);
    }

    #[test]
    fn tempo_delta_values() {
        assert_eq!(SolidoKey::ArrowRight.tempo_delta(), Some(5.0));
        assert_eq!(SolidoKey::ArrowLeft.tempo_delta(), Some(-5.0));
        assert_eq!(SolidoKey::Space.tempo_delta(), None);
    }

    #[test]
    fn new_variants_return_none() {
        for key in [SolidoKey::D, SolidoKey::E, SolidoKey::S, SolidoKey::L, SolidoKey::F1, SolidoKey::F2, SolidoKey::F3, SolidoKey::OpenBracket, SolidoKey::CloseBracket] {
            assert_eq!(key.pitch_value(), None, "{:?} should have no pitch", key);
            assert_eq!(key.gravity_delta(), None, "{:?} should have no gravity", key);
            assert_eq!(key.note_number(), None, "{:?} should have no note", key);
            assert_eq!(key.tempo_delta(), None, "{:?} should have no tempo", key);
        }
    }
}
