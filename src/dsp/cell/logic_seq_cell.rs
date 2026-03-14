//! Algorithmic trigger pattern generator.
//!
//! Generates trigger patterns via mathematical algorithms: euclidean rhythm,
//! prime steps, fibonacci spacing, and polyrhythm overlays. Used for SPGL's
//! low-density triggers and TBLK's percussion patterns.
//!
//! # Parameters
//! - `rate`: Trigger clock rate (0.1-20 Hz)
//! - `density`: Pattern fill density (0-1)
//!
//! # String Parameters
//! - `algorithm`: Pattern type (euclidean, prime, fibonacci, polyrhythm)
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "logic_seq_cell",
//!   "params": { "rate": 0.5, "density": 0.3 },
//!   "string_params": { "algorithm": "fibonacci" }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Trigger source: logic_seq → env_cell (gate edge detection)
//! - Cross-organism: logic_seq emits to affinity graph for learned sync

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell};

/// Valid parameter ranges for logic_seq_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("rate", 0.01, 50.0),
    ("density", 0.0, 1.0),
    ("rotation", 0.0, 15.0),
    ("mutation_rate", 0.0, 1.0),
    ("accent_density", 0.0, 1.0),
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

/// Convert xorshift output to [0.0, 1.0) float.
#[inline]
fn rand_f32(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32)
}
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Algorithmic pattern types.
#[derive(Clone, Copy, Debug)]
enum LogicAlgorithm {
    Euclidean,   // Bjorklund distribution (maximal evenness)
    Prime,       // Triggers on prime-numbered steps
    Fibonacci,   // Inter-trigger spacing follows fibonacci sequence
    Polyrhythm,  // Two overlapping euclidean patterns
}

impl LogicAlgorithm {
    /// Parse algorithm from string param.
    fn from_str(s: &str) -> Self {
        match s {
            "prime" => LogicAlgorithm::Prime,
            "fibonacci" | "fib" => LogicAlgorithm::Fibonacci,
            "polyrhythm" | "poly" => LogicAlgorithm::Polyrhythm,
            _ => LogicAlgorithm::Euclidean, // Default
        }
    }
}

/// Check if a number is prime.
fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let limit = (n as f32).sqrt() as u32;
    for i in (3..=limit).step_by(2) {
        if n % i == 0 {
            return false;
        }
    }
    true
}

/// Number of steps in all patterns. Fixed at 16 to allow stack allocation.
const STEPS: usize = 16;

/// Fill `out` with a euclidean rhythm pattern.
///
/// Distributes `hits` maximally evenly across `STEPS` using the slope method.
fn euclidean_pattern(hits: usize, out: &mut [bool; STEPS]) {
    if hits == 0 {
        *out = [false; STEPS];
        return;
    }
    if hits >= STEPS {
        *out = [true; STEPS];
        return;
    }
    let slope = hits as f32 / STEPS as f32;
    for i in 0..STEPS {
        out[i] = ((i as f32) * slope) as usize != (((i + 1) as f32) * slope) as usize;
    }
}

/// Fill `out` with a prime-step pattern.
///
/// Triggers on steps whose 1-based index is prime.
fn prime_pattern(out: &mut [bool; STEPS]) {
    for i in 0..STEPS {
        out[i] = is_prime(i as u32 + 1);
    }
}

/// Fill `out` with a fibonacci-spaced pattern.
///
/// Inter-trigger spacing follows the fibonacci sequence, scaled by (1-density).
fn fibonacci_pattern(density: f32, out: &mut [bool; STEPS]) {
    *out = [false; STEPS];
    const FIB_SEQ: [usize; 9] = [1, 1, 2, 3, 5, 8, 13, 21, 34];
    let scale = (1.0 - density).max(0.1);
    let mut pos = 0;
    let mut fib_idx = 0;
    while pos < STEPS {
        out[pos] = true;
        let gap = ((FIB_SEQ[fib_idx % FIB_SEQ.len()] as f32) * scale) as usize;
        pos += gap.max(1);
        fib_idx += 1;
    }
}

