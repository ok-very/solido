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
    /// 3. Normalize distance to [-1, 1] using midpoints to adjacent degrees
    /// 4. Apply pull curve: pull_norm = norm_d * |norm_d|^gravity
    /// 5. Scale back to cents and subtract from raw position
    /// 6. Convert final cents to Hz
    pub fn quantize(&self, raw_pitch: f32) -> f64 {
        let raw_cents = self.raw_to_cents(raw_pitch);
        let nearest = self.find_nearest_weighted(raw_cents);

        if self.gravity <= 0.0 {
            return self.cents_to_hz(raw_cents);
        }

        if self.gravity >= 1.0 {
            return self.cents_to_hz(nearest.degree_cents);
        }

        // Find half-span to boundary (midpoint to adjacent degree)
        let half_span = self.half_span_for(nearest.degree_index, nearest.distance);

        if half_span < 0.01 {
            // Degenerate: single-note scale or zero spacing
            return self.cents_to_hz(nearest.degree_cents);
        }

        // Normalize distance to [-1, 1]
        let norm_d = (nearest.distance / half_span).clamp(-1.0, 1.0);

        // Apply pull curve: gravity=0 → linear (no pull), gravity=1 → x|x| (strong pull)
        let pull_norm = norm_d * norm_d.abs().powf(self.gravity as f64);

        // Scale back to cents
        let pull_cents = pull_norm * half_span;

        self.cents_to_hz(raw_cents - pull_cents)
    }

    /// Compute the half-span (distance from degree to its boundary) on the
    /// side where the input lies. The boundary is the midpoint to the adjacent degree.
    fn half_span_for(&self, degree_index: usize, distance: f64) -> f64 {
        let period = self.tuning.period_cents();
        let n = self.tuning.cents.len(); // includes root, excludes period duplicate

        if n <= 1 {
            return period / 2.0;
        }

        // Inner degrees (excluding period endpoint)
        let inner = n - 1;
        let degree_cents_folded = self.tuning.cents[degree_index];

        // Find the neighbor on the side the input is on
        if distance >= 0.0 {
            // Input is above the degree — find next higher degree
            let next_idx = if degree_index < inner - 1 {
                degree_index + 1
            } else {
                // Wrap: next is root + period
                0
            };
            let next_cents = if next_idx == 0 {
                period // root of next octave
            } else {
                self.tuning.cents[next_idx]
            };
            let span = next_cents - degree_cents_folded;
            (span / 2.0).max(0.01)
        } else {
            // Input is below the degree — find previous lower degree
            let prev_cents = if degree_index > 0 {
                self.tuning.cents[degree_index - 1]
            } else {
                // Wrap: previous is last inner degree - period
                self.tuning.cents[inner] - period
            };
            let span = degree_cents_folded - prev_cents;
            (span / 2.0).max(0.01)
        }
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
///
/// Smooths in the logarithmic (cents) domain so that a glide across
/// one octave takes the same time regardless of register. Converts
/// to Hz only at the output boundary.
pub struct PitchSmoother {
    /// Current pitch in cents (log domain).
    current_cents: f64,
    /// Target pitch in cents (log domain).
    target_cents: f64,
    /// Slew rate in cents per second.
    pub slew_rate: f64,
    /// Reference frequency for cents=0 (default: 261.63 Hz = C4).
    root_hz: f64,
}

impl PitchSmoother {
    /// Create a new PitchSmoother. `slew_rate` is in cents per second.
    pub fn new(slew_rate: f64) -> Self {
        Self {
            current_cents: 0.0,
            target_cents: 0.0,
            slew_rate,
            root_hz: 261.63,
        }
    }

    /// Set the root Hz reference (must match PitchGravity.root_hz).
    pub fn set_root_hz(&mut self, hz: f64) {
        self.root_hz = hz;
    }

    /// Set a new target frequency from quantize() output.
    /// Internally converts to cents for log-domain smoothing.
    pub fn set_target(&mut self, hz: f64) {
        if hz > 0.0 {
            self.target_cents = 1200.0 * (hz / self.root_hz).log2();
        }
    }

    /// Advance by dt seconds. Returns the smoothed Hz value.
    pub fn tick(&mut self, dt: f32) -> f64 {
        let diff = self.target_cents - self.current_cents;
        if diff.abs() < 0.1 {
            // Close enough in cents — snap to avoid creep
            self.current_cents = self.target_cents;
        } else {
            let max_step = self.slew_rate * dt as f64;
            let step = diff.signum() * max_step.min(diff.abs());
            self.current_cents += step;
        }
        self.current_hz()
    }

    /// Current smoothed Hz value (converted from internal cents).
    pub fn current_hz(&self) -> f64 {
        self.root_hz * 2.0_f64.powf(self.current_cents / 1200.0)
    }

    /// Immediately jump to target (no glide).
    pub fn snap_to_target(&mut self) {
        self.current_cents = self.target_cents;
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
        // 440 Hz is ~900 cents above C4. Slew at 2400 cents/sec = reach in ~0.375s
        let mut sm = PitchSmoother::new(2400.0);
        sm.set_target(440.0);

        // 60 ticks at 60Hz = 1 second — more than enough
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
        // Slew at 600 cents/sec. One tick = 10 cents.
        let mut sm = PitchSmoother::new(600.0);
        sm.set_target(440.0); // ~900 cents above C4

        sm.tick(1.0 / 60.0);
        // Should have moved ~10 cents from 0 cents, so Hz ≈ 261.63 * 2^(10/1200) ≈ 263.15
        let hz = sm.current_hz();
        let moved_cents = 1200.0 * (hz / 261.63).log2();
        assert!(
            (moved_cents - 10.0).abs() < 1.0,
            "should move ~10 cents per tick, moved {moved_cents}"
        );
    }

    #[test]
    fn smoother_snap_to_target() {
        let mut sm = PitchSmoother::new(100.0);
        sm.set_target(880.0);
        sm.snap_to_target();
        assert!(
            (sm.current_hz() - 880.0).abs() < 0.1,
            "snap should reach 880: {}",
            sm.current_hz()
        );
    }

    #[test]
    fn smoother_equal_time_per_octave() {
        // Key audit fix: glide from 100→200 Hz (1 octave) should take the same
        // time as 400→800 Hz (1 octave) in the log domain.
        let slew = 1200.0; // 1200 cents/sec = 1 octave per second

        // Low register: C4 → C5 (261.63 → 523.25)
        let mut sm_low = PitchSmoother::new(slew);
        sm_low.set_target(523.25);
        let mut ticks_low = 0;
        while (sm_low.current_hz() - 523.25).abs() > 1.0 && ticks_low < 120 {
            sm_low.tick(1.0 / 60.0);
            ticks_low += 1;
        }

        // High register: C6 → C7 (1046.5 → 2093.0)
        let mut sm_high = PitchSmoother::new(slew);
        sm_high.set_root_hz(261.63);
        // Start at C6
        sm_high.set_target(1046.5);
        sm_high.snap_to_target();
        sm_high.set_target(2093.0);
        let mut ticks_high = 0;
        while (sm_high.current_hz() - 2093.0).abs() > 2.0 && ticks_high < 120 {
            sm_high.tick(1.0 / 60.0);
            ticks_high += 1;
        }

        // Both should take approximately the same number of ticks (±2)
        assert!(
            (ticks_low as i32 - ticks_high as i32).unsigned_abs() <= 2,
            "log-domain smoothing: low={ticks_low} ticks, high={ticks_high} ticks"
        );
    }
}
