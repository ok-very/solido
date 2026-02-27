use crate::dsp::shared::{shared, Shared};

use crate::dsp::atom::envelopes::ClockAtom;
use crate::dsp::atom::DspAtom;
use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::organism::dna::CellDna;

/// Arpeggiator pattern modes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArpPattern {
    Up,
    Down,
    UpDown,
    Random,
    Converge,
}

impl ArpPattern {
    fn from_float(v: f32) -> Self {
        match v.round() as i32 {
            0 => Self::Up,
            1 => Self::Down,
            2 => Self::UpDown,
            3 => Self::Random,
            _ => Self::Converge,
        }
    }
}

/// Pitch/gate event produced by the arpeggiator.
#[derive(Debug, Clone, Copy)]
pub struct ArpEvent {
    pub freq: f32,
    pub gate_on: bool,
    pub velocity: f32,
}

/// MELO arpeggiator sequencer cell.
///
/// Owns ClockAtom + pattern logic.
/// Produces pitch/gate events for TimbreVoice.
/// Output: mono (1ch) — gate output (1.0 on, 0.0 off).
pub struct Arpeggiator {
    clock: ClockAtom,
    rate_hz: Shared,
    pattern: Shared,
    octaves: Shared,
    gate_length: Shared,
    swing: Shared,
    /// Base notes in the arp chord (MIDI-style pitch values or Hz).
    /// Default: C minor triad relative to root.
    notes: Vec<f32>,
    /// Current step index.
    step_index: usize,
    /// Direction for UpDown pattern (true = ascending).
    ascending: bool,
    /// Pending events.
    pending_events: Vec<ArpEvent>,
    /// Gate timer: counts down samples until gate off.
    gate_timer: u32,
    /// Samples per step.
    samples_per_step: f32,
    /// Current note frequency.
    current_freq: f32,
    sample_rate: f32,
    /// Simple RNG state for Random pattern.
    rng_state: u32,
}

impl Arpeggiator {
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let rate = param_or(dna, "rate_hz", 4.0);
        let pat = param_or(dna, "pattern", 0.0);
        let oct = param_or(dna, "octaves", 1.0);
        let gate = param_or(dna, "gate_length", 0.5);
        let sw = param_or(dna, "swing", 0.0);

        let rate_hz = shared(rate);
        let pattern = shared(pat);
        let octaves = shared(oct);
        let gate_length = shared(gate);
        let swing = shared(sw);

        // Use rate_hz directly as BPM equivalent (rate_hz steps per second)
        let bpm = rate * 60.0; // convert steps/sec to bpm
        let clock = ClockAtom::new(bpm, 1.0, sr);

        // Default arp notes: minor triad relative to 261.63 Hz (middle C)
        let notes = vec![261.63, 311.13, 392.00]; // C4, Eb4, G4

        let samples_per_step = sr / rate;

        let handles = vec![
            ("rate_hz".into(), rate_hz.clone()),
            ("pattern".into(), pattern.clone()),
            ("octaves".into(), octaves.clone()),
            ("gate_length".into(), gate_length.clone()),
            ("swing".into(), swing.clone()),
        ];

        let cell = Arpeggiator {
            clock,
            rate_hz,
            pattern,
            octaves,
            gate_length,
            swing,
            notes,
            step_index: 0,
            ascending: true,
            pending_events: Vec::with_capacity(4),
            gate_timer: 0,
            samples_per_step,
            current_freq: 261.63,
            sample_rate: sr,
            rng_state: 42,
        };

