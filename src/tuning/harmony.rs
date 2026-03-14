/// Organism-to-organism harmonic interaction via Tenney height.
///
/// Tenney height for a just-intonation ratio p/q = log2(p × q).
/// Lower TH = more consonant. We map each 12-TET interval to its
/// nearest JI ratio and derive consonance via exp(-TENNEY_DECAY × TH).

/// Exponential decay constant for Tenney height → consonance mapping.
const TENNEY_DECAY: f32 = 0.22;

/// Detuning tolerance in cents. Live intervals within this distance
/// of a JI ratio receive consonance; beyond, it fades to zero.
const DETUNE_TOLERANCE: f32 = 30.0;

/// Root consonance weight in the blend (static DNA component).
const ROOT_BLEND: f32 = 0.3;

/// Live consonance weight in the blend (dynamic pitch component).
const LIVE_BLEND: f32 = 0.7;

/// Tenney-derived consonance for each 12-TET semitone interval.
/// Values = exp(-TENNEY_DECAY * log2(p * q)) for nearest JI ratio.
///
/// Ranking: unison(1.0) > 5th(0.57) > 4th(0.45) > M6(0.42) > M3(0.39)
///        > m3(0.34) > m6(0.31) > M2(0.26) > M7(0.22) > m7(0.21)
///        > m2(0.18) > tritone(0.10)
const TENNEY_CONSONANCE: [f32; 12] = [
    1.000, // 0: unison    1/1   TH=0.0
    0.175, // 1: minor 2nd 16/15 TH=7.9
    0.258, // 2: major 2nd  9/8  TH=6.2
    0.339, // 3: minor 3rd  6/5  TH=4.9
    0.386, // 4: major 3rd  5/4  TH=4.3
    0.454, // 5: fourth     4/3  TH=3.6
    0.100, // 6: tritone   45/32 TH=10.5
    0.566, // 7: fifth      3/2  TH=2.6
    0.310, // 8: minor 6th  8/5  TH=5.3
    0.422, // 9: major 6th  5/3  TH=3.9
    0.207, // 10: minor 7th 16/9 TH=7.2
    0.219, // 11: major 7th 15/8 TH=6.9
];

/// JI intervals within one octave: (cents position, consonance value).
/// Sorted by cents for nearest-neighbour lookup.
const JI_INTERVALS: [(f32, f32); 12] = [
    (0.0, 1.000),     // 1/1
    (111.73, 0.175),   // 16/15
    (203.91, 0.258),   // 9/8
    (315.64, 0.339),   // 6/5
    (386.31, 0.386),   // 5/4
    (498.04, 0.454),   // 4/3
    (600.00, 0.100),   // 45/32
    (701.96, 0.566),   // 3/2
    (813.69, 0.310),   // 8/5
    (884.36, 0.422),   // 5/3
    (996.09, 0.207),   // 16/9
    (1088.27, 0.219),  // 15/8
];

/// Per-pair harmonic snapshot, computed each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HarmonicPair {
    /// Static consonance from DNA root_pitch_class.
    pub root_consonance: f32,
    /// Dynamic consonance from live seq_pitch_hz.
    pub live_consonance: f32,
    /// Blended: root * 0.3 + live * 0.7.
    pub consonance: f32,
}

/// Tenney consonance for a 12-TET semitone interval (0-11).
pub fn tenney_consonance_interval(interval: u8) -> f32 {
    TENNEY_CONSONANCE[(interval % 12) as usize]
}

/// Convert Hz to pitch class (0=C, 9=A, etc).
/// Returns None for invalid frequencies.
pub fn hz_to_pitch_class(hz: f32) -> Option<u8> {
    if hz <= 0.0 {
        return None;
    }
    let midi = 12.0 * (hz / 440.0).log2() + 69.0;
    Some((midi.round() as i32).rem_euclid(12) as u8)
}

/// Tenney consonance between two Hz frequencies, with detuning penalty.
///
/// Finds the nearest JI interval and weights consonance by how close the
/// actual interval is to just intonation. Returns 0.0 for invalid inputs.
pub fn tenney_consonance_hz(hz_a: f32, hz_b: f32) -> f32 {
    if hz_a <= 0.0 || hz_b <= 0.0 {
        return 0.0;
    }

    // Compute interval in cents, reduced to [0, 1200)
    let ratio = hz_a / hz_b;
    let cents_raw = (1200.0 * ratio.log2()).abs();
    let cents = cents_raw % 1200.0;

    // Find nearest JI interval across the full octave [0, 1200)
    let mut best_dist = f32::MAX;
    let mut best_consonance = 0.0_f32;

    for &(ji_cents, ji_consonance) in &JI_INTERVALS {
        // Wrap-aware distance (e.g. 1150 cents is 50 cents from unison at 0/1200)
        let d = (cents - ji_cents).abs();
        let dist = d.min(1200.0 - d);
        if dist < best_dist {
            best_dist = dist;
            best_consonance = ji_consonance;
        }
    }

    // Detuning penalty: linear fade from 1.0 at exact JI to 0.0 at DETUNE_TOLERANCE
    let detune_penalty = (1.0 - best_dist / DETUNE_TOLERANCE).max(0.0);
    best_consonance * detune_penalty
}

