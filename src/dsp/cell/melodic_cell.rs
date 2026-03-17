//! Melodic cell — unified walk + rhythm + scale quantization for pitched percussion.
//!
//! Combines walk_cell's pitch wandering with seq_cell's rhythmic gating and
//! scale gravity quantization. Outputs gate + pitch Hz on two channels.
//!
//! Designed for RECH's mallet percussion: walk contour discovers pitches,
//! rhythmic gate pattern triggers sample_cell via Trigger wires, and scale
//! gravity keeps pitches consonant with the active raga/scale.
//!
//! # Parameters
//! - `step_size`: Max interval per step in semitones (0.5–12.0)
//! - `gravity`: Scale quantization pull (0.0–1.0)
//! - `center_hz`: Harmonic center frequency (20–2000 Hz)
//! - `range_semitones`: Exploration range from center (1–24 semitones)
//! - `gate_length`: Fraction of step duration for gate (0.01–1.0)
//! - `swing`: Even-step swing amount (0.0–1.0)
//! - `chaos`: Mutation amount (0.0–1.0). 0 = deterministic, 1 = fully random.
//! - `tempo_ratio`: BPM multiplier (default 1.0)
//!
//! # String Parameters
//! - `gates`: Comma-separated gate pattern, e.g. "1,1,0,1,1,0,1,0"
//!
//! # Output
//! - ch0: gate (0.0 or 1.0)
//! - ch1: pitch Hz (scale-quantized, gravity-pulled)

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for melodic_cell.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("step_size", 0.5, 12.0),
    ("gravity", 0.0, 1.0),
    ("center_hz", 20.0, 2000.0),
    ("range_semitones", 1.0, 24.0),
    ("gate_length", 0.01, 1.0),
    ("swing", 0.0, 1.0),
    ("chaos", 0.0, 1.0),
];

/// Inline xorshift32 PRNG — RT-safe, no allocation.
#[inline]
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Convert xorshift output to [-1.0, 1.0) float.
#[inline]
fn rand_bipolar(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32) * 2.0 - 1.0
}

/// Convert xorshift output to [0.0, 1.0) float.
#[inline]
fn rand_f32(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32)
}

/// Parse comma-separated 0/1 gate list.
fn parse_gate_list(s: &str) -> Vec<bool> {
    if s.is_empty() {
        return vec![true; 8]; // Default: all gates on
    }
    s.split(',')
        .map(|part| part.trim() == "1")
        .collect()
}

/// Melodic cell — walk + rhythm + scale quantization.
pub struct MelodicCell {
    // Walk state
    /// Current position in semitones relative to center.
    position: f32,
    /// Current output Hz (last quantized pitch).
    current_hz: f32,

    // Rhythm state
    /// Phase accumulator: 0.0–1.0 within current step.
    phase: f32,
    /// Current step index.
    step: usize,
    /// Gate pattern (DNA seed).
    gate_pattern: Vec<bool>,
    /// Active gate pattern (mutated by chaos).
    active_gates: Vec<bool>,
    /// Whether gate is currently active (within gate_length portion of step).
    gate_active: bool,
    /// Direction for deterministic mode: +1 ascending, -1 descending.
    walk_direction: f32,

    // Shared handles
    step_size_handle: Shared,
    gravity_handle: Shared,
    center_hz_handle: Shared,
    range_semitones_handle: Shared,
    gate_length_handle: Shared,
    swing_handle: Shared,
    chaos_handle: Shared,

    // Timing
    bpm: f32,
    tempo_ratio: f32,
    sample_rate: f32,

    // Scale awareness
    scale_weights: [f32; 12],

    // PRNG
    rng_state: u32,

    base_values: HashMap<String, f32>,
}

