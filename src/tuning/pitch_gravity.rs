use super::scala::TuningSystem;

/// Result of a weighted nearest-degree search.
#[derive(Debug, Clone)]
pub struct NearestDegree {
    /// Index into TuningSystem.cents (0 = root).
    pub degree_index: usize,
    /// The degree's cents value in the original (unfolded) octave.
    pub degree_cents: f64,
    /// Signed distance from input cents to degree cents.
    pub distance: f64,
    /// The degree's weight from degree_weights.
    pub weight: f32,
}

/// Pitch gravity quantizer — pulls continuous pitch toward scale degrees
/// with configurable strength and per-degree weighting.
pub struct PitchGravity {
    pub tuning: TuningSystem,
    pub root_hz: f64,
    /// 0.0 = free (no snapping), 1.0 = hard snap to nearest degree.
    pub gravity: f32,
    /// Octave range for pitch mapping: (low, high) inclusive.
    pub octave_range: (i32, i32),
    /// Per-degree pull multiplier. Higher weight = stronger attraction.
    /// Length should match tuning.cents.len(). Default: all 1.0.
    pub degree_weights: Vec<f32>,
}

impl PitchGravity {
    pub fn new(tuning: TuningSystem) -> Self {
        let num_degrees = tuning.cents.len();
        Self {
            root_hz: 261.63,
            gravity: 0.5,
            octave_range: (-1, 2),
            degree_weights: vec![1.0; num_degrees],
            tuning,
        }
    }

    /// Total cents span of the pitch range.
    fn total_cents_span(&self) -> f64 {
        let period = self.tuning.period_cents();
        (self.octave_range.1 - self.octave_range.0) as f64 * period
    }

    /// Map raw_pitch [0,1] to absolute cents from root.
    pub fn raw_to_cents(&self, raw_pitch: f32) -> f64 {
        let period = self.tuning.period_cents();
        let low_cents = self.octave_range.0 as f64 * period;
        low_cents + (raw_pitch as f64) * self.total_cents_span()
    }

    /// Convert absolute cents from root to Hz.
    pub fn cents_to_hz(&self, cents: f64) -> f64 {
        self.root_hz * 2.0_f64.powf(cents / 1200.0)
    }

    /// Quantize a raw_pitch [0,1] to Hz using gravity pull.
    ///
    /// Algorithm:
    /// 1. Map raw_pitch to cents across the octave range
    /// 2. Find the nearest weighted degree
    /// 3. Apply cubic pull curve: pull = d * |d|^(1 + gravity)
    /// 4. Subtract pull from raw position
    /// 5. Convert final cents to Hz
    pub fn quantize(&self, raw_pitch: f32) -> f64 {
        let raw_cents = self.raw_to_cents(raw_pitch);
        let nearest = self.find_nearest_weighted(raw_cents);

        if self.gravity <= 0.0 {
            return self.cents_to_hz(raw_cents);
        }

        if self.gravity >= 1.0 {
            return self.cents_to_hz(nearest.degree_cents);
        }

        // Normalize distance by period for scale-density independence
        let period = self.tuning.period_cents();
        let norm_d = nearest.distance / period;
        let pull_normalized = norm_d * norm_d.abs().powf(1.0 + self.gravity as f64);
        let pull_cents = pull_normalized * period;

        self.cents_to_hz(raw_cents - pull_cents)
    }

    /// Find the nearest scale degree, weighted by degree_weights.
    ///
    /// Folds input cents into one period via rem_euclid, computes
    /// effective_distance = |cents - degree| / weight, picks minimum.
    /// Returns result with cents restored to the original octave.
    pub fn find_nearest_weighted(&self, cents: f64) -> NearestDegree {
        let period = self.tuning.period_cents();
        let folded = cents.rem_euclid(period);
        let octave_base = cents - folded;

        let mut best = NearestDegree {
            degree_index: 0,
            degree_cents: octave_base,
            distance: folded,
            weight: self.weight_for(0),
        };
        let mut best_effective = folded.abs() / self.weight_for(0).max(f32::EPSILON) as f64;

        // Check each degree (skip last = period endpoint, same as root)
        let inner_count = if self.tuning.cents.len() > 1 {
            self.tuning.cents.len() - 1
        } else {
            self.tuning.cents.len()
        };

        for (i, &deg_cents) in self.tuning.cents[..inner_count].iter().enumerate() {
            let w = self.weight_for(i);
            if w <= 0.0 {
                continue;
            }

            let dist = folded - deg_cents;
            let effective = dist.abs() / w as f64;

            if effective < best_effective {
                best_effective = effective;
                best = NearestDegree {
                    degree_index: i,
                    degree_cents: octave_base + deg_cents,
                    distance: dist,
                    weight: w,
                };
            }
        }

        // Check wrap-around to root of next period
        {
            let wrap_dist = folded - period;
            let w = self.weight_for(0);
            let effective = wrap_dist.abs() / w.max(f32::EPSILON) as f64;
            if effective < best_effective {
                best = NearestDegree {
                    degree_index: 0,
                    degree_cents: octave_base + period,
                    distance: wrap_dist,
                    weight: w,
                };
            }
        }

        best
    }

    fn weight_for(&self, index: usize) -> f32 {
        self.degree_weights.get(index).copied().unwrap_or(1.0)
    }
}

/// Block-rate pitch smoother — provides portamento glide between
/// discrete quantize() jumps.
pub struct PitchSmoother {
    current_hz: f64,
    target_hz: f64,
    /// Slew rate in Hz per second.
    pub slew_rate: f64,
}

impl PitchSmoother {
    pub fn new(slew_rate: f64) -> Self {
        Self {
            current_hz: 261.63,
            target_hz: 261.63,
            slew_rate,
        }
    }