/// Compute harmonic pair between two organisms.
///
/// - `root_a`, `root_b`: root_pitch_class (0-11) from DNA
/// - `hz_a`, `hz_b`: current seq_pitch_hz (0.0 if not playing)
/// - `scale_affinity_a`, `scale_affinity_b`: from DNA (KKIT = 0.0)
pub fn compute_harmonic_pair(
    root_a: u8,
    root_b: u8,
    hz_a: f32,
    hz_b: f32,
    scale_affinity_a: f32,
    scale_affinity_b: f32,
) -> HarmonicPair {
    // KKIT exemption: either organism with scale_affinity < 0.01
    if scale_affinity_a < 0.01 || scale_affinity_b < 0.01 {
        return HarmonicPair::default();
    }

    // Static consonance from DNA root pitch classes (shortest interval)
    let d = ((root_a as i8) - (root_b as i8)).unsigned_abs() % 12;
    let interval = d.min(12 - d);
    let root_consonance = tenney_consonance_interval(interval);

    // Dynamic consonance from live pitch
    let have_live = hz_a > 0.0 && hz_b > 0.0;
    let live_consonance = if have_live {
        tenney_consonance_hz(hz_a, hz_b)
    } else {
        0.0
    };

    // Blend: if both organisms are playing, blend static + dynamic.
    // Otherwise, use static only.
    let consonance = if have_live {
        root_consonance * ROOT_BLEND + live_consonance * LIVE_BLEND
    } else {
        root_consonance
    };

    HarmonicPair {
        root_consonance,
        live_consonance,
        consonance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenney_table_ordering() {
        // 5th > 4th > M3 > m3 > M2 > tritone
        assert!(TENNEY_CONSONANCE[7] > TENNEY_CONSONANCE[5]); // 5th > 4th
        assert!(TENNEY_CONSONANCE[5] > TENNEY_CONSONANCE[4]); // 4th > M3
        assert!(TENNEY_CONSONANCE[4] > TENNEY_CONSONANCE[3]); // M3 > m3
        assert!(TENNEY_CONSONANCE[3] > TENNEY_CONSONANCE[2]); // m3 > M2
        assert!(TENNEY_CONSONANCE[2] > TENNEY_CONSONANCE[6]); // M2 > tritone
        assert!(TENNEY_CONSONANCE[0] > TENNEY_CONSONANCE[7]); // unison > 5th
    }

    #[test]
    fn hz_to_pitch_class_standard() {
        assert_eq!(hz_to_pitch_class(440.0), Some(9));     // A
        assert_eq!(hz_to_pitch_class(261.63), Some(0));    // C
        assert_eq!(hz_to_pitch_class(329.63), Some(4));    // E
        assert_eq!(hz_to_pitch_class(880.0), Some(9));     // A (octave up)
        assert_eq!(hz_to_pitch_class(0.0), None);
        assert_eq!(hz_to_pitch_class(-1.0), None);
    }

    #[test]
    fn tenney_hz_perfect_fifth() {
        // 440 Hz (A4) and 660 Hz (E5) = perfect fifth (3:2)
        let c = tenney_consonance_hz(440.0, 660.0);
        assert!((c - 0.566).abs() < 0.05, "expected ~0.566, got {c}");
    }

    #[test]
    fn tenney_hz_detuned_fifth() {
        // 440 Hz and 650 Hz — a detuned fifth (~675 cents instead of 702)
        let pure = tenney_consonance_hz(440.0, 660.0);
        let detuned = tenney_consonance_hz(440.0, 650.0);
        assert!(detuned < pure, "detuned fifth ({detuned}) should be less consonant than pure ({pure})");
    }

    #[test]
    fn tenney_hz_tritone() {
        // 440 Hz and ~622 Hz = tritone (45:32)
        let c = tenney_consonance_hz(440.0, 622.25);
        assert!(c < 0.15, "tritone consonance should be low, got {c}");
    }

    #[test]
    fn tenney_hz_unison() {
        let c = tenney_consonance_hz(440.0, 440.0);
        assert!((c - 1.0).abs() < 0.01, "unison should be ~1.0, got {c}");
    }

    #[test]
    fn tenney_hz_octave() {
        // Octave = same pitch class, should map to unison consonance
        let c = tenney_consonance_hz(440.0, 880.0);
        assert!((c - 1.0).abs() < 0.01, "octave should be ~1.0, got {c}");
    }

    #[test]
    fn consonance_static_unison() {
        // DRON(A=9) + ACID(A=9) = unison
        let pair = compute_harmonic_pair(9, 9, 0.0, 0.0, 0.3, 0.7);
        assert!((pair.root_consonance - 1.0).abs() < 0.001);
        assert!((pair.consonance - 1.0).abs() < 0.001); // fallback to root
    }

    #[test]
    fn consonance_static_minor_third() {
        // A(9) to C(0) = 3 semitones = minor third
        let pair = compute_harmonic_pair(9, 0, 0.0, 0.0, 0.3, 0.8);
        assert!((pair.root_consonance - 0.339).abs() < 0.01);
    }

    #[test]
    fn consonance_kkit_exempt() {
        // KKIT has scale_affinity = 0.0
        let pair = compute_harmonic_pair(0, 9, 440.0, 660.0, 0.0, 0.8);
        assert_eq!(pair.consonance, 0.0);
        assert_eq!(pair.root_consonance, 0.0);
    }

    #[test]
    fn consonance_fallback_no_seq() {
        // One organism not playing (hz=0) → use root_consonance only
        let pair = compute_harmonic_pair(0, 7, 0.0, 440.0, 0.5, 0.5);
        assert!((pair.consonance - pair.root_consonance).abs() < 0.001);
        assert_eq!(pair.live_consonance, 0.0);
    }

    #[test]
    fn consonance_blended() {
        // Both playing: blend root * 0.3 + live * 0.7
        let pair = compute_harmonic_pair(9, 9, 440.0, 660.0, 0.5, 0.5);
        let expected = 1.0 * ROOT_BLEND + pair.live_consonance * LIVE_BLEND;
        assert!((pair.consonance - expected).abs() < 0.01,
            "blend mismatch: expected {expected}, got {}", pair.consonance);
    }
}
