#![allow(dead_code)]
/// Gamaka ornaments — slides and vibrato in the cents domain.
///
/// All internal state operates in cents (not Hz) per audit finding #4.
/// The output is a cents offset that the quantizer adds to the base pitch.

/// Configuration for gamaka ornaments.
#[derive(Clone, Debug)]
pub struct GamakaConfig {
    /// Time in milliseconds for a slide between degrees.
    pub slide_time_ms: f64,
    /// Vibrato depth in cents (peak deviation from center).
    pub vibrato_depth_cents: f64,
    /// Vibrato rate in Hz.
    pub vibrato_rate_hz: f64,
}

impl Default for GamakaConfig {
    fn default() -> Self {
        Self {
            slide_time_ms: 80.0,
            vibrato_depth_cents: 15.0,
            vibrato_rate_hz: 5.5,
        }
    }
}

/// Current state of gamaka processing.
#[derive(Clone, Debug)]
pub enum GamakaState {
    Idle,
    Sliding {
        from_cents: f64,
        to_cents: f64,
        progress: f64, // 0.0 .. 1.0
    },
    Vibrating {
        center_cents: f64,
        phase: f64, // radians
    },
}

impl GamakaState {
    pub fn new() -> Self {
        GamakaState::Idle
    }

    /// Start a slide from one pitch to another.
    pub fn start_slide(&mut self, from_cents: f64, to_cents: f64) {
        *self = GamakaState::Sliding {
            from_cents,
            to_cents,
            progress: 0.0,
        };
    }

    /// Start vibrato around a center pitch.
    pub fn start_vibrato(&mut self, center_cents: f64) {
        *self = GamakaState::Vibrating {
            center_cents,
            phase: 0.0,
        };
    }

    /// Advance the gamaka state by `dt` seconds. Returns cents offset from base pitch.
    pub fn tick(&mut self, dt: f64, config: &GamakaConfig) -> f64 {
        match self {
            GamakaState::Idle => 0.0,

            GamakaState::Sliding {
                from_cents,
                to_cents,
                progress,
            } => {
                let slide_time_sec = config.slide_time_ms / 1000.0;
                if slide_time_sec <= 0.0 {
                    let offset = *to_cents - *from_cents;
                    *self = GamakaState::Idle;
                    return offset;
                }

                *progress += dt / slide_time_sec;

                if *progress >= 1.0 {
                    let final_cents = *to_cents;
                    let from = *from_cents;
                    // Transition to vibrato at the destination
                    *self = GamakaState::Vibrating {
                        center_cents: final_cents,
                        phase: 0.0,
                    };
                    return final_cents - from;
                }

                // Smooth interpolation (ease-in-out via smoothstep)
                let t = *progress;
                let smooth = t * t * (3.0 - 2.0 * t);
                let current = *from_cents + (*to_cents - *from_cents) * smooth;
                current - *from_cents
            }

            GamakaState::Vibrating {
                center_cents: _,
                phase,
            } => {
                let rate = config.vibrato_rate_hz;
                let depth = config.vibrato_depth_cents;

                *phase += dt * rate * std::f64::consts::TAU;
                // Keep phase bounded to avoid precision loss
                if *phase > std::f64::consts::TAU * 1000.0 {
                    *phase %= std::f64::consts::TAU;
                }

                depth * phase.sin()
            }
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, GamakaState::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_reaches_target() {
        let config = GamakaConfig {
            slide_time_ms: 100.0,
            vibrato_depth_cents: 0.0,
            vibrato_rate_hz: 5.0,
        };
        let mut state = GamakaState::new();
        state.start_slide(0.0, 200.0);

        let mut max_offset = 0.0_f64;
        // 200ms at 1ms steps — double the slide time
        for _ in 0..200 {
            let offset = state.tick(0.001, &config);
            if offset.abs() > max_offset.abs() {
                max_offset = offset;
            }
        }

        // Should have reached 200 cents offset at slide completion
        // (then transitions to vibrato with 0 depth)
        assert!(
            (max_offset - 200.0).abs() < 1.0,
            "slide should reach target: {}",
            max_offset
        );
        // Should now be in vibrato state
        assert!(
            matches!(state, GamakaState::Vibrating { .. }),
            "should transition to vibrato after slide"
        );
    }

    #[test]
    fn slide_transitions_to_vibrato() {
        let config = GamakaConfig {
            slide_time_ms: 50.0,
            vibrato_depth_cents: 10.0,
            vibrato_rate_hz: 5.0,
        };
        let mut state = GamakaState::new();
        state.start_slide(0.0, 100.0);

        // Run past slide completion
        for _ in 0..100 {
            state.tick(0.001, &config);
        }

        // Should now be vibrating
        assert!(
            matches!(state, GamakaState::Vibrating { .. }),
            "should transition to vibrato after slide"
        );
    }

    #[test]
    fn vibrato_oscillates() {
        let config = GamakaConfig {
            slide_time_ms: 100.0,
            vibrato_depth_cents: 20.0,
            vibrato_rate_hz: 10.0,
        };
        let mut state = GamakaState::new();
        state.start_vibrato(440.0);

        let mut min_offset = f64::MAX;
        let mut max_offset = f64::MIN;

        for _ in 0..1000 {
            let offset = state.tick(0.001, &config);
            min_offset = min_offset.min(offset);
            max_offset = max_offset.max(offset);
        }

        assert!(
            max_offset > 10.0,
            "vibrato should oscillate positive: {}",
            max_offset
        );
        assert!(
            min_offset < -10.0,
            "vibrato should oscillate negative: {}",
            min_offset
        );
    }

    #[test]
    fn idle_returns_zero() {
        let config = GamakaConfig::default();
        let mut state = GamakaState::new();
        assert!(state.is_idle());
        assert_eq!(state.tick(0.016, &config), 0.0);
    }

    #[test]
    fn cents_domain_output() {
        // Verify output is in cents, not Hz
        let config = GamakaConfig {
            slide_time_ms: 100.0,
            vibrato_depth_cents: 50.0,
            vibrato_rate_hz: 5.0,
        };
        let mut state = GamakaState::new();
        state.start_vibrato(0.0);

        let offset = state.tick(0.05, &config); // quarter cycle at 5Hz
        // Should be within vibrato depth range
        assert!(
            offset.abs() <= 50.0 + 1.0,
            "offset should be in cents range: {}",
            offset
        );
    }
}