    /// Set a new target frequency from quantize() output.
    pub fn set_target(&mut self, hz: f64) {
        self.target_hz = hz;
    }

    /// Advance by dt seconds. Returns the smoothed Hz value.
    pub fn tick(&mut self, dt: f32) -> f64 {
        let diff = self.target_hz - self.current_hz;
        if diff.abs() < 0.01 {
            self.current_hz = self.target_hz;
        } else {
            let max_step = self.slew_rate * dt as f64;
            let step = diff.signum() * max_step.min(diff.abs());
            self.current_hz += step;
        }
        self.current_hz
    }

    /// Current smoothed Hz value.
    pub fn current_hz(&self) -> f64 {
        self.current_hz
    }

    /// Immediately jump to target (no glide).
    pub fn snap_to_target(&mut self) {
        self.current_hz = self.target_hz;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::scala::TuningSystem;

    const BHAIRAV_SCL: &str = "\
! bhairav.scl
!
Bhairav raga
7
!
112.00000
386.31371
498.04500
701.95500
813.68629
1088.26871
2/1
";

    fn bhairav_gravity(gravity: f32) -> PitchGravity {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let mut pg = PitchGravity::new(ts);
        pg.gravity = gravity;
        pg.octave_range = (0, 1);
        pg
    }

    #[test]
    fn gravity_zero_is_continuous() {
        let pg = bhairav_gravity(0.0);
        let hz_a = pg.quantize(0.40);
        let hz_b = pg.quantize(0.41);
        let ratio = hz_b / hz_a;
        assert!(
            (ratio - 1.0).abs() < 0.05,
            "gravity=0 should be continuous: {hz_a} -> {hz_b}"
        );
    }

    #[test]
    fn gravity_one_snaps_to_degrees() {
        let pg = bhairav_gravity(1.0);
        // Both raw pitches near Pa (~702/1200 ≈ 0.585)
        let hz_a = pg.quantize(0.55);
        let hz_b = pg.quantize(0.60);
        assert!(
            (hz_a - hz_b).abs() < 1.0,
            "gravity=1 should snap to same degree: {hz_a} vs {hz_b}"
        );
    }

    #[test]
    fn gravity_half_pulls_toward_degrees() {
        let pg = bhairav_gravity(0.5);
        // 600 cents = midway between Ma (498) and Pa (702)
        let hz = pg.quantize(600.0 / 1200.0);
        let ma_hz = pg.tuning.degree_to_hz(3, 0, pg.root_hz);
        let pa_hz = pg.tuning.degree_to_hz(4, 0, pg.root_hz);
        assert!(
            hz > ma_hz - 1.0 && hz < pa_hz + 1.0,
            "should be between Ma and Pa: {hz} (Ma={ma_hz}, Pa={pa_hz})"
        );
    }

    #[test]
    fn find_nearest_weighted_basic() {
        let pg = bhairav_gravity(0.5);
        let nearest = pg.find_nearest_weighted(700.0);
        assert_eq!(nearest.degree_index, 4, "700 cents nearest to Pa (702)");
    }

    #[test]
    fn find_nearest_weighted_high_weight() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let mut pg = PitchGravity::new(ts);
        pg.octave_range = (0, 1);
        pg.degree_weights[3] = 10.0; // Ma gets high weight
        pg.degree_weights[4] = 1.0; // Pa stays normal

        // 650 cents: normally closer to Pa (702, dist=52) than Ma (498, dist=152)
        // But Ma's weight 10: effective_dist = 152/10 = 15.2
        // Pa's weight 1: effective_dist = 52/1 = 52.0
        let nearest = pg.find_nearest_weighted(650.0);
        assert_eq!(
            nearest.degree_index, 3,
            "high-weight Ma should attract 650 cents"
        );
    }

    #[test]
    fn quantize_produces_positive_hz() {
        let pg = bhairav_gravity(0.5);
        for i in 0..=100 {
            let raw = i as f32 / 100.0;
            let hz = pg.quantize(raw);
            assert!(hz > 0.0, "Hz should be positive for raw={raw}, got {hz}");
        }
    }

    #[test]
    fn cents_to_hz_root() {
        let pg = bhairav_gravity(0.5);
        let hz = pg.cents_to_hz(0.0);
        assert!((hz - 261.63).abs() < 0.01);
    }

    #[test]
    fn cents_to_hz_octave() {
        let pg = bhairav_gravity(0.5);
        let hz = pg.cents_to_hz(1200.0);
        assert!(
            (hz - 523.26).abs() < 0.1,
            "1200 cents should double root: {hz}"
        );
    }

    #[test]
    fn smoother_reaches_target() {
        let mut sm = PitchSmoother::new(1000.0);
        sm.set_target(440.0);

        for _ in 0..60 {
            sm.tick(1.0 / 60.0);
        }

        assert!(
            (sm.current_hz() - 440.0).abs() < 1.0,
            "should reach target: {}",
            sm.current_hz()
        );
    }

    #[test]
    fn smoother_slew_rate_limits_speed() {
        let mut sm = PitchSmoother::new(100.0);
        sm.set_target(440.0);

        sm.tick(1.0 / 60.0);
        let moved = (sm.current_hz() - 261.63).abs();
        assert!(moved < 2.0, "should move ~1.67 Hz per tick, moved {moved}");
        assert!(moved > 1.0, "should move at least 1 Hz, moved {moved}");
    }

    #[test]
    fn smoother_snap_to_target() {
        let mut sm = PitchSmoother::new(100.0);
        sm.set_target(880.0);
        sm.snap_to_target();
        assert!((sm.current_hz() - 880.0).abs() < 0.001);
    }
}
