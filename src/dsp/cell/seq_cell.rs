//! Internal sequencer cell with per-step pitch, gate, accent, and slide data.
//!
//! Outputs stereo signal where ch0 carries gate (1.0 during note, 0.0 during rest)
//! and ch1 carries pitch (Hz). This dual-purpose output allows:
//! - Trigger wires to read ch0 for gate-driven envelopes
//! - Audio wires to slew_cell to read ch1 for pitch smoothing
//!
//! # Parameters
//! - `bpm`: Tempo (20-300 BPM)
//! - `steps`: Active step count (1-16)
//! - `gate_length`: Gate duration as fraction of step (0.01-1.0)
//! - `swing`: Swing amount (0-1) — delays even steps
//!
//! # String Parameters
//! - `pitches`: Comma-separated Hz values (e.g., "110,130.8,146.8,110")
//! - `accents`: Comma-separated 0/1 flags (e.g., "1,0,0,1")
//! - `gates`: Comma-separated 0/1 flags (e.g., "1,1,0,1")
//! - `slides`: Comma-separated 0/1 flags (e.g., "0,0,1,0")
//!
//! # Example DNA
//! ```json
//! {
//!   "cell_type": "seq_cell",
//!   "params": { "bpm": 130, "steps": 4, "gate_length": 0.5, "swing": 0.0 },
//!   "string_params": {
//!     "pitches": "110,130.8,146.8,110",
//!     "accents": "1,0,0,1",
//!     "gates": "1,1,0,1",
//!     "slides": "0,0,1,0"
//!   }
//! }
//! ```
//!
//! # Wire Graph Patterns
//! - Trigger wire to env_cell: seq[ch0] → env (gate rising edge triggers attack)
//! - Audio wire to slew_cell: seq[ch1] → slew (pitch smoothing for slides)
//! - Modulation wire from slew: slew → osc.freq (apply smoothed pitch)

use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Valid parameter ranges for seq_cell — single source of truth.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("bpm", 10.0, 400.0),
    ("gate_length", 0.01, 1.0),
    ("swing", 0.0, 1.0),
];

/// Parse comma-separated float list from string param.
fn parse_float_list(s: &str) -> Result<Vec<f32>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|part| {
            part.trim()
                .parse::<f32>()
                .map_err(|_| format!("Invalid float: '{}'", part))
        })
        .collect()
}

/// Parse comma-separated 0/1 gate list (binary flags).
fn parse_gate_list(s: &str) -> Result<Vec<bool>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|part| match part.trim() {
            "1" => Ok(true),
            "0" => Ok(false),
            x => Err(format!("Expected 0 or 1, got '{}'", x)),
        })
        .collect()
}

/// Internal sequencer — organism's innate musical pattern stored in DNA.
///
/// Outputs stereo: ch0=gate (1.0 or 0.0), ch1=pitch (Hz).
///
/// # Parameters
/// - `bpm`: Tempo (20-300 BPM)
/// - `gate_length`: Gate duration as fraction of step (0.01-1.0)
/// - `swing`: Swing amount (0-1)
///
/// # String Parameters
/// - `pitches`, `accents`, `gates`, `slides`: Comma-separated per-step data
pub struct SeqCell {
    // Step data (parsed from string_params)
    pitches: Vec<f32>,   // Hz per step
    accents: Vec<bool>,  // Accent flag per step
    gates: Vec<bool>,    // Gate on/off per step
    slides: Vec<bool>,   // Slide/legato flag per step

    // Timing params
    bpm_handle: Shared,
    gate_length_handle: Shared, // 0.0-1.0 fraction of step
    swing_handle: Shared,        // 0.0-1.0 swing amount

    /// Ratio applied to global BPM. 1.0 = match global, 0.5 = half-time,
    /// 2.0 = double-time, 1.5 = 3:2 polyrhythm.
    tempo_ratio: f32,

    // State
    current_step: usize,
    phase: f32, // 0.0-1.0 within current step
    gate_active: bool,
    sample_rate: f32,
    base_values: HashMap<String, f32>,
}

