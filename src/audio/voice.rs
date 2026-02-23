use std::f32::consts::PI;

/// ADSR envelope stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR envelope generator with precomputed per-sample rates.
///
/// All times in milliseconds, all levels in 0.0..1.0.
/// Rates are precomputed when `note_on()`/`note_off()` is called to avoid
/// division in the audio callback hot path.
#[derive(Debug, Clone)]
pub struct AdsrState {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    stage: AdsrStage,
    level: f32,
    sample_rate: f32,
    attack_rate: f32,
    decay_rate: f32,
    release_rate: f32,
}

impl AdsrState {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            attack_ms: 10.0,
            decay_ms: 100.0,
            sustain: 0.7,
            release_ms: 200.0,
            stage: AdsrStage::Idle,
            level: 0.0,
            sample_rate,
            attack_rate: 0.0,
            decay_rate: 0.0,
            release_rate: 0.0,
        };
        s.recalc_rates();
        s
    }

    fn recalc_rates(&mut self) {
        let sr = self.sample_rate;
        self.attack_rate = if self.attack_ms > 0.0 {
            1.0 / (self.attack_ms * 0.001 * sr)
        } else {
            1.0 // instant attack
        };
        self.decay_rate = if self.decay_ms > 0.0 {
            (1.0 - self.sustain) / (self.decay_ms * 0.001 * sr)
        } else {
            1.0
        };
        self.release_rate = if self.release_ms > 0.0 {
            self.sustain / (self.release_ms * 0.001 * sr)
        } else {
            1.0
        };
    }

    /// Trigger the envelope. Transitions to Attack from any stage.
    pub fn note_on(&mut self) {
        self.recalc_rates();
        self.stage = AdsrStage::Attack;
        // Don't reset level — allows retriggering from current position
    }

    /// Release the envelope. Transitions to Release from any non-Idle stage.
    pub fn note_off(&mut self) {
        if self.stage != AdsrStage::Idle {
            // Recompute release rate from current level (not sustain level)
            self.release_rate = if self.release_ms > 0.0 {
                self.level / (self.release_ms * 0.001 * self.sample_rate)
            } else {
                1.0
            };
            self.stage = AdsrStage::Release;
        }
    }

    /// Process one sample. Returns the current envelope level (0.0..1.0).
    pub fn process(&mut self) -> f32 {
        match self.stage {
            AdsrStage::Idle => {}
            AdsrStage::Attack => {
                self.level += self.attack_rate;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                self.level -= self.decay_rate;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                self.level = self.sustain;
            }
            AdsrStage::Release => {
                self.level -= self.release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                }
            }
        }
        self.level
    }

    pub fn is_idle(&self) -> bool {
        self.stage == AdsrStage::Idle
    }

    pub fn stage(&self) -> AdsrStage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }
}

/// Chamberlin state-variable filter (lowpass output).
///
/// Simple, stable, good for real-time. Two state variables updated per sample.
#[derive(Debug, Clone)]
pub struct SvfState {
    low: f32,
    band: f32,
}

impl SvfState {
    pub fn new() -> Self {
        Self { low: 0.0, band: 0.0 }
    }

    /// Process one sample through the lowpass filter.
    ///
    /// - `cutoff_hz`: filter cutoff frequency (clamped for stability)
    /// - `resonance`: 0.0 (no resonance) to 1.0 (maximum resonance)
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32, sample_rate: f32) -> f32 {
        // Clamp cutoff so the SVF coefficient stays stable (f < 1.0)
        // Chamberlin is stable when cutoff < sr / (2*PI) ≈ sr * 0.159
        // We use sr * 0.48 as the user-facing max but limit the actual
        // coefficient to 0.95 to prevent blowup.
        let cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.48);
        let f = (2.0 * (PI * cutoff / sample_rate).sin()).min(0.95);
        // Damping: resonance 0..1 maps to q 2..0.1 (prevent self-oscillation)
        let q = 2.0 * (1.0 - resonance.clamp(0.0, 0.95));

        // Chamberlin SVF: update low, compute high from new low, then update band
        self.low += f * self.band;
        let high = input - self.low - q * self.band;
        self.band += f * high;

        // Protect against NaN/inf from numerical edge cases
        if !self.low.is_finite() {
            self.reset();
            return 0.0;
        }

        self.low
    }

    pub fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }
}