        Some((Box::new(cell), handles))
    }

    /// Drain pending arp events.
    pub fn drain_events(&mut self) -> impl Iterator<Item = ArpEvent> + '_ {
        self.pending_events.drain(..)
    }

    /// Set the chord notes for the arpeggiator.
    pub fn set_notes(&mut self, notes: Vec<f32>) {
        self.notes = notes;
        self.step_index = 0;
        self.ascending = true;
    }

    fn next_note(&mut self) -> f32 {
        if self.notes.is_empty() {
            return 261.63;
        }

        let octaves = self.octaves.value().round() as usize;
        let octaves = octaves.max(1).min(3);

        // Build expanded note list with octaves
        let total_notes = self.notes.len() * octaves;
        let pattern = ArpPattern::from_float(self.pattern.value());

        let idx = match pattern {
            ArpPattern::Up => {
                let i = self.step_index % total_notes;
                self.step_index = (self.step_index + 1) % total_notes;
                i
            }
            ArpPattern::Down => {
                let i = (total_notes - 1) - (self.step_index % total_notes);
                self.step_index = (self.step_index + 1) % total_notes;
                i
            }
            ArpPattern::UpDown => {
                let i = self.step_index % total_notes;
                if self.ascending {
                    if i == total_notes - 1 {
                        self.ascending = false;
                    }
                } else if i == 0 {
                    self.ascending = true;
                }
                if self.ascending {
                    self.step_index = (self.step_index + 1) % total_notes;
                } else {
                    self.step_index = if self.step_index == 0 {
                        total_notes - 1
                    } else {
                        self.step_index - 1
                    };
                }
                i
            }
            ArpPattern::Random => {
                // Simple xorshift RNG
                self.rng_state ^= self.rng_state << 13;
                self.rng_state ^= self.rng_state >> 17;
                self.rng_state ^= self.rng_state << 5;
                (self.rng_state as usize) % total_notes
            }
            ArpPattern::Converge => {
                // Alternates between bottom and top, working inward
                let half = total_notes / 2;
                let cycle = self.step_index % total_notes;
                let i = if cycle % 2 == 0 {
                    cycle / 2
                } else {
                    total_notes - 1 - cycle / 2
                };
                self.step_index = (self.step_index + 1) % total_notes;
                i.min(total_notes - 1)
            }
        };

        // Map index to note + octave
        let base_idx = idx % self.notes.len();
        let octave = idx / self.notes.len();
        self.notes[base_idx] * (1 << octave) as f32
    }
}

impl DspCell for Arpeggiator {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        // Update clock rate from shared
        let rate = self.rate_hz.value().max(0.1);
        let bpm = rate * 60.0;
        self.clock.set_param("bpm", bpm);
        self.samples_per_step = self.sample_rate / rate;

        // Tick clock
        let mut clock_out = [0.0f32];
        self.clock.tick(&[], &mut clock_out);

        // Process gate timer
        if self.gate_timer > 0 {
            self.gate_timer -= 1;
            if self.gate_timer == 0 {
                // Gate off
                self.pending_events.push(ArpEvent {
                    freq: self.current_freq,
                    gate_on: false,
                    velocity: 0.0,
                });
            }
        }

        output[0] = if self.gate_timer > 0 { 1.0 } else { 0.0 };

        if clock_out[0] > 0.5 {
            // New step: pick next note
            let freq = self.next_note();
            self.current_freq = freq;

            // Gate on
            self.pending_events.push(ArpEvent {
                freq,
                gate_on: true,
                velocity: 0.8,
            });

            // Set gate timer
            let gate_len = self.gate_length.value().clamp(0.1, 1.0);
            self.gate_timer = (self.samples_per_step * gate_len) as u32;

            output[0] = 1.0;
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { freq, .. } => {
                // Use NoteOn to set root pitch — rebuild chord from it
                let root = *freq;
                self.notes = vec![root, root * 1.189, root * 1.498]; // minor third + fifth
            }
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
        self.ascending = true;
        self.pending_events.clear();
        self.gate_timer = 0;
    }

    fn name(&self) -> &str {
        "arpeggiator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("rate_hz".into(), 4.0);
        params.insert("pattern".into(), 0.0);
        params.insert("octaves".into(), 1.0);
        params.insert("gate_length".into(), 0.5);
        CellDna {
            cell_type: "arpeggiator".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn arpeggiator_produces_events() {
        let (cell_box, _) = Arpeggiator::from_dna(&make_dna(), SR).unwrap();
        // Downcast to access drain_events
        let mut cell = cell_box;

        let mut total_gates = 0u32;
        let mut out = [0.0f32];
        for _ in 0..(SR as usize * 2) {
            cell.tick(&[], &mut out);
            if out[0] > 0.5 {
                total_gates += 1;
            }
        }

        // At 4 Hz, 2 seconds should produce ~8 gate events
        // Each gate lasts samples_per_step * 0.5 samples
        assert!(
            total_gates > 100, // gate is held for multiple samples
            "Arpeggiator should produce gates: got {} gate-on samples",
            total_gates
        );
    }

    #[test]
    fn arpeggiator_is_mono() {
        let (cell, _) = Arpeggiator::from_dna(&make_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }
}