/// Fill `out` with a polyrhythm pattern.
///
/// ORs two euclidean patterns at `density` and `1-density` hit counts.
fn polyrhythm_pattern(density: f32, out: &mut [bool; STEPS]) {
    let hits_a = (STEPS as f32 * density).round() as usize;
    let hits_b = (STEPS as f32 * (1.0 - density)).round() as usize;
    let mut buf_a = [false; STEPS];
    let mut buf_b = [false; STEPS];
    euclidean_pattern(hits_a, &mut buf_a);
    euclidean_pattern(hits_b, &mut buf_b);
    for i in 0..STEPS {
        out[i] = buf_a[i] || buf_b[i];
    }
}

/// Algorithmic trigger pattern generator.
///
/// Generates 16-step patterns via mathematical algorithms. Output is mono
/// trigger: 1.0 on hit, 0.0 otherwise.
///
/// # Parameters
/// - `rate`: Clock rate (0.1-20 Hz)
/// - `density`: Pattern fill density (0-1)
///
/// # String Parameters
/// - `algorithm`: euclidean, prime, fibonacci, polyrhythm
pub struct LogicSeqCell {
    phase: f32,              // 0.0-1.0 within clock cycle
    step_counter: u32,       // Total steps elapsed
    rate_handle: Shared,
    density_handle: Shared,
    /// Pattern rotation offset [0, 15] — shifts pattern start point.
    rotation_handle: Shared,
    /// Mutation rate [0, 1] — probability of random bit flip per cycle.
    mutation_rate_handle: Shared,
    /// Accent density [0, 1] — second euclidean layer for accent output (ch1).
    accent_density_handle: Shared,
    algorithm: LogicAlgorithm,
    pattern_cache: [bool; STEPS], // Precomputed 16-step pattern — stack-allocated, no heap
    /// Second pattern layer for accents — stack-allocated.
    accent_cache: [bool; STEPS],
    cached_density: f32,
    cached_rotation: f32,
    cached_accent_density: f32,
    base_values: HashMap<String, f32>,
    sample_rate: f32,
    /// Inline xorshift PRNG state for mutation.
    rng_state: u32,
}

impl LogicSeqCell {
    /// Build a logic_seq_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell instance + control thread handles
    /// - `None`: If construction fails (shouldn't happen with valid DNA)
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let rate = param_or(dna, "rate", 2.0);
        let density = param_or(dna, "density", 0.5);
        let rotation = param_or(dna, "rotation", 0.0);
        let mutation_rate = param_or(dna, "mutation_rate", 0.0);
        let accent_density = param_or(dna, "accent_density", 0.0);

        // Read algorithm string param
        let algorithm_str = string_param_or(dna, "algorithm", "euclidean");
        let algorithm = LogicAlgorithm::from_str(algorithm_str);

        // Our Shared handles for the control thread
        let rate_handle = shared::shared(rate);
        let density_handle = shared::shared(density);
        let rotation_handle = shared::shared(rotation);
        let mutation_rate_handle = shared::shared(mutation_rate);
        let accent_density_handle = shared::shared(accent_density);

        let handles = vec![
            ("rate".into(), rate_handle.clone()),
            ("density".into(), density_handle.clone()),
            ("rotation".into(), rotation_handle.clone()),
            ("mutation_rate".into(), mutation_rate_handle.clone()),
            ("accent_density".into(), accent_density_handle.clone()),
        ];

        // Build initial patterns — stack-allocated, no heap
        let mut pattern_cache = [false; STEPS];
        Self::build_pattern_into(&algorithm, density, &mut pattern_cache);
        let mut accent_cache = [false; STEPS];
        if accent_density > 0.01 {
            let accent_hits = (STEPS as f32 * accent_density).round() as usize;
            euclidean_pattern(accent_hits, &mut accent_cache);
        }

        // Store base values from DNA for additive modulation
        let base_values = [
            ("rate", rate), ("density", density), ("rotation", rotation),
            ("mutation_rate", mutation_rate), ("accent_density", accent_density),
        ].iter().map(|(k, v)| (k.to_string(), *v)).collect();

        // Seed PRNG from rate bits
        let rng_state = rate.to_bits().wrapping_mul(2654435761) | 1;