impl MelodicCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let step_size = param_or(dna, "step_size", 3.0);
        let gravity = param_or(dna, "gravity", 0.6);
        let center_hz = param_or(dna, "center_hz", 261.6);
        let range_semitones = param_or(dna, "range_semitones", 12.0);
        let gate_length = param_or(dna, "gate_length", 0.3);
        let swing = param_or(dna, "swing", 0.0);
        let chaos = param_or(dna, "chaos", 0.02);
        let tempo_ratio = param_or(dna, "tempo_ratio", 1.0);
        let bpm = param_or(dna, "bpm", 130.0);

        let gates_str = string_param_or(dna, "gates", "1,1,0,1,1,0,1,0");
        let gate_pattern = parse_gate_list(gates_str);
        let active_gates = gate_pattern.clone();

        let step_size_handle = shared::shared(step_size);
        let gravity_handle = shared::shared(gravity);
        let center_hz_handle = shared::shared(center_hz);
        let range_semitones_handle = shared::shared(range_semitones);
        let gate_length_handle = shared::shared(gate_length);
        let swing_handle = shared::shared(swing);
        let chaos_handle = shared::shared(chaos);

        // Seed PRNG from center_hz + tempo_ratio bits
        let rng_state = center_hz.to_bits()
            .wrapping_mul(2654435761)
            .wrapping_add(tempo_ratio.to_bits())
            | 1;

        let mut base_values = HashMap::new();
        base_values.insert("step_size".into(), step_size);
        base_values.insert("gravity".into(), gravity);
        base_values.insert("center_hz".into(), center_hz);
        base_values.insert("range_semitones".into(), range_semitones);
        base_values.insert("gate_length".into(), gate_length);
        base_values.insert("swing".into(), swing);
        base_values.insert("chaos".into(), chaos);

        let handles = vec![
            ("step_size".into(), step_size_handle.clone()),
            ("gravity".into(), gravity_handle.clone()),
            ("center_hz".into(), center_hz_handle.clone()),
            ("range_semitones".into(), range_semitones_handle.clone()),
            ("gate_length".into(), gate_length_handle.clone()),
            ("swing".into(), swing_handle.clone()),
            ("chaos".into(), chaos_handle.clone()),
        ];

        let cell = Self {
            position: 0.0,
            current_hz: center_hz,
            phase: 0.0,
            step: 0,
            gate_pattern,
            active_gates,
            gate_active: false,
            walk_direction: 1.0,
            step_size_handle,
            gravity_handle,
            center_hz_handle,
            range_semitones_handle,
            gate_length_handle,
            swing_handle,
            chaos_handle,
            bpm,
            tempo_ratio,
            sample_rate: sr,
            scale_weights: [0.0; 12],
            rng_state,
            base_values,
        };

        Some((Box::new(cell), handles))
    }

    /// Quantize semitone position toward nearest active scale degree.
    fn quantize_gravity(&self, semitones: f32, gravity: f32) -> f32 {
        if gravity < 0.01 {
            return semitones;
        }

        // Check if any scale weights are active
        let has_active = self.scale_weights.iter().any(|&w| w >= 0.1);
        if !has_active {
            return semitones;
        }

        let octave_pos = ((semitones % 12.0) + 12.0) % 12.0;
        let octave_base = semitones - octave_pos;

        let mut best_degree = octave_pos;
        let mut best_dist = f32::MAX;
        for i in 0..12 {
            if self.scale_weights[i] < 0.1 {
                continue;
            }
            let d = i as f32;
            let dist = (octave_pos - d).abs().min(12.0 - (octave_pos - d).abs());
            let weighted_dist = dist / self.scale_weights[i];
            if weighted_dist < best_dist {
                best_dist = weighted_dist;
                best_degree = d;
            }
        }

        let quantized = octave_base + best_degree;
        semitones * (1.0 - gravity) + quantized * gravity
    }
}