/// A single synthesis voice: sine oscillator → SVF lowpass → amplitude → envelope.
///
/// Phase uses f64 for accumulation accuracy over long notes.
/// All audio output is f32 (matches cpal format).
#[derive(Debug, Clone)]
pub struct Voice {
    pub id: u64,
    pub frequency: f32,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub amplitude: f32,
    pub active: bool,
    phase: f64,
    pub(crate) envelope: AdsrState,
    svf: SvfState,
}

impl Voice {
    pub fn new(id: u64, frequency: f32, cutoff: f32, amplitude: f32, sample_rate: f32) -> Self {
        let mut v = Self {
            id,
            frequency,
            filter_cutoff: cutoff,
            filter_resonance: 0.3,
            amplitude,
            active: false,
            phase: 0.0,
            envelope: AdsrState::new(sample_rate),
            svf: SvfState::new(),
        };
        v.note_on();
        v
    }

    /// Create an inactive placeholder voice (for pool initialization).
    pub fn empty(sample_rate: f32) -> Self {
        Self {
            id: 0,
            frequency: 0.0,
            filter_cutoff: 2000.0,
            filter_resonance: 0.3,
            amplitude: 0.0,
            active: false,
            phase: 0.0,
            envelope: AdsrState::new(sample_rate),
            svf: SvfState::new(),
        }
    }

    pub fn note_on(&mut self) {
        self.active = true;
        self.envelope.note_on();
    }

    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Render one sample through the DSP chain.
    pub fn render_sample(&mut self, sample_rate: f32) -> f32 {
        let env = self.envelope.process();
        if env <= 0.0 {
            if self.envelope.is_idle() {
                self.active = false;
            }
            return 0.0;
        }

        // Sine oscillator
        let osc = (self.phase * std::f64::consts::TAU).sin() as f32;
        self.phase += self.frequency as f64 / sample_rate as f64;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // SVF lowpass filter
        let filtered = self.svf.process(osc, self.filter_cutoff, self.filter_resonance, sample_rate);

        // Apply amplitude and envelope
        filtered * self.amplitude * env
    }

    /// True when the voice has finished releasing and can be reclaimed.
    pub fn is_finished(&self) -> bool {
        !self.active && self.envelope.is_idle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    // --- ADSR tests ---

    #[test]
    fn adsr_idle_by_default() {
        let adsr = AdsrState::new(SR);
        assert_eq!(adsr.stage(), AdsrStage::Idle);
        assert!((adsr.level() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn adsr_note_on_starts_attack() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();
        assert_eq!(adsr.stage(), AdsrStage::Attack);
    }

    #[test]
    fn adsr_attack_reaches_peak() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();
        // Attack is 10ms → 441 samples at 44100Hz
        for _ in 0..500 {
            adsr.process();
        }
        assert!(
            (adsr.level() - 1.0).abs() < 0.01,
            "After 500 samples (>10ms attack), level should be ~1.0, got {}",
            adsr.level()
        );
    }

    #[test]
    fn adsr_decay_reaches_sustain() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();
        // Run through attack + decay (10ms + 100ms = ~4851 samples)
        for _ in 0..6000 {
            adsr.process();
        }
        assert_eq!(adsr.stage(), AdsrStage::Sustain);
        assert!(
            (adsr.level() - 0.7).abs() < 0.01,
            "Sustain level should be ~0.7, got {}",
            adsr.level()
        );
    }

    #[test]
    fn adsr_release_reaches_idle() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();
        // Run to sustain
        for _ in 0..6000 {
            adsr.process();
        }
        adsr.note_off();
        assert_eq!(adsr.stage(), AdsrStage::Release);
        // Release is 200ms → 8820 samples
        for _ in 0..10000 {
            adsr.process();
        }
        assert_eq!(adsr.stage(), AdsrStage::Idle);
        assert!(adsr.level() <= 0.0 + 1e-6);
        assert!(adsr.is_idle());
    }

    #[test]
    fn adsr_retrigger_from_release() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();
        for _ in 0..6000 {
            adsr.process();
        }
        adsr.note_off();
        // Let release run partway
        for _ in 0..2000 {
            adsr.process();
        }
        let level_before = adsr.level();
        assert!(level_before > 0.0 && level_before < 0.7);