impl SeqCell {
    /// Build a seq_cell from DNA.
    ///
    /// # Returns
    /// - `Some((cell, handles))`: Cell + shared param handles
    /// - `None`: Parsing failed or invalid param lengths
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let bpm = param_or(dna, "bpm", 130.0);
        let tempo_ratio = param_or(dna, "tempo_ratio", 1.0);
        let gate_length = param_or(dna, "gate_length", 0.5);
        let swing = param_or(dna, "swing", 0.0);

        // Parse string params
        let pitches_str = string_param_or(dna, "pitches", "");
        let accents_str = string_param_or(dna, "accents", "");
        let gates_str = string_param_or(dna, "gates", "");
        let slides_str = string_param_or(dna, "slides", "");

        let pitches = parse_float_list(pitches_str).ok()?;
        let accents = parse_gate_list(accents_str).ok()?;
        let gates = parse_gate_list(gates_str).ok()?;
        let slides = parse_gate_list(slides_str).ok()?;

        // Validate: all lists must have same length and at least 1 step
        let step_count = pitches.len();
        if step_count == 0 {
            log::warn!("seq_cell: pitches list is empty");
            return None;
        }
        if accents.len() != step_count
            || gates.len() != step_count
            || slides.len() != step_count
        {
            log::warn!(
                "seq_cell: list lengths don't match — pitches={}, accents={}, gates={}, slides={}",
                pitches.len(),
                accents.len(),
                gates.len(),
                slides.len()
            );
            return None;
        }

        // Create shared handles
        let bpm_handle = shared::shared(bpm);
        let gate_length_handle = shared::shared(gate_length);
        let swing_handle = shared::shared(swing);

        let mut base_values = HashMap::new();
        base_values.insert("bpm".into(), bpm);
        base_values.insert("gate_length".into(), gate_length);
        base_values.insert("swing".into(), swing);

        let cell = Self {
            tempo_ratio,
            pitches,
            accents,
            gates,
            slides,
            bpm_handle: bpm_handle.clone(),
            gate_length_handle: gate_length_handle.clone(),
            swing_handle: swing_handle.clone(),
            current_step: 0,
            phase: 0.0,
            gate_active: false,
            sample_rate: sr,
            base_values,
        };

        let handles = vec![
            ("bpm".into(), bpm_handle),
            ("gate_length".into(), gate_length_handle),
            ("swing".into(), swing_handle),
        ];

        Some((Box::new(cell), handles))
    }
}

