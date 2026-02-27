use crate::dsp::shared::{shared, Shared};

use crate::dsp::atom::envelopes::ClockAtom;
use crate::dsp::atom::DspAtom;
use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// Trigger event produced by PatternGen for downstream cells.
#[derive(Debug, Clone, Copy)]
pub struct TriggerEvent {
    pub velocity: f32,
    pub accent: bool,
}

/// TBLK rhythm sequencer cell.
///
/// Owns ClockAtom + euclidean pattern logic.
/// Produces trigger events (not audio output).
/// Output: mono (1ch) — outputs trigger pulses (1.0 on trigger, 0.0 otherwise).
pub struct PatternGen {
    clock: ClockAtom,
    bpm: Shared,
    steps: Shared,
    hits: Shared,
    accent_depth: Shared,
    swing: Shared,
    /// Current pattern (true = hit, false = rest)
    pattern: Vec<bool>,
    /// Accent pattern
    accents: Vec<bool>,
    step_index: usize,
    pending_triggers: Vec<TriggerEvent>,
    sample_rate: f32,
    /// Swing: delay even steps by swing amount of step duration
    swing_counter: f32,
    swing_delay_samples: f32,
    waiting_for_swing: bool,
}

impl PatternGen {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let bpm_val = param_or(dna, "bpm", 120.0);
        let steps_val = param_or(dna, "steps", 8.0);
        let hits_val = param_or(dna, "hits", 5.0);
        let accent_val = param_or(dna, "accent_depth", 0.6);
        let swing_val = param_or(dna, "swing", 0.0);

        let bpm = shared(bpm_val);
        let steps = shared(steps_val);
        let hits = shared(hits_val);
        let accent_depth = shared(accent_val);
        let swing = shared(swing_val);

        let steps_i = (steps_val as usize).max(1);
        let hits_i = (hits_val as usize).min(steps_i);
        let pattern = euclidean_pattern(steps_i, hits_i);
        let accents = compute_accents(steps_i);

        let clock = ClockAtom::new(bpm_val, 1.0, sr);

        let handles = vec![
            ("bpm".into(), bpm.clone()),
            ("steps".into(), steps.clone()),
            ("hits".into(), hits.clone()),
            ("accent_depth".into(), accent_depth.clone()),
            ("swing".into(), swing.clone()),
        ];

        let cell = PatternGen {
            clock,
            bpm,
            steps,
            hits,
            accent_depth,
            swing,
            pattern,
            accents,
            step_index: 0,
            pending_triggers: Vec::with_capacity(4),
            sample_rate: sr,
            swing_counter: 0.0,
            swing_delay_samples: 0.0,
            waiting_for_swing: false,
        };

        Some((Box::new(cell), handles))
    }

    /// Drain pending trigger events.
    pub fn drain_triggers(&mut self) -> impl Iterator<Item = TriggerEvent> + '_ {
        self.pending_triggers.drain(..)
    }
}

impl DspCell for PatternGen {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Update clock BPM from shared handle
        let current_bpm = self.bpm.value();
        self.clock.set_param("bpm", current_bpm);

        // Rebuild pattern if steps/hits changed
        let steps_i = (self.steps.value() as usize).max(1).min(16);
        let hits_i = (self.hits.value() as usize).min(steps_i);
        if self.pattern.len() != steps_i {
            self.pattern = euclidean_pattern(steps_i, hits_i);
            self.accents = compute_accents(steps_i);
            self.step_index = self.step_index % steps_i;
        }

        // Tick clock
        let mut clock_out = [0.0f32];
        self.clock.tick(&[], &mut clock_out);

        output[0] = 0.0;

        if clock_out[0] > 0.5 {
            // Clock fired — advance step
            let step = self.step_index % self.pattern.len();

            if self.pattern[step] {
                let accent = self.accents[step];
                let accent_d = self.accent_depth.value();
                let velocity = if accent {
                    1.0
                } else {
                    1.0 - accent_d
                };

                self.pending_triggers.push(TriggerEvent { velocity, accent });
                output[0] = velocity;
            }

            self.step_index = (self.step_index + 1) % self.pattern.len();
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => self.reset(),
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        DspAnalysis {
            rms: 0.0,
            peak: 0.0,
        }
    }

    fn output_channels(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.clock.reset();
        self.step_index = 0;
        self.pending_triggers.clear();
    }

    fn name(&self) -> &str {
        "pattern_gen"
    }
}

/// Bjorklund/Euclidean rhythm pattern generator.
/// Distributes `hits` as evenly as possible across `steps`.
fn euclidean_pattern(steps: usize, hits: usize) -> Vec<bool> {
    if steps == 0 {
        return vec![];
    }
    let hits = hits.min(steps);
    let mut pattern = vec![false; steps];

    if hits == 0 {
        return pattern;
    }
    if hits == steps {
        return vec![true; steps];
    }

    // Bresenham-style distribution
    let mut bucket = 0i32;
    for i in 0..steps {
        bucket += hits as i32;
        if bucket >= steps as i32 {
            bucket -= steps as i32;
            pattern[i] = true;
        }
    }

    pattern
}

/// Simple accent pattern: first beat and every 4th beat get accent.
fn compute_accents(steps: usize) -> Vec<bool> {
    (0..steps).map(|i| i == 0 || i % 4 == 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), 120.0);
        params.insert("steps".into(), 8.0);
        params.insert("hits".into(), 5.0);
        params.insert("accent_depth".into(), 0.6);
        CellDna {
            cell_type: "pattern_gen".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn euclidean_5_of_8() {
        let pattern = euclidean_pattern(8, 5);
        let count = pattern.iter().filter(|&&b| b).count();
        assert_eq!(count, 5, "Should have 5 hits in 8 steps");
    }

    #[test]
    fn euclidean_0_of_8() {
        let pattern = euclidean_pattern(8, 0);
        let count = pattern.iter().filter(|&&b| b).count();
        assert_eq!(count, 0);
    }

    #[test]
    fn euclidean_8_of_8() {
        let pattern = euclidean_pattern(8, 8);
        let count = pattern.iter().filter(|&&b| b).count();
        assert_eq!(count, 8);
    }

    #[test]
    fn pattern_gen_produces_triggers() {
        let (mut cell, _) = PatternGen::from_dna(&make_dna(), SR).unwrap();

        // Run for 2 seconds at 120 BPM = ~4 beats = should hit several pattern steps
        let mut trigger_count = 0;
        let mut out = [0.0f32];
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut out);
            if out[0] > 0.1 {
                trigger_count += 1;
            }
        }

        assert!(
            trigger_count > 0,
            "PatternGen should produce triggers over 2 seconds"
        );
    }

    #[test]
    fn pattern_gen_is_mono() {
        let (cell, _) = PatternGen::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