impl DspCell for MelodicCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let step_size = clamp_param(PARAM_RANGES, "step_size", self.step_size_handle.value());
        let gravity = clamp_param(PARAM_RANGES, "gravity", self.gravity_handle.value());
        let center_hz = clamp_param(PARAM_RANGES, "center_hz", self.center_hz_handle.value());
        let range = clamp_param(PARAM_RANGES, "range_semitones", self.range_semitones_handle.value());
        let gate_length = clamp_param(PARAM_RANGES, "gate_length", self.gate_length_handle.value());
        let swing = clamp_param(PARAM_RANGES, "swing", self.swing_handle.value());
        let chaos = clamp_param(PARAM_RANGES, "chaos", self.chaos_handle.value());

        let pattern_len = self.active_gates.len().max(1);

        // Compute samples per step from BPM
        // BPM = beats per minute, each beat = one step
        let effective_bpm = (self.bpm * self.tempo_ratio).clamp(10.0, 600.0);
        let steps_per_second = effective_bpm / 60.0;
        let phase_inc = steps_per_second / self.sample_rate;

        // Apply swing: even steps are delayed
        let is_even_step = self.step % 2 == 1; // 0-indexed: step 1, 3, 5... are "even" in musical sense
        let swing_offset = if is_even_step { swing * 0.33 } else { 0.0 };

        self.phase += phase_inc;

        // Step boundary
        if self.phase >= 1.0 + swing_offset {
            self.phase -= 1.0 + swing_offset;
            self.step = (self.step + 1) % pattern_len;

            // Chaos mutation on gate pattern
            if chaos > 0.01 {
                let flip_prob = chaos * 0.3; // Scale down so chaos=1 doesn't flip every gate
                for i in 0..pattern_len {
                    if rand_f32(&mut self.rng_state) < flip_prob {
                        self.active_gates[i] = !self.active_gates[i];
                    }
                }
            }

            // Check gate state for current step
            let gate_on = self.active_gates[self.step];

            if gate_on {
                // Advance walk position
                if chaos < 0.01 {
                    // Deterministic: fixed ascending step, wrap at range
                    self.position += step_size * self.walk_direction;
                    if self.position > range {
                        self.walk_direction = -1.0;
                        self.position = range;
                    } else if self.position < -range {
                        self.walk_direction = 1.0;
                        self.position = -range;
                    }
                } else {
                    // Random walk with chaos-scaled randomness
                    let delta = rand_bipolar(&mut self.rng_state) * step_size * chaos
                        + step_size * self.walk_direction * (1.0 - chaos);
                    self.position += delta;
                }

                // Soft clamp to range (spring force)
                if self.position.abs() > range {
                    let overshoot = self.position.abs() - range;
                    self.position -= self.position.signum() * overshoot * 0.5;
                    // Reverse direction when hitting boundary
                    if self.position.abs() > range {
                        self.walk_direction = -self.walk_direction;
                        self.position = self.position.clamp(-range, range);
                    }
                }

                // Gravity quantization toward scale degrees
                self.position = self.quantize_gravity(self.position, gravity);

                // Convert to Hz
                self.current_hz = (center_hz * 2.0f32.powf(self.position / 12.0))
                    .clamp(20.0, 20000.0);

                self.gate_active = true;
            } else {
                // Gate off — pitch stays, gate goes low
                self.gate_active = false;
            }
        }

        // Gate duration: turn off gate after gate_length portion of step
        if self.gate_active && self.phase > gate_length {
            self.gate_active = false;
        }

        // Output
        if output.len() >= 2 {
            output[0] = if self.gate_active { 1.0 } else { 0.0 };
            output[1] = self.current_hz;
        } else if !output.is_empty() {
            output[0] = if self.gate_active { 1.0 } else { 0.0 };
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::SetGlobalBpm(bpm) => {
                self.bpm = *bpm;
            }
            DspCommand::SetScaleWeights(weights, _blend) => {
                self.scale_weights = *weights;
            }
            DspCommand::SetChaos(c) => {
                self.chaos_handle.set(*c);
            }
            DspCommand::NudgePhase(nudge) => {
                if *nudge == 0.0 {
                    self.phase = 0.0;
                    self.step = 0;
                } else {
                    self.phase = (self.phase + nudge).rem_euclid(1.0);
                }
            }
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        2 // ch0=gate, ch1=pitch Hz
    }

    fn reset(&mut self) {
        self.position = 0.0;
        self.current_hz = self.center_hz_handle.value();
        self.phase = 0.0;
        self.step = 0;
        self.gate_active = false;
        self.walk_direction = 1.0;
        // Restore gate pattern from DNA seed
        self.active_gates = self.gate_pattern.clone();
    }

    fn name(&self) -> &str {
        "melodic_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_melodic_dna(
        center_hz: f32,
        step_size: f32,
        gravity: f32,
        chaos: f32,
        gates: &str,
    ) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("center_hz".into(), center_hz);
        params.insert("step_size".into(), step_size);
        params.insert("gravity".into(), gravity);
        params.insert("range_semitones".into(), 12.0);
        params.insert("gate_length".into(), 0.3);
        params.insert("swing".into(), 0.0);
        params.insert("chaos".into(), chaos);
        params.insert("bpm".into(), 300.0); // Fast BPM for testing
        params.insert("tempo_ratio".into(), 1.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("gates".into(), gates.into());

        CellDna {
            cell_type: "melodic_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn melodic_initial_output_is_center() {
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.0, "1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);

        // Initial pitch should be center_hz
        assert!(
            (output[1] - 261.6).abs() < 0.5,
            "initial pitch should be ~center_hz, got {}",
            output[1]
        );
    }

    #[test]
    fn melodic_deterministic_ascending() {
        let dna = make_melodic_dna(261.6, 2.0, 0.0, 0.0, "1,1,1,1,1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut pitches = Vec::new();

        // Run long enough to get several steps
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut output);
            if output[0] > 0.5 && !pitches.contains(&((output[1] * 10.0) as i32)) {
                pitches.push((output[1] * 10.0) as i32);
            }
        }

        assert!(pitches.len() > 2, "should have multiple distinct pitches, got {}", pitches.len());
    }

    #[test]
    fn melodic_gate_pattern_rests() {
        // Pattern with rests — pitch should not advance on rest steps
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.0, "1,0,0,0");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut gate_high_count = 0;
        let mut gate_low_count = 0;

        for _ in 0..(SR as usize) {
            cell.tick(&[], &mut output);
            if output[0] > 0.5 {
                gate_high_count += 1;
            } else {
                gate_low_count += 1;
            }
        }

        // With "1,0,0,0" pattern, gate should be low most of the time
        assert!(
            gate_low_count > gate_high_count * 2,
            "gate should be low most of the time with 1,0,0,0 pattern (high={}, low={})",
            gate_high_count, gate_low_count
        );
    }

    #[test]
    fn melodic_gravity_quantizes_to_scale() {
        let dna = make_melodic_dna(261.6, 3.0, 1.0, 0.0, "1,1,1,1,1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        // Set pentatonic scale weights: C D E G A
        let mut weights = [0.0f32; 12];
        weights[0] = 1.0; // C
        weights[2] = 1.0; // D
        weights[4] = 1.0; // E
        weights[7] = 1.0; // G
        weights[9] = 1.0; // A
        cell.handle_command(&DspCommand::SetScaleWeights(weights, 1.0));

        let mut output = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut output);
        }

        // With gravity=1.0 and pentatonic weights, pitch should be valid
        assert!(output[1] > 20.0 && output[1] < 20000.0, "pitch should be in valid range");
    }

    #[test]
    fn melodic_chaos_mutates_pattern() {
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.8, "1,1,1,1,1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut saw_gate_off = false;

        // With chaos=0.8, gate pattern should mutate — some steps should become rests
        for _ in 0..(SR as usize * 3) {
            cell.tick(&[], &mut output);
            if output[0] < 0.5 {
                saw_gate_off = true;
            }
        }

        assert!(saw_gate_off, "with chaos=0.8, should see some gate-off periods from mutation");
    }

    #[test]
    fn melodic_set_global_bpm() {
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.0, "1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        // Set BPM to something different
        cell.handle_command(&DspCommand::SetGlobalBpm(200.0));

        let mut output = [0.0f32; 2];
        let mut step_transitions = 0;
        let mut prev_gate = false;

        for _ in 0..(SR as usize) {
            cell.tick(&[], &mut output);
            let gate = output[0] > 0.5;
            if gate && !prev_gate {
                step_transitions += 1;
            }
            prev_gate = gate;
        }

        // At 200 BPM with 4-step pattern, ~200/60 = 3.3 steps/sec
        // Should see a few transitions in 1 second
        assert!(
            step_transitions >= 2,
            "should see step transitions at 200 BPM, got {}",
            step_transitions
        );
    }

    #[test]
    fn melodic_output_channels_is_two() {
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.0, "1,1,1,1");
        let (cell, _) = MelodicCell::new(&dna, SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn melodic_reset_restores_state() {
        let dna = make_melodic_dna(261.6, 3.0, 0.0, 0.0, "1,1,1,1");
        let (mut cell, _) = MelodicCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        // Run for a while
        for _ in 0..(SR as usize) {
            cell.tick(&[], &mut output);
        }

        cell.handle_command(&DspCommand::Reset);
        cell.tick(&[], &mut output);

        // After reset, pitch should be back near center
        assert!(
            (output[1] - 261.6).abs() < 1.0,
            "after reset, pitch should be near center_hz, got {}",
            output[1]
        );
    }
}