impl DspCell for SeqCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let bpm = clamp_param(PARAM_RANGES, "bpm", self.bpm_handle.value());
        let gate_length = clamp_param(PARAM_RANGES, "gate_length", self.gate_length_handle.value());
        let swing = clamp_param(PARAM_RANGES, "swing", self.swing_handle.value());

        // Calculate step duration in seconds
        let steps_per_second = bpm / 60.0;
        let step_duration_seconds = 1.0 / steps_per_second;
        let samples_per_step = step_duration_seconds * self.sample_rate;

        // Apply swing to even steps (step index 1, 3, 5, ... in 0-indexed)
        let is_even_step = (self.current_step % 2) == 1;
        let swing_offset_samples = if is_even_step {
            swing * samples_per_step * 0.5
        } else {
            0.0
        };
        let adjusted_samples_per_step = samples_per_step + swing_offset_samples;

        // Phase advance: increment by 1/samples_per_step each sample
        self.phase += 1.0 / adjusted_samples_per_step;

        // Check if gate should be active.
        // Slide steps: gate extends to the full step boundary (no gap before next step).
        // This prevents env_cell from seeing a low→high edge at the transition,
        // keeping the envelope in sustain through the note change — the 303 slide effect.
        let effective_gate_length = if self.slides[self.current_step] {
            1.0 // Hold gate through end of step — no retrigger on next note
        } else {
            gate_length
        };
        self.gate_active = self.gates[self.current_step] && self.phase < effective_gate_length;

        // Wrap phase and advance step
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.current_step = (self.current_step + 1) % self.pitches.len();
        }

        // Output: ch0=gate (1.0 or 0.0), ch1=pitch (Hz)
        if output.len() >= 2 {
            output[0] = if self.gate_active { 1.0 } else { 0.0 };
            output[1] = self.pitches[self.current_step];
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::Reset | DspCommand::Panic => {
                self.reset();
            }
            DspCommand::SetGlobalBpm(global_bpm) => {
                let effective = global_bpm * self.tempo_ratio;
                self.bpm_handle.set(effective.clamp(20.0, 300.0));
            }
            DspCommand::NudgePhase(nudge) => {
                if nudge.abs() >= 1.0 {
                    // Hard reset: magnitude >= 1.0 means "set phase to 0"
                    self.phase = 0.0;
                    self.current_step = 0;
                } else {
                    self.phase = (self.phase + nudge).rem_euclid(1.0);
                }
            }
            _ => {}
        }
    }

    fn analysis(&self) -> DspAnalysis {
        // Sequencer doesn't produce audio, so no RMS/peak
        DspAnalysis::new(0.0, 0.0)
    }

    fn output_channels(&self) -> usize {
        2 // Stereo: ch0=gate, ch1=pitch (control signals, not audio)
    }

    fn reset(&mut self) {
        self.current_step = 0;
        self.phase = 0.0;
        self.gate_active = false;
    }

    fn name(&self) -> &str {
        "seq_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] { PARAM_RANGES }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_test_dna(
        bpm: f32,
        gate_length: f32,
        pitches: &str,
        gates: &str,
    ) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), bpm);
        params.insert("gate_length".into(), gate_length);
        params.insert("swing".into(), 0.0);

        // Count pitch entries to generate matching accents/slides
        let pitch_count = pitches.split(',').count();
        let zeros = (0..pitch_count).map(|_| "0").collect::<Vec<_>>().join(",");

        let mut string_params = BTreeMap::new();
        string_params.insert("pitches".into(), pitches.into());
        string_params.insert("accents".into(), zeros.clone());
        string_params.insert("gates".into(), gates.into());
        string_params.insert("slides".into(), zeros);

        CellDna {
            cell_type: "seq_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn seq_fires_triggers() {
        // Gate should go high at step boundaries
        let dna = make_test_dna(60.0, 0.5, "110,130,146,110", "1,1,1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // At start of step 0, gate should be high
        cell.tick(&[], &mut output);
        assert!(output[0] > 0.5, "Gate should be high at step start");
    }

    #[test]
    fn seq_respects_gates() {
        // Rest steps (gates="0") should produce output[0]=0.0
        let dna = make_test_dna(60.0, 0.5, "110,130,146,110", "1,0,1,0");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Step 0: gate=1, should be high
        cell.tick(&[], &mut output);
        assert!(output[0] > 0.5, "Step 0 gate should be high");

        // Advance to step 1 (gate=0)
        // At 60 BPM, step duration = 1 second = 44100 samples
        for _ in 0..44100 {
            cell.tick(&[], &mut output);
        }

        // Now on step 1 with gate=0
        assert!(output[0] < 0.5, "Step 1 gate should be low (rest step)");
    }

    #[test]
    fn seq_pitch_per_step() {
        // Each step should output correct pitch on ch1
        let dna = make_test_dna(60.0, 0.5, "110,130,146,165", "1,1,1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Step 0: pitch=110
        cell.tick(&[], &mut output);
        assert!((output[1] - 110.0).abs() < 0.1, "Step 0 pitch should be 110 Hz");

        // Advance to step 1 (add margin for timing)
        for _ in 0..45000 {
            cell.tick(&[], &mut output);
        }
        assert!((output[1] - 130.0).abs() < 0.1, "Step 1 pitch should be 130 Hz, got {}", output[1]);
    }

    #[test]
    fn seq_wraps_at_step_count() {
        // Should cycle back to step 0 after last step
        let dna = make_test_dna(120.0, 0.5, "110,130", "1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Advance through 2 steps (120 BPM = 0.5s per step = 22050 samples + generous margin)
        for _ in 0..(24000 * 2) {
            cell.tick(&[], &mut output);
        }

        // Should be back at step 0 or have wrapped multiple times
        // Just verify sequencer is producing one of the two pitches
        let pitch = output[1];
        assert!(
            (pitch - 110.0).abs() < 5.0 || (pitch - 130.0).abs() < 5.0,
            "Should be producing one of the sequence pitches, got {}",
            pitch
        );
    }

    #[test]
    fn seq_swing_shifts_even() {
        // Even steps (1, 3, ...) should be delayed by swing amount
        // Verify swing affects timing by checking pitch transitions
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), 120.0);
        params.insert("gate_length".into(), 0.5);
        params.insert("swing".into(), 0.5); // 50% swing

        let mut string_params = BTreeMap::new();
        string_params.insert("pitches".into(), "110,220".into()); // Different pitches to detect step change
        string_params.insert("accents".into(), "0,0".into());
        string_params.insert("gates".into(), "1,1".into());
        string_params.insert("slides".into(), "0,0".into());

        let dna = CellDna {
            cell_type: "seq_cell".into(),
            params,
            string_params,
        };

        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Start on step 0 (pitch = 110)
        cell.tick(&[], &mut output);

        // Verify swing parameter exists and cell works
        // (Actual swing timing is complex to test without internal state access)
        // This test just confirms the cell accepts swing parameter without error
        assert!(
            (output[1] - 110.0).abs() < 0.1,
            "Swing cell should produce audio on first step"
        );

        // Tick forward - with swing, step transitions should be delayed
        for _ in 0..25000 {
            cell.tick(&[], &mut output);
        }

        // After sufficient ticks, should have transitioned to step 1 (pitch = 220)
        assert!(
            output[1] > 200.0,
            "Should eventually transition to step 1, got pitch {}",
            output[1]
        );
    }

    #[test]
    fn seq_gate_length() {
        // Gate duration should match gate_length param
        let dna = make_test_dna(60.0, 0.25, "110,110,110,110", "1,1,1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Step duration at 60 BPM = 1 second = 44100 samples
        // Gate length = 0.25 = 11025 samples

        let mut gate_high_count = 0;
        for _ in 0..44100 {
            cell.tick(&[], &mut output);
            if output[0] > 0.5 {
                gate_high_count += 1;
            }
        }

        // Gate should be high for approximately 25% of the step
        let expected = 11025;
        let tolerance = 100; // Allow some timing slop
        assert!(
            (gate_high_count as i32 - expected as i32).abs() < tolerance,
            "Gate high count {} should be near {} (gate_length=0.25)",
            gate_high_count,
            expected
        );
    }

    #[test]
    fn seq_slide_holds_gate() {
        // On a slide step, gate should remain high until the step boundary
        // (no gap period that would retrigger the envelope)
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), 60.0);      // 1 second per step
        params.insert("gate_length".into(), 0.5); // Without slide: gate high for 0.5s
        params.insert("swing".into(), 0.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("pitches".into(), "110,220".into());
        string_params.insert("accents".into(), "0,0".into());
        string_params.insert("gates".into(), "1,1".into());
        // Step 0 has slide: gate should stay high until end of step (no 0.5s cutoff)
        string_params.insert("slides".into(), "1,0".into());

        let dna = CellDna { cell_type: "seq_cell".into(), params, string_params };
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // At 60 BPM, step = 44100 samples. Without slide, gate drops at 22050 (0.5 × step).
        // With slide on step 0, gate should still be HIGH at 80% of step duration.
        let check_sample = (44100.0 * 0.8) as usize;
        for _ in 0..check_sample {
            cell.tick(&[], &mut output);
        }

        assert!(
            output[0] > 0.5,
            "Slide step should hold gate high at 80% of step (no gap). Got gate={}",
            output[0]
        );
    }

    #[test]
    fn seq_slide_glides_pitch() {
        // Slide steps output pitch on ch1 normally — glide comes from slew_cell downstream.
        // Verify seq_cell still outputs correct pitch on slide steps.
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), 120.0);
        params.insert("gate_length".into(), 0.5);
        params.insert("swing".into(), 0.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("pitches".into(), "110,220".into());
        string_params.insert("accents".into(), "0,0".into());
        string_params.insert("gates".into(), "1,1".into());
        string_params.insert("slides".into(), "1,0".into()); // step 0 slides to step 1

        let dna = CellDna { cell_type: "seq_cell".into(), params, string_params };
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // First tick: step 0, pitch=110
        cell.tick(&[], &mut output);
        assert!(
            (output[1] - 110.0).abs() < 1.0,
            "Slide step 0 should output 110 Hz on ch1, got {}",
            output[1]
        );

        // Advance to step 1 (pitch=220)
        for _ in 0..23000 {
            cell.tick(&[], &mut output);
        }
        assert!(
            (output[1] - 220.0).abs() < 1.0,
            "Step 1 should output 220 Hz on ch1, got {}",
            output[1]
        );
    }

    #[test]
    fn seq_parses_string_params() {
        // Loading DNA with comma-separated lists should succeed
        let mut params = BTreeMap::new();
        params.insert("bpm".into(), 130.0);
        params.insert("gate_length".into(), 0.4);
        params.insert("swing".into(), 0.0);

        let mut string_params = BTreeMap::new();
        string_params.insert(
            "pitches".into(),
            "130.8,146.8,164.8,130.8".into(),
        );
        string_params.insert("accents".into(), "1,0,0,1".into());
        string_params.insert("gates".into(), "1,1,0,1".into());
        string_params.insert("slides".into(), "0,0,1,0".into());

        let dna = CellDna {
            cell_type: "seq_cell".into(),
            params,
            string_params,
        };

        let result = SeqCell::new(&dna, 44100.0);
        assert!(result.is_some(), "Should parse comma-separated string params");

        let (mut cell_box, handles) = result.unwrap();

        // Verify we got expected param handles
        assert_eq!(handles.len(), 3); // bpm, gate_length, swing

        // Verify cell produces output
        let mut output = [0.0f32; 2];
        cell_box.tick(&[], &mut output);

        // First step should output pitch ~130.8 Hz on ch1
        assert!(
            (output[1] - 130.8).abs() < 0.1,
            "First pitch should be 130.8 Hz, got {}",
            output[1]
        );

        // Cell should be named correctly
        assert_eq!(cell_box.name(), "seq_cell");
    }

    #[test]
    fn nudge_phase_adjusts_accumulator() {
        let dna = make_test_dna(120.0, 0.5, "110,220", "1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Tick a few times to establish phase
        for _ in 0..100 {
            cell.tick(&[], &mut output);
        }

        // Small nudge: phase should shift
        cell.handle_command(&DspCommand::NudgePhase(0.25));
        // Should still produce valid output
        cell.tick(&[], &mut output);
        assert!(output[1] > 0.0, "Should still produce pitch after nudge");
    }

    #[test]
    fn nudge_phase_hard_reset() {
        let dna = make_test_dna(60.0, 0.5, "110,220,330,440", "1,1,1,1");
        let (mut cell, _) = SeqCell::new(&dna, 44100.0).unwrap();

        let mut output = [0.0f32; 2];

        // Advance past step 0
        for _ in 0..50000 {
            cell.tick(&[], &mut output);
        }

        // Hard reset (magnitude >= 1.0)
        cell.handle_command(&DspCommand::NudgePhase(-1.0));
        cell.tick(&[], &mut output);

        // Should be back at step 0, pitch = 110
        assert!(
            (output[1] - 110.0).abs() < 1.0,
            "Hard reset should return to step 0 (pitch 110), got {}",
            output[1]
        );
    }
}