        // Retrigger
        adsr.note_on();
        assert_eq!(adsr.stage(), AdsrStage::Attack);
        // Level should continue from where it was (not reset to 0)
        assert!((adsr.level() - level_before).abs() < 0.01);
    }

    #[test]
    fn adsr_full_cycle() {
        let mut adsr = AdsrState::new(SR);
        adsr.note_on();

        // Track stages we visit
        let mut visited = Vec::new();
        let mut last_stage = adsr.stage();
        visited.push(last_stage);

        // Attack + Decay + Sustain
        for _ in 0..10000 {
            adsr.process();
            if adsr.stage() != last_stage {
                last_stage = adsr.stage();
                visited.push(last_stage);
            }
        }

        adsr.note_off();
        for _ in 0..15000 {
            adsr.process();
            if adsr.stage() != last_stage {
                last_stage = adsr.stage();
                visited.push(last_stage);
            }
        }

        assert_eq!(
            visited,
            vec![
                AdsrStage::Attack,
                AdsrStage::Decay,
                AdsrStage::Sustain,
                AdsrStage::Release,
                AdsrStage::Idle,
            ]
        );
    }

    // --- SVF tests ---

    #[test]
    fn svf_passes_dc() {
        let mut svf = SvfState::new();
        // DC signal (1.0) through lowpass with high cutoff should settle near 1.0
        // Run long enough for transient to settle
        let mut out = 0.0;
        for _ in 0..4000 {
            out = svf.process(1.0, 5000.0, 0.0, SR);
        }
        assert!(
            (out - 1.0).abs() < 0.1,
            "DC through lowpass should settle near 1.0, got {out}"
        );
    }

    #[test]
    fn svf_attenuates_high_freq() {
        let mut svf = SvfState::new();
        // Generate a 10kHz sine, filter at 200Hz cutoff
        let mut energy = 0.0;
        for i in 0..4410 {
            let input = (2.0 * PI * 10000.0 * i as f32 / SR).sin();
            let out = svf.process(input, 200.0, 0.0, SR);
            energy += out * out;
        }
        let rms = (energy / 4410.0).sqrt();
        assert!(
            rms < 0.05,
            "10kHz through 200Hz lowpass should be heavily attenuated, RMS={rms}"
        );
    }

    #[test]
    fn svf_reset_clears_state() {
        let mut svf = SvfState::new();
        for i in 0..100 {
            let input = (2.0 * PI * 440.0 * i as f32 / SR).sin();
            svf.process(input, 2000.0, 0.5, SR);
        }
        svf.reset();
        assert!((svf.low).abs() < 1e-6);
        assert!((svf.band).abs() < 1e-6);
    }

    // --- Voice tests ---

    #[test]
    fn voice_produces_audio() {
        let mut voice = Voice::new(1, 440.0, 2000.0, 0.5, SR);
        let mut energy = 0.0;
        for _ in 0..4410 {
            let s = voice.render_sample(SR);
            energy += s * s;
        }
        let rms = (energy / 4410.0).sqrt();
        assert!(rms > 0.01, "Active voice should produce audio, RMS={rms}");
    }

    #[test]
    fn voice_silent_after_release() {
        let mut voice = Voice::new(1, 440.0, 2000.0, 0.5, SR);
        // Play through attack+decay to sustain
        for _ in 0..6000 {
            voice.render_sample(SR);
        }
        voice.note_off();
        // Let release complete (200ms = 8820 samples, give extra margin)
        for _ in 0..15000 {
            voice.render_sample(SR);
        }
        assert!(!voice.active);
        assert!(voice.is_finished());
        let s = voice.render_sample(SR);
        assert!(
            s.abs() < 1e-6,
            "Finished voice should output silence, got {s}"
        );
    }

    #[test]
    fn voice_phase_wraps() {
        let mut voice = Voice::new(1, 20000.0, 20000.0, 0.5, SR);
        for _ in 0..100000 {
            voice.render_sample(SR);
        }
        assert!(
            voice.phase >= 0.0 && voice.phase < 1.0,
            "Phase should stay in [0, 1), got {}",
            voice.phase
        );
    }
}