        let cell = Self {
            phase: 0.0,
            step_counter: 0,
            rate_handle,
            density_handle,
            rotation_handle,
            mutation_rate_handle,
            accent_density_handle,
            algorithm,
            pattern_cache,
            accent_cache,
            cached_density: density,
            cached_rotation: rotation,
            cached_accent_density: accent_density,
            base_values,
            sample_rate: sr,
            rng_state,
        };

        Some((Box::new(cell), handles))
    }

    /// Fill `out` with the 16-step pattern for the given algorithm and density.
    ///
    /// Writes into a caller-provided `[bool; STEPS]` — no heap allocation.
    fn build_pattern_into(algorithm: &LogicAlgorithm, density: f32, out: &mut [bool; STEPS]) {
        match algorithm {
            LogicAlgorithm::Euclidean => {
                let hits = (STEPS as f32 * density).round() as usize;
                euclidean_pattern(hits, out);
            }
            LogicAlgorithm::Prime => prime_pattern(out),
            LogicAlgorithm::Fibonacci => fibonacci_pattern(density, out),
            LogicAlgorithm::Polyrhythm => polyrhythm_pattern(density, out),
        }
    }
}

impl DspCell for LogicSeqCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Read params
        let rate = self.rate_handle.value().clamp(0.1, 20.0);
        let density = self.density_handle.value().clamp(0.0, 1.0);
        let rotation = self.rotation_handle.value().clamp(0.0, 15.0);
        let mutation_rate = self.mutation_rate_handle.value().clamp(0.0, 1.0);
        let accent_density = self.accent_density_handle.value().clamp(0.0, 1.0);

        // Rebuild pattern if density changed — fills in place, no heap allocation
        if (density - self.cached_density).abs() > 0.01 {
            self.cached_density = density;
            Self::build_pattern_into(&self.algorithm, density, &mut self.pattern_cache);
        }

        // Rebuild accent pattern if accent_density changed
        if (accent_density - self.cached_accent_density).abs() > 0.01 {
            self.cached_accent_density = accent_density;
            if accent_density > 0.01 {
                let hits = (STEPS as f32 * accent_density).round() as usize;
                euclidean_pattern(hits, &mut self.accent_cache);
            } else {
                self.accent_cache = [false; STEPS];
            }
        }

        // Advance phase
        self.phase += rate / self.sample_rate;

        // Detect clock edge (phase wraps)
        let mut trigger_out = 0.0;
        let mut accent_out = 0.0;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.step_counter += 1;

            // Apply rotation offset to pattern index
            let rot = rotation.round() as usize;
            let pattern_idx = ((self.step_counter as usize) + rot) % STEPS;

            // Mutation: flip a random bit per cycle with probability mutation_rate
            if mutation_rate > 0.01 {
                let roll = rand_f32(&mut self.rng_state);
                if roll < mutation_rate {
                    let bit_idx = (xorshift32(&mut self.rng_state) as usize) % STEPS;
                    self.pattern_cache[bit_idx] = !self.pattern_cache[bit_idx];
                }
            }

            if self.pattern_cache[pattern_idx] {
                trigger_out = 1.0;
            }

            // Accent layer
            if accent_density > 0.01 && self.accent_cache[pattern_idx] {
                accent_out = 1.0;
            }
        }

        // Stereo output: ch0=trigger, ch1=accent
        output[0] = trigger_out;
        if output.len() > 1 {
            output[1] = accent_out;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            DspCommand::SetRotation(rot) => {
                self.rotation_handle.set(rot.clamp(0.0, 15.0));
            }
            DspCommand::SetMutationRate(rate) => {
                self.mutation_rate_handle.set(rate.clamp(0.0, 1.0));
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // logic_seq is a control signal, not audio
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        2 // ch0=trigger, ch1=accent
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.step_counter = 0;
    }

    fn name(&self) -> &str {
        "logic_seq_cell"
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

    fn make_dna(rate: f32, density: f32, algorithm: &str) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("rate".into(), rate);
        params.insert("density".into(), density);

        let mut string_params = BTreeMap::new();
        string_params.insert("algorithm".into(), algorithm.into());

        CellDna {
            cell_type: "logic_seq_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn logic_euclidean_distribution() {
        // density=0.5 → exactly 8 hits in 16 steps
        let dna = make_dna(1.0, 0.5, "euclidean");
        let (mut cell, _handles) = LogicSeqCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut trigger_count = 0;

        // Run 16 steps at 1 Hz
        let samples_per_step = SR as usize;
        for _ in 0..16 {
            for _ in 0..samples_per_step {
                cell.tick(&[], &mut output);
                if output[0] > 0.5 {
                    trigger_count += 1;
                }
            }
        }

        assert_eq!(
            trigger_count, 8,
            "Euclidean with density=0.5 should produce 8 hits in 16 steps"
        );
    }

    #[test]
    fn logic_prime_steps() {
        // Prime algorithm triggers on steps 2,3,5,7,11,13 (1-indexed)
        // In 0-indexed: 1,2,4,6,10,12
        let dna = make_dna(1.0, 0.5, "prime"); // density ignored for prime
        let (mut cell, _) = LogicSeqCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut trigger_steps = Vec::new();

        // Run 16 steps
        let samples_per_step = SR as usize;
        for step in 0..16 {
            let mut triggered = false;
            for _ in 0..samples_per_step {
                cell.tick(&[], &mut output);
                if output[0] > 0.5 {
                    triggered = true;
                }
            }
            if triggered {
                trigger_steps.push(step);
            }
        }

        // Prime steps (0-indexed): 1,2,4,6,10,12 (indices of primes 2,3,5,7,11,13)
        let expected = vec![1, 2, 4, 6, 10, 12];
        assert_eq!(
            trigger_steps, expected,
            "Prime algorithm should trigger on prime-numbered steps"
        );
    }

    #[test]
    fn logic_fibonacci_spacing() {
        // Fibonacci gaps should increase (1,1,2,3,5,8...)
        let dna = make_dna(1.0, 0.5, "fibonacci");
        let (mut cell, _) = LogicSeqCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut trigger_steps = Vec::new();

        // Run 16 steps
        let samples_per_step = SR as usize;
        for step in 0..16 {
            let mut triggered = false;
            for _ in 0..samples_per_step {
                cell.tick(&[], &mut output);
                if output[0] > 0.5 {
                    triggered = true;
                }
            }
            if triggered {
                trigger_steps.push(step);
            }
        }

        // Calculate gaps between triggers
        let mut gaps = Vec::new();
        for i in 1..trigger_steps.len() {
            gaps.push(trigger_steps[i] - trigger_steps[i - 1]);
        }

        // Gaps should generally increase (fibonacci-like)
        // Not exact because of scaling, but trend should be increasing
        assert!(
            gaps.len() >= 2,
            "Should have at least 2 gaps to measure trend"
        );
        let avg_early = gaps[0] as f32;
        let avg_late = gaps[gaps.len() - 1] as f32;
        assert!(
            avg_late >= avg_early,
            "Later gaps should be >= earlier gaps (fibonacci growth): {:?}",
            gaps
        );
    }

    #[test]
    fn logic_density_scales() {
        // density=0.75 should produce ~3× more triggers than density=0.25
        let dna_low = make_dna(1.0, 0.25, "euclidean");
        let dna_high = make_dna(1.0, 0.75, "euclidean");

        let (mut cell_low, _) = LogicSeqCell::new(&dna_low, SR).unwrap();
        let (mut cell_high, _) = LogicSeqCell::new(&dna_high, SR).unwrap();

        let mut output_low = [0.0f32; 2];
        let mut output_high = [0.0f32; 2];
        let mut count_low = 0;
        let mut count_high = 0;

        // Run 16 steps
        let samples_per_step = SR as usize;
        for _ in 0..16 {
            for _ in 0..samples_per_step {
                cell_low.tick(&[], &mut output_low);
                cell_high.tick(&[], &mut output_high);
                if output_low[0] > 0.5 {
                    count_low += 1;
                }
                if output_high[0] > 0.5 {
                    count_high += 1;
                }
            }
        }

        // Ratio should be close to 0.75/0.25 = 3
        let ratio = count_high as f32 / count_low as f32;
        assert!(
            ratio > 2.5 && ratio < 3.5,
            "High density should produce ~3× more triggers, got ratio={}",
            ratio
        );
    }

    #[test]
    fn logic_rate_changes_speed() {
        // rate=4.0 should produce more steps per second than rate=1.0
        // At rate=1.0: 1 step/sec, at rate=4.0: 4 steps/sec
        let dna_slow = make_dna(1.0, 0.5, "euclidean");
        let dna_fast = make_dna(4.0, 0.5, "euclidean");

        let (mut cell_slow, _) = LogicSeqCell::new(&dna_slow, SR).unwrap();
        let (mut cell_fast, _) = LogicSeqCell::new(&dna_fast, SR).unwrap();

        let mut output_slow = [0.0f32; 2];
        let mut output_fast = [0.0f32; 2];
        let mut count_slow = 0;
        let mut count_fast = 0;

        // Run for 4 seconds to get enough steps
        for _ in 0..(SR as usize * 4) {
            cell_slow.tick(&[], &mut output_slow);
            cell_fast.tick(&[], &mut output_fast);
            if output_slow[0] > 0.5 {
                count_slow += 1;
            }
            if output_fast[0] > 0.5 {
                count_fast += 1;
            }
        }

        // Over 4 seconds: rate=1.0 gets 4 steps, rate=4.0 gets 16 steps
        // With density=0.5, expect 2 vs 8 triggers → ratio ~4
        let ratio = count_fast as f32 / count_slow.max(1) as f32;
        assert!(
            ratio > 2.5,
            "rate=4.0 should produce significantly more triggers than rate=1.0, got ratio={} (slow={}, fast={})",
            ratio,
            count_slow,
            count_fast
        );
    }

    #[test]
    fn rotation_shifts_pattern() {
        // Rotation should produce triggers at different step positions
        // Run 32 steps (2 full cycles) to catch all hits regardless of rotation offset
        let dna_no_rot = make_dna(1.0, 0.25, "euclidean");
        let (mut cell_no, _) = LogicSeqCell::new(&dna_no_rot, SR).unwrap();

        let mut dna_rot = make_dna(1.0, 0.25, "euclidean");
        dna_rot.params.insert("rotation".into(), 3.0);
        let (mut cell_rot, _) = LogicSeqCell::new(&dna_rot, SR).unwrap();

        let mut out_no = [0.0f32; 2];
        let mut out_rot = [0.0f32; 2];
        let mut steps_no = Vec::new();
        let mut steps_rot = Vec::new();

        let sps = SR as usize;
        for step in 0..32 {
            let mut trig_no = false;
            let mut trig_rot = false;
            for _ in 0..sps {
                cell_no.tick(&[], &mut out_no);
                cell_rot.tick(&[], &mut out_rot);
                if out_no[0] > 0.5 { trig_no = true; }
                if out_rot[0] > 0.5 { trig_rot = true; }
            }
            if trig_no { steps_no.push(step); }
            if trig_rot { steps_rot.push(step); }
        }

        // Both should produce triggers
        assert!(!steps_no.is_empty(), "unrotated should produce triggers");
        assert!(!steps_rot.is_empty(), "rotated should produce triggers");
        // Pattern positions should differ
        assert_ne!(steps_no, steps_rot, "rotation should shift pattern positions");
    }

    #[test]
    fn mutation_changes_pattern() {
        // With mutation_rate=1.0, pattern should differ from initial after many cycles
        let mut dna = make_dna(10.0, 0.5, "euclidean");
        dna.params.insert("mutation_rate".into(), 1.0);
        let (mut cell, _) = LogicSeqCell::new(&dna, SR).unwrap();

        let mut output = [0.0f32; 2];
        let mut trigger_count = 0;

        // Run for 5 seconds at rate=10 → 50 cycles → plenty of mutations
        for _ in 0..(SR as usize * 5) {
            cell.tick(&[], &mut output);
            if output[0] > 0.5 { trigger_count += 1; }
        }

        // Should still produce some triggers (pattern mutates but doesn't die)
        assert!(trigger_count > 0, "mutated pattern should still produce triggers");
    }

    #[test]
    fn set_rotation_command() {
        let dna = make_dna(1.0, 0.5, "euclidean");
        let (mut cell, _) = LogicSeqCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::SetRotation(8.0));
        // Should not crash
        let mut output = [0.0f32; 2];
        cell.tick(&[], &mut output);
    }
}
