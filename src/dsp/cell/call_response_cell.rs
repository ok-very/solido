//! Call-response cell — sawaal-jawaab (musical conversation) between human and organism.
//!
//! Captures a human-played MIDI phrase, detects phrase boundaries, then generates
//! a mutated response. Three phrase boundary modes create different conversational rhythms:
//! - **gap**: Silence gap after last NoteOff (natural phrasing, good for alap/taan)
//! - **beat**: Phrase ends on next beat boundary after last NoteOff (tala-locked)
//! - **fixed**: Fixed N-beat window from first NoteOn (strict antiphonal)
//!
//! # Output Convention
//! - ch0: gate (1.0 during response notes, 0.0 otherwise)
//! - ch1: pitch Hz (from response phrase with mutations)
//!
//! Compatible with Trigger wires to sample_cell, osc_cell, etc.

use std::any::Any;
use std::collections::HashMap;

use crate::dsp::cell::{param_or, string_param_or, DspCell, clamp_param};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::phrase_bank;
use crate::dsp::shared::{self, Shared};
use crate::organism::dna::CellDna;

/// Maximum notes in a captured phrase. 48 × 16 bytes = 768 bytes, stack-allocated.
const MAX_PHRASE_NOTES: usize = 48;

/// Valid parameter ranges for call_response_cell.
pub const PARAM_RANGES: &[(&str, f32, f32)] = &[
    ("chaos", 0.0, 1.0),
    ("gravity", 0.0, 1.0),
    ("gap_threshold", 0.1, 2.0),
    ("contour_fidelity", 0.0, 1.0),
    ("rhythm_jitter", 0.0, 1.0),
    ("response_tempo", 0.25, 4.0),
    ("max_pitch_deviation", 1.0, 12.0),
    ("drift_rate", 0.0, 0.5),
    ("insert_probability", 0.0, 0.5),
    ("delete_probability", 0.0, 0.5),
    ("post_silence", 0.05, 2.0),
    ("fixed_beats", 1.0, 16.0),
];

// ─── PRNG ────────────────────────────────────────────────────────────

#[inline]
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

#[inline]
fn rand_bipolar(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32) * 2.0 - 1.0
}

#[inline]
fn rand_f32(state: &mut u32) -> f32 {
    (xorshift32(state) as f32) / (u32::MAX as f32)
}

// ─── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CRState {
    Idle,
    Listen,
    Respond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhraseMode {
    Gap,
    Beat,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseMode {
    Forward,
    Mirror,
    Augment,
    Diminish,
}

/// A single captured note in the phrase buffer.
#[derive(Clone, Copy, Debug)]
struct CapturedNote {
    freq_hz: f32,
    velocity: f32,
    duration_samples: u32,
    ioi_samples: u32,
}

impl Default for CapturedNote {
    fn default() -> Self {
        Self { freq_hz: 0.0, velocity: 0.0, duration_samples: 0, ioi_samples: 0 }
    }
}

/// Fixed-size phrase buffer. RT-safe, no heap.
struct Phrase {
    notes: [CapturedNote; MAX_PHRASE_NOTES],
    len: u8,
}

impl Phrase {
    fn new() -> Self {
        Self {
            notes: [CapturedNote::default(); MAX_PHRASE_NOTES],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, note: CapturedNote) {
        if (self.len as usize) < MAX_PHRASE_NOTES {
            self.notes[self.len as usize] = note;
            self.len += 1;
        }
    }

    fn as_slice(&self) -> &[CapturedNote] {
        &self.notes[..self.len as usize]
    }
}

// ─── Variance tracking (RT-safe, no allocation) ─────────────────────

/// Compute pitch interval variance and rhythm ratio variance of a phrase.
/// Uses Welford's online algorithm — fixed iteration, no heap.
fn compute_phrase_variance(phrase: &Phrase) -> (f32, f32) {
    let len = phrase.len as usize;
    if len < 2 {
        return (0.0, 0.0);
    }

    let mut int_sum = 0.0f32;
    let mut int_sq_sum = 0.0f32;
    let mut ratio_sum = 0.0f32;
    let mut ratio_sq_sum = 0.0f32;
    let mut ratio_count = 0u32;

    for i in 1..len {
        // Pitch interval in semitones
        let interval = 12.0 * (phrase.notes[i].freq_hz / phrase.notes[i - 1].freq_hz).log2();
        if interval.is_finite() {
            int_sum += interval;
            int_sq_sum += interval * interval;
        }

        // Rhythm ratio (IOI[i] / IOI[i-1])
        let prev_ioi = phrase.notes[i - 1].ioi_samples as f32;
        if prev_ioi > 0.0 {
            let ratio = phrase.notes[i].ioi_samples as f32 / prev_ioi;
            ratio_sum += ratio;
            ratio_sq_sum += ratio * ratio;
            ratio_count += 1;
        }
    }

    let n = (len - 1) as f32;
    let pitch_var = (int_sq_sum / n) - (int_sum / n).powi(2);
    let rhythm_var = if ratio_count > 0 {
        let rn = ratio_count as f32;
        (ratio_sq_sum / rn) - (ratio_sum / rn).powi(2)
    } else {
        0.0
    };

    (pitch_var.max(0.0), rhythm_var.max(0.0))
}

// ─── Cell ────────────────────────────────────────────────────────────

pub struct CallResponseCell {
    state: CRState,
    phrase_mode: PhraseMode,
    response_mode: ResponseMode,

    // Capture state
    captured: Phrase,
    /// True while a note is held down during Listen
    note_held: bool,
    /// Current note's freq (during Listen)
    current_note_freq: f32,
    /// Current note's velocity (during Listen)
    current_note_vel: f32,
    /// Sample counter since current note started
    note_on_counter: u32,
    /// Sample counter since last NoteOn (for IOI measurement)
    last_note_on_sample: u32,
    /// Global sample counter (wrapping)
    sample_counter: u32,
    /// Samples since last NoteOff (for gap detection)
    silence_counter: u32,

    // Beat tracking (for beat/fixed modes)
    beat_phase: f32,
    bpm: f32,
    /// Sample counter since first NoteOn in fixed mode
    fixed_start_sample: u32,
    /// Whether we've started a fixed-length window
    fixed_active: bool,

    // Response state
    response: Phrase,
    /// Current index into response phrase
    response_idx: usize,
    /// Sample counter within current response note
    response_sample: u32,
    /// True while response note gate is high
    response_gate: bool,
    /// Current response pitch Hz
    response_pitch: f32,
    /// Velocity of current response note [0,1] — encodes dynamics into gate output
    response_velocity: f32,
    /// Post-response silence counter
    post_silence_counter: u32,
    /// Post-silence duration in samples
    post_silence_samples: u32,
    /// True when in post-response silence gap
    in_post_silence: bool,

    // Pass-through state (always active, regardless of CR state)
    /// Gate value for human pass-through (velocity when held, 0.0 when off).
    passthrough_gate: f32,
    /// Pitch Hz for human pass-through.
    passthrough_freq: f32,
    /// When true, NoteOn also starts capture. When false, pass-through only (noodling).
    listening: bool,

    // Auto-call / phrase bank
    /// Pre-loaded phrase bank (heap Vec, allocated once at construction, never resized).
    phrase_bank: Vec<Phrase>,
    /// Auto-call: self-seed from bank when Idle.
    auto_call: bool,
    /// Samples of idle silence before auto-call triggers.
    auto_call_delay_samples: u32,
    /// Counter for idle time (reset on NoteOn or state transition).
    idle_counter: u32,
    /// Whether auto-call "call" phrase plays audibly before the response.
    call_audible: bool,
    /// True while playing the "call" half of auto-conversation.
    auto_call_playing_call: bool,
    /// Mutation generation counter (increments per auto-call response cycle).
    generation: u32,
    /// Seed phrase pitch interval variance (baseline for entropy check).
    seed_pitch_variance: f32,
    /// Seed phrase rhythm ratio variance (baseline for entropy check).
    seed_rhythm_variance: f32,
    /// Variance ratio threshold that triggers re-seed from bank.
    reseed_threshold: f32,

    // Drift
    repetition_count: u32,

    // Scale gravity
    scale_weights: [f32; 12],

    // Shared handles
    chaos_handle: Shared,
    gravity_handle: Shared,
    gap_threshold_handle: Shared,
    contour_fidelity_handle: Shared,
    rhythm_jitter_handle: Shared,
    response_tempo_handle: Shared,
    max_pitch_deviation_handle: Shared,
    drift_rate_handle: Shared,
    insert_probability_handle: Shared,
    delete_probability_handle: Shared,
    post_silence_handle: Shared,
    fixed_beats_handle: Shared,

    sample_rate: f32,
    rng_state: u32,
    base_values: HashMap<String, f32>,
}

impl CallResponseCell {
    pub fn new(dna: &CellDna, sr: f32) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let chaos = param_or(dna, "chaos", 0.2);
        let gravity = param_or(dna, "gravity", 0.6);
        let gap_threshold = param_or(dna, "gap_threshold", 0.4);
        let contour_fidelity = param_or(dna, "contour_fidelity", 0.7);
        let rhythm_jitter = param_or(dna, "rhythm_jitter", 0.15);
        let response_tempo = param_or(dna, "response_tempo", 1.0);
        let max_pitch_deviation = param_or(dna, "max_pitch_deviation", 5.0);
        let drift_rate = param_or(dna, "drift_rate", 0.05);
        let insert_probability = param_or(dna, "insert_probability", 0.0);
        let delete_probability = param_or(dna, "delete_probability", 0.0);
        let post_silence = param_or(dna, "post_silence", 0.15);
        let fixed_beats = param_or(dna, "fixed_beats", 4.0);
        let bpm = param_or(dna, "bpm", 120.0);

        let phrase_mode = match string_param_or(dna, "phrase_mode", "gap") {
            "beat" => PhraseMode::Beat,
            "fixed" => PhraseMode::Fixed,
            _ => PhraseMode::Gap,
        };

        let response_mode = match string_param_or(dna, "response_mode", "forward") {
            "mirror" => ResponseMode::Mirror,
            "augment" => ResponseMode::Augment,
            "diminish" => ResponseMode::Diminish,
            _ => ResponseMode::Forward,
        };

        // Pass-through / auto-call DNA params
        let auto_listen = string_param_or(dna, "auto_listen", "true") == "true";
        let auto_call = string_param_or(dna, "auto_call", "false") == "true";
        let auto_call_delay = param_or(dna, "auto_call_delay", 2.0).max(0.1);
        let call_audible = string_param_or(dna, "call_audible", "true") == "true";
        let reseed_threshold = param_or(dna, "reseed_threshold", 0.4).clamp(0.05, 1.0);

        // Load phrase bank from JSON or MIDI
        let phrase_source = string_param_or(dna, "phrase_source", "");
        let base_dir = std::path::Path::new("assets/phrases");
        let phrase_data = phrase_bank::load_phrases(phrase_source, base_dir);
        let mut seed_pitch_variance = 0.0f32;
        let mut seed_rhythm_variance = 0.0f32;
        let loaded_bank: Vec<Phrase> = phrase_data
            .iter()
            .map(|pd| {
                let mut phrase = Phrase::new();
                for note in &pd.notes {
                    if (phrase.len as usize) < MAX_PHRASE_NOTES {
                        let freq_hz = phrase_bank::midi_to_hz(note.midi_note);
                        phrase.push(CapturedNote {
                            freq_hz,
                            velocity: note.velocity,
                            duration_samples: (note.duration_sec * sr).max(1.0) as u32,
                            ioi_samples: (note.ioi_sec * sr).max(1.0) as u32,
                        });
                    }
                }
                phrase
            })
            .collect();

        // Compute seed variance from first phrase (if any) for anti-convergence baseline
        if let Some(first) = loaded_bank.first() {
            let (pv, rv) = compute_phrase_variance(first);
            seed_pitch_variance = pv;
            seed_rhythm_variance = rv;
        }

        let chaos_handle = shared::shared(chaos);
        let gravity_handle = shared::shared(gravity);
        let gap_threshold_handle = shared::shared(gap_threshold);
        let contour_fidelity_handle = shared::shared(contour_fidelity);
        let rhythm_jitter_handle = shared::shared(rhythm_jitter);
        let response_tempo_handle = shared::shared(response_tempo);
        let max_pitch_deviation_handle = shared::shared(max_pitch_deviation);
        let drift_rate_handle = shared::shared(drift_rate);
        let insert_probability_handle = shared::shared(insert_probability);
        let delete_probability_handle = shared::shared(delete_probability);
        let post_silence_handle = shared::shared(post_silence);
        let fixed_beats_handle = shared::shared(fixed_beats);

        let rng_state = (sr as u32).wrapping_mul(2654435761).wrapping_add(
            chaos.to_bits().wrapping_mul(0x9e3779b9),
        ) | 1;

        let mut base_values = HashMap::new();
        base_values.insert("chaos".into(), chaos);
        base_values.insert("gravity".into(), gravity);
        base_values.insert("gap_threshold".into(), gap_threshold);
        base_values.insert("contour_fidelity".into(), contour_fidelity);
        base_values.insert("rhythm_jitter".into(), rhythm_jitter);
        base_values.insert("response_tempo".into(), response_tempo);
        base_values.insert("max_pitch_deviation".into(), max_pitch_deviation);
        base_values.insert("drift_rate".into(), drift_rate);
        base_values.insert("insert_probability".into(), insert_probability);
        base_values.insert("delete_probability".into(), delete_probability);
        base_values.insert("post_silence".into(), post_silence);
        base_values.insert("fixed_beats".into(), fixed_beats);

        let handles = vec![
            ("chaos".into(), chaos_handle.clone()),
            ("gravity".into(), gravity_handle.clone()),
            ("gap_threshold".into(), gap_threshold_handle.clone()),
            ("contour_fidelity".into(), contour_fidelity_handle.clone()),
            ("rhythm_jitter".into(), rhythm_jitter_handle.clone()),
            ("response_tempo".into(), response_tempo_handle.clone()),
            ("max_pitch_deviation".into(), max_pitch_deviation_handle.clone()),
            ("drift_rate".into(), drift_rate_handle.clone()),
            ("insert_probability".into(), insert_probability_handle.clone()),
            ("delete_probability".into(), delete_probability_handle.clone()),
            ("post_silence".into(), post_silence_handle.clone()),
            ("fixed_beats".into(), fixed_beats_handle.clone()),
        ];

        let cell = Self {
            state: CRState::Idle,
            phrase_mode,
            response_mode,
            captured: Phrase::new(),
            note_held: false,
            current_note_freq: 0.0,
            current_note_vel: 0.0,
            note_on_counter: 0,
            last_note_on_sample: 0,
            sample_counter: 0,
            silence_counter: 0,
            beat_phase: 0.0,
            bpm,
            fixed_start_sample: 0,
            fixed_active: false,
            response: Phrase::new(),
            response_idx: 0,
            response_sample: 0,
            response_gate: false,
            response_pitch: 0.0,
            response_velocity: 0.0,
            post_silence_counter: 0,
            post_silence_samples: 0,
            in_post_silence: false,
            passthrough_gate: 0.0,
            passthrough_freq: 0.0,
            listening: auto_listen,
            phrase_bank: loaded_bank,
            auto_call,
            auto_call_delay_samples: (auto_call_delay * sr) as u32,
            idle_counter: 0,
            call_audible,
            auto_call_playing_call: false,
            generation: 0,
            seed_pitch_variance,
            seed_rhythm_variance,
            reseed_threshold,
            repetition_count: 0,
            scale_weights: [0.0; 12],
            chaos_handle,
            gravity_handle,
            gap_threshold_handle,
            contour_fidelity_handle,
            rhythm_jitter_handle,
            response_tempo_handle,
            max_pitch_deviation_handle,
            drift_rate_handle,
            insert_probability_handle,
            delete_probability_handle,
            post_silence_handle,
            fixed_beats_handle,
            sample_rate: sr,
            rng_state,
            base_values,
        };

        Some((Box::new(cell), handles))
    }

    /// State as u8 for bridge data: 0=Idle, 1=Listen, 2=Respond.
    pub fn state_code(&self) -> u8 {
        match self.state {
            CRState::Idle => 0,
            CRState::Listen => 1,
            CRState::Respond => 2,
        }
    }

    /// Number of captured notes (for bridge data).
    pub fn phrase_len(&self) -> u8 {
        self.captured.len
    }

    /// Whether the cell is in capture-armed mode (for bridge data).
    pub fn listening(&self) -> bool {
        self.listening
    }

    /// Trigger autonomous call from the phrase bank.
    fn trigger_auto_call(&mut self) {
        let bank_len = self.phrase_bank.len();
        if bank_len == 0 {
            return;
        }

        let chaos = self.chaos_handle.value().clamp(0.0, 1.0);

        // Anti-convergence: check if current phrase has flattened
        let should_reseed = if self.generation > 2 && self.captured.len > 1 {
            let (pv, rv) = compute_phrase_variance(&self.captured);
            let combined = pv + rv;
            let seed_combined = self.seed_pitch_variance + self.seed_rhythm_variance;
            seed_combined > 0.01 && combined < self.reseed_threshold * seed_combined
        } else {
            // First few generations always use bank
            self.generation == 0
        };

        if should_reseed {
            // Chaos-weighted selection: low chaos = first phrases, high chaos = random
            let idx = if chaos < 0.3 && bank_len > 1 {
                // Prefer early phrases (lower chaos = more familiar)
                let range = (bank_len as f32 * (0.3 + chaos)).ceil() as usize;
                (rand_f32(&mut self.rng_state) * range.min(bank_len) as f32) as usize % bank_len
            } else {
                (rand_f32(&mut self.rng_state) * bank_len as f32) as usize % bank_len
            };

            // Copy bank phrase into captured
            self.captured.clear();
            let src = &self.phrase_bank[idx];
            for i in 0..src.len as usize {
                self.captured.push(src.notes[i]);
            }

            // Transpose to organism's tonal center + quantize to scale
            self.transpose_to_scale();

            self.generation = 0;
        }
        // else: captured already holds last response (generational chain)

        if self.call_audible {
            // Play the call phrase audibly, then generate response
            self.response.clear();
            for i in 0..self.captured.len as usize {
                self.response.push(self.captured.notes[i]);
            }
            self.auto_call_playing_call = true;
            // Use start_response machinery for playback init
            self.response_idx = 0;
            self.response_sample = 0;
            self.response_gate = false;
            self.response_velocity = 0.0;
            self.response_pitch = if self.response.len > 0 {
                self.response.notes[0].freq_hz
            } else {
                0.0
            };
            self.in_post_silence = false;
            let post_secs = clamp_param(PARAM_RANGES, "post_silence", self.post_silence_handle.value());
            self.post_silence_samples = (post_secs * self.sample_rate) as u32;
        } else {
            // Skip call, go straight to response
            self.auto_call_playing_call = false;
            self.start_response();
        }

        self.state = CRState::Respond;
        self.idle_counter = 0;
    }

    /// Transpose captured phrase toward the organism's active scale degrees.
    fn transpose_to_scale(&mut self) {
        let has_active = self.scale_weights.iter().any(|&w| w >= 0.1);
        if !has_active || self.captured.len == 0 {
            return;
        }

        // Find the phrase's median pitch class
        let len = self.captured.len as usize;
        let mut pc_sum = 0.0f32;
        for i in 0..len {
            let semitones = 12.0 * (self.captured.notes[i].freq_hz / 440.0).log2() + 69.0;
            pc_sum += ((semitones % 12.0) + 12.0) % 12.0;
        }
        let phrase_center_pc = pc_sum / len as f32;

        // Find the strongest scale degree
        let mut best_pc = 0usize;
        let mut best_w = 0.0f32;
        for i in 0..12 {
            if self.scale_weights[i] > best_w {
                best_w = self.scale_weights[i];
                best_pc = i;
            }
        }

        // Compute shift to align phrase center with strongest scale degree
        let shift = best_pc as f32 - phrase_center_pc;
        // Round to nearest semitone
        let shift_rounded = shift.round();

        // Apply transposition
        for i in 0..len {
            let semitones = 12.0 * (self.captured.notes[i].freq_hz / 440.0).log2() + 69.0;
            let new_semitones = semitones + shift_rounded;
            self.captured.notes[i].freq_hz =
                (440.0 * 2.0f32.powf((new_semitones - 69.0) / 12.0)).clamp(20.0, 20000.0);
        }
    }

    /// Finalize the current held note and push it to the phrase buffer.
    fn finalize_note(&mut self) {
        if self.note_held && self.current_note_freq > 0.0 {
            let ioi = if self.captured.len == 0 {
                0 // First note has no IOI
            } else {
                self.sample_counter.wrapping_sub(self.last_note_on_sample)
            };
            self.captured.push(CapturedNote {
                freq_hz: self.current_note_freq,
                velocity: self.current_note_vel,
                duration_samples: self.note_on_counter,
                ioi_samples: ioi,
            });
            self.note_held = false;
        }
    }

    /// Check if a phrase boundary has been reached.
    fn check_boundary(&self) -> bool {
        if self.captured.len == 0 {
            return false;
        }
        match self.phrase_mode {
            PhraseMode::Gap => {
                let gap_secs = clamp_param(PARAM_RANGES, "gap_threshold", self.gap_threshold_handle.value());
                let gap_samples = (gap_secs * self.sample_rate) as u32;
                !self.note_held && self.silence_counter >= gap_samples
            }
            PhraseMode::Beat => {
                // After NoteOff, wait for next beat boundary
                if self.note_held {
                    return false;
                }
                // Detect beat boundary: phase crosses 1.0
                let bps = (self.bpm / 60.0).max(0.1);
                let phase_inc = bps / self.sample_rate;
                let next_phase = self.beat_phase + phase_inc;
                !self.note_held && self.silence_counter > 0 && next_phase >= 1.0
            }
            PhraseMode::Fixed => {
                if !self.fixed_active {
                    return false;
                }
                let fixed_beats = clamp_param(PARAM_RANGES, "fixed_beats", self.fixed_beats_handle.value());
                let bps = (self.bpm / 60.0).max(0.1);
                let window_samples = (fixed_beats / bps * self.sample_rate) as u32;
                self.sample_counter.wrapping_sub(self.fixed_start_sample) >= window_samples
            }
        }
    }

    /// Generate mutated response from captured phrase.
    fn generate_response(&mut self) {
        let chaos_base = clamp_param(PARAM_RANGES, "chaos", self.chaos_handle.value());
        let drift = clamp_param(PARAM_RANGES, "drift_rate", self.drift_rate_handle.value());
        // Divergence pressure: chaos grows with generation depth to counteract gravity convergence
        let divergence = 0.01 * (self.generation.min(20) as f32);
        let effective_chaos = (chaos_base + drift * self.repetition_count as f32 + divergence).clamp(0.0, 1.0);

        let gravity = clamp_param(PARAM_RANGES, "gravity", self.gravity_handle.value());
        let contour_fidelity = clamp_param(PARAM_RANGES, "contour_fidelity", self.contour_fidelity_handle.value());
        let rhythm_jitter = clamp_param(PARAM_RANGES, "rhythm_jitter", self.rhythm_jitter_handle.value());
        let max_dev = clamp_param(PARAM_RANGES, "max_pitch_deviation", self.max_pitch_deviation_handle.value());
        let response_tempo = clamp_param(PARAM_RANGES, "response_tempo", self.response_tempo_handle.value());
        let insert_prob = clamp_param(PARAM_RANGES, "insert_probability", self.insert_probability_handle.value());
        let delete_prob = clamp_param(PARAM_RANGES, "delete_probability", self.delete_probability_handle.value());

        let src = self.captured.as_slice();
        let src_len = src.len();
        if src_len == 0 {
            self.response.clear();
            return;
        }

        self.response.clear();

        // Build response order based on response_mode
        // We iterate through indices; for mirror mode, reverse the order
        let indices: Vec<usize> = match self.response_mode {
            ResponseMode::Mirror => (0..src_len).rev().collect(),
            _ => (0..src_len).collect(),
        };

        // Structural anchors: first note, contour peak, last note get half mutation
        let peak_idx = src.iter()
            .enumerate()
            .max_by(|a, b| a.1.freq_hz.partial_cmp(&b.1.freq_hz).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        let mut prev_semitones: Option<f32> = None;

        for &idx in &indices {
            let note = src[idx];

            // Delete probability: convert note to rest
            if effective_chaos > 0.01 && rand_f32(&mut self.rng_state) < effective_chaos * delete_prob {
                continue;
            }

            // Structural anchors resist mutation (first, peak, last)
            let anchor_scale = if idx == 0 || idx == src_len - 1 || idx == peak_idx {
                0.5
            } else {
                1.0
            };

            // Pitch mutation
            let orig_semitones = 12.0 * (note.freq_hz / 440.0).log2() + 69.0;
            let mut new_semitones = orig_semitones
                + rand_bipolar(&mut self.rng_state) * effective_chaos * max_dev * anchor_scale;

            // Contour preservation: if previous note exists, ensure interval direction is preserved
            if contour_fidelity > 0.0 {
                if let Some(prev_st) = prev_semitones {
                    let orig_prev_st = if idx > 0 {
                        let prev_note = src[if self.response_mode == ResponseMode::Mirror { idx } else { idx - 1 }];
                        12.0 * (prev_note.freq_hz / 440.0).log2() + 69.0
                    } else {
                        orig_semitones
                    };
                    let orig_interval = orig_semitones - orig_prev_st;
                    let new_interval = new_semitones - prev_st;
                    let orig_dir = orig_interval.signum();
                    let new_dir = new_interval.signum();
                    if orig_interval.abs() > 0.01 && orig_dir != new_dir && contour_fidelity > 0.5 {
                        // Flip to preserve direction
                        new_semitones = prev_st + orig_dir * (new_semitones - prev_st).abs();
                    }
                }
            }
            prev_semitones = Some(new_semitones);

            // Gravity quantization toward scale degrees
            new_semitones = self.quantize_gravity(new_semitones, gravity * effective_chaos.max(0.01));

            let new_freq = 440.0 * 2.0f32.powf((new_semitones - 69.0) / 12.0);

            // Rhythm mutation
            let dur_scale = match self.response_mode {
                ResponseMode::Augment => 2.0,
                ResponseMode::Diminish => 0.5,
                _ => 1.0,
            };
            let dur_jitter = 1.0 + rand_bipolar(&mut self.rng_state) * effective_chaos * rhythm_jitter;
            let new_duration = ((note.duration_samples as f32 * dur_scale * dur_jitter * response_tempo)
                .max(1.0)) as u32;
            let ioi_jitter = 1.0 + rand_bipolar(&mut self.rng_state) * effective_chaos * rhythm_jitter;
            let new_ioi = ((note.ioi_samples as f32 * dur_scale * ioi_jitter * response_tempo)
                .max(1.0)) as u32;

            // Velocity mutation
            let new_vel = (note.velocity * (1.0 + rand_bipolar(&mut self.rng_state) * effective_chaos * 0.3))
                .clamp(0.05, 1.0);

            self.response.push(CapturedNote {
                freq_hz: new_freq.clamp(20.0, 20000.0),
                velocity: new_vel,
                duration_samples: new_duration,
                ioi_samples: new_ioi,
            });

            // Insert probability: add ghost note
            if effective_chaos > 0.01
                && rand_f32(&mut self.rng_state) < effective_chaos * insert_prob
                && (self.response.len as usize) < MAX_PHRASE_NOTES
            {
                // Interpolate between current and next note
                let next_freq = if idx + 1 < src_len {
                    src[idx + 1].freq_hz
                } else {
                    note.freq_hz
                };
                let ghost_freq = (new_freq + next_freq) * 0.5;
                let ghost_dur = new_duration / 2;
                self.response.push(CapturedNote {
                    freq_hz: ghost_freq.clamp(20.0, 20000.0),
                    velocity: new_vel * 0.6,
                    duration_samples: ghost_dur.max(1),
                    ioi_samples: ghost_dur.max(1),
                });
            }
        }

        // Fix IOIs: first note needs IOI = duration (plays immediately).
        // Subsequent notes with IOI < duration get IOI = duration (ensure full note plays).
        let rlen = self.response.len as usize;
        if rlen > 0 {
            self.response.notes[0].ioi_samples = self.response.notes[0].duration_samples;
            for i in 1..rlen {
                if self.response.notes[i].ioi_samples < self.response.notes[i].duration_samples {
                    self.response.notes[i].ioi_samples = self.response.notes[i].duration_samples;
                }
            }
        }

        self.repetition_count += 1;
    }

    /// Quantize a semitone position toward the nearest active scale degree.
    fn quantize_gravity(&self, semitones: f32, gravity: f32) -> f32 {
        if gravity < 0.01 {
            return semitones;
        }

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

    /// Begin playback of the response phrase.
    fn start_response(&mut self) {
        self.generate_response();
        self.response_idx = 0;
        self.response_sample = 0;
        self.response_gate = false;
        self.response_velocity = 0.0;
        self.response_pitch = if self.response.len > 0 {
            self.response.notes[0].freq_hz
        } else {
            0.0
        };
        self.in_post_silence = false;

        let post_secs = clamp_param(PARAM_RANGES, "post_silence", self.post_silence_handle.value());
        self.post_silence_samples = (post_secs * self.sample_rate) as u32;
    }

    /// Tick the response playback state machine. Returns (gate, pitch_hz).
    fn tick_response(&mut self) -> (f32, f32) {
        if self.in_post_silence {
            self.post_silence_counter += 1;
            if self.post_silence_counter >= self.post_silence_samples {
                if self.auto_call_playing_call {
                    // Call phase done — now generate the mutated response
                    self.auto_call_playing_call = false;
                    self.start_response();
                    return (0.0, self.response_pitch);
                }
                // Auto-call generational chain: response becomes next captured
                if self.auto_call && self.response.len > 0 {
                    self.captured.clear();
                    for i in 0..self.response.len as usize {
                        self.captured.push(self.response.notes[i]);
                    }
                    self.generation += 1;
                }
                self.state = CRState::Idle;
                self.in_post_silence = false;
            }
            return (0.0, self.response_pitch);
        }

        let len = self.response.len as usize;
        if self.response_idx >= len {
            // All notes played — enter post-silence
            self.in_post_silence = true;
            self.post_silence_counter = 0;
            return (0.0, self.response_pitch);
        }

        let note = self.response.notes[self.response_idx];

        // Advance check FIRST — if we've finished this note's IOI window,
        // move to next note and return gate=0 as a boundary marker.
        let advance_at = if self.response_idx == 0 {
            note.ioi_samples.max(note.duration_samples).max(1)
        } else {
            note.ioi_samples.max(1)
        };

        if self.response_sample >= advance_at {
            self.response_idx += 1;
            self.response_sample = 0;
            self.response_gate = false;
            if self.response_idx < len {
                self.response_pitch = self.response.notes[self.response_idx].freq_hz;
            }
            // Return one sample of gate=0 as note boundary
            return (0.0, self.response_pitch);
        }

        // Gate on at note start
        if self.response_sample == 0 {
            self.response_gate = true;
            self.response_velocity = note.velocity;
            self.response_pitch = note.freq_hz;
        }

        // Gate off after duration
        if self.response_sample >= note.duration_samples {
            self.response_gate = false;
        }

        self.response_sample += 1;

        // Velocity-encoded gate: velocity value when on (floor 0.05), 0.0 when off.
        // Downstream trigger wires use >TRIGGER_GATE_THRESHOLD for edge detection,
        // so any velocity above 0.05 registers as a valid gate.
        let gate = if self.response_gate { self.response_velocity.max(0.05) } else { 0.0 };
        (gate, self.response_pitch)
    }
}

impl DspCell for CallResponseCell {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.sample_counter = self.sample_counter.wrapping_add(1);

        // Beat phase tracking
        let bps = (self.bpm / 60.0).max(0.1);
        self.beat_phase += bps / self.sample_rate;
        if self.beat_phase >= 1.0 {
            self.beat_phase -= 1.0;
        }

        match self.state {
            CRState::Idle => {
                // Auto-call: trigger self-conversation when idle long enough
                if self.auto_call && !self.phrase_bank.is_empty()
                    && self.passthrough_gate < 0.01
                {
                    self.idle_counter += 1;
                    if self.idle_counter >= self.auto_call_delay_samples {
                        self.trigger_auto_call();
                        // Fall through to Respond branch on next tick
                    }
                } else {
                    self.idle_counter = 0;
                }

                // Pass-through: hear yourself through the organism's voice
                if output.len() >= 2 {
                    output[0] = self.passthrough_gate;
                    output[1] = self.passthrough_freq;
                } else if !output.is_empty() {
                    output[0] = self.passthrough_gate;
                }
            }
            CRState::Listen => {
                // Track held note duration
                if self.note_held {
                    self.note_on_counter += 1;
                    self.silence_counter = 0;
                } else {
                    self.silence_counter += 1;
                }

                // Check phrase boundary
                if self.check_boundary() {
                    // Finalize any held note
                    self.finalize_note();
                    if self.captured.len > 0 {
                        self.state = CRState::Respond;
                        self.start_response();
                    }
                }

                // Pass-through: hear yourself while it captures
                if output.len() >= 2 {
                    output[0] = self.passthrough_gate;
                    output[1] = self.passthrough_freq;
                } else if !output.is_empty() {
                    output[0] = self.passthrough_gate;
                }
            }
            CRState::Respond => {
                let (gate, pitch) = self.tick_response();
                if output.len() >= 2 {
                    output[0] = gate;
                    output[1] = pitch;
                } else if !output.is_empty() {
                    output[0] = gate;
                }
            }
        }
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        match cmd {
            DspCommand::NoteOn { freq, velocity } => {
                // ALWAYS set pass-through — you hear yourself regardless of state
                self.passthrough_gate = *velocity;
                self.passthrough_freq = *freq;
                self.idle_counter = 0;

                // Only start capture when listening is armed
                if self.listening {
                    match self.state {
                        CRState::Idle => {
                            // First note → start listening
                            // repetition_count persists across cycles (drift accumulates)
                            self.state = CRState::Listen;
                            self.captured.clear();
                            self.note_held = true;
                            self.current_note_freq = *freq;
                            self.current_note_vel = *velocity;
                            self.note_on_counter = 0;
                            self.last_note_on_sample = self.sample_counter;
                            self.silence_counter = 0;
                            self.fixed_active = true;
                            self.fixed_start_sample = self.sample_counter;
                        }
                        CRState::Listen => {
                            // Finalize previous note, start new one
                            self.finalize_note();
                            self.last_note_on_sample = self.sample_counter;
                            self.note_held = true;
                            self.current_note_freq = *freq;
                            self.current_note_vel = *velocity;
                            self.note_on_counter = 0;
                            self.silence_counter = 0;
                        }
                        CRState::Respond => {
                            // New call interrupts response → restart listen
                            self.state = CRState::Listen;
                            self.captured.clear();
                            self.repetition_count = 0;
                            self.auto_call_playing_call = false;
                            self.note_held = true;
                            self.current_note_freq = *freq;
                            self.current_note_vel = *velocity;
                            self.note_on_counter = 0;
                            self.last_note_on_sample = self.sample_counter;
                            self.silence_counter = 0;
                            self.fixed_active = true;
                            self.fixed_start_sample = self.sample_counter;
                        }
                    }
                }
            }
            DspCommand::NoteOff => {
                // ALWAYS clear pass-through gate
                self.passthrough_gate = 0.0;

                // Only finalize note when listening and in Listen state
                if self.listening && self.state == CRState::Listen && self.note_held {
                    self.finalize_note();
                    self.silence_counter = 0;
                }
            }
            DspCommand::SetListening(on) => {
                self.listening = *on;
                // If turned off mid-Listen, discard partial phrase, return to Idle
                if !on && self.state == CRState::Listen {
                    self.captured.clear();
                    self.note_held = false;
                    self.state = CRState::Idle;
                }
            }
            DspCommand::SetGlobalBpm(bpm) => {
                self.bpm = *bpm;
            }
            DspCommand::SetScaleWeights(weights, _blend) => {
                self.scale_weights = *weights;
            }
            DspCommand::SetChaos(c) => {
                self.chaos_handle.set(*c);
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
        self.state = CRState::Idle;
        self.captured.clear();
        self.response.clear();
        self.note_held = false;
        self.current_note_freq = 0.0;
        self.current_note_vel = 0.0;
        self.note_on_counter = 0;
        self.last_note_on_sample = 0;
        self.sample_counter = 0;
        self.silence_counter = 0;
        self.beat_phase = 0.0;
        self.fixed_active = false;
        self.response_idx = 0;
        self.response_sample = 0;
        self.response_gate = false;
        self.response_pitch = 0.0;
        self.response_velocity = 0.0;
        self.in_post_silence = false;
        self.repetition_count = 0;
        self.passthrough_gate = 0.0;
        self.passthrough_freq = 0.0;
        self.idle_counter = 0;
        self.auto_call_playing_call = false;
        // generation intentionally preserved across resets (tracks lifetime depth)
    }

    fn name(&self) -> &str {
        "call_response_cell"
    }

    fn param_ranges(&self) -> &'static [(&'static str, f32, f32)] {
        PARAM_RANGES
    }

    fn get_param_base(&self, name: &str) -> Option<f32> {
        self.base_values.get(name).copied()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    fn make_cr_dna(
        chaos: f32,
        gravity: f32,
        gap_threshold: f32,
        phrase_mode: &str,
        response_mode: &str,
    ) -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("chaos".into(), chaos);
        params.insert("gravity".into(), gravity);
        params.insert("gap_threshold".into(), gap_threshold);
        params.insert("contour_fidelity".into(), 0.7);
        params.insert("rhythm_jitter".into(), 0.15);
        params.insert("response_tempo".into(), 1.0);
        params.insert("max_pitch_deviation".into(), 5.0);
        params.insert("drift_rate".into(), 0.0);
        params.insert("insert_probability".into(), 0.0);
        params.insert("delete_probability".into(), 0.0);
        params.insert("post_silence".into(), 0.05);
        params.insert("fixed_beats".into(), 4.0);
        params.insert("bpm".into(), 120.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("phrase_mode".into(), phrase_mode.into());
        string_params.insert("response_mode".into(), response_mode.into());

        CellDna {
            cell_type: "call_response_cell".into(),
            params,
            string_params,
        }
    }

    /// Simulate N samples of silence.
    fn tick_n(cell: &mut Box<dyn DspCell>, n: usize) -> Vec<[f32; 2]> {
        let mut results = Vec::new();
        for _ in 0..n {
            let mut out = [0.0f32; 2];
            cell.tick(&[], &mut out);
            results.push(out);
        }
        results
    }

    #[test]
    fn idle_produces_silence() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        let results = tick_n(&mut cell, 100);
        for r in &results {
            assert_eq!(r[0], 0.0, "gate should be 0 in idle");
            assert_eq!(r[1], 0.0, "pitch should be 0 in idle");
        }
    }

    #[test]
    fn noteon_transitions_to_listen() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        // Should be in listen state — pass-through outputs velocity as gate
        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!((out[0] - 0.8).abs() < 0.01, "pass-through gate should be velocity 0.8, got {}", out[0]);
        assert!((out[1] - 440.0).abs() < 0.1, "pass-through pitch should be 440, got {}", out[1]);
    }

    #[test]
    fn full_cycle_idle_listen_respond() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "forward"); // Short gap threshold
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Play a note
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.2) as usize); // Hold for 200ms
        cell.handle_command(&DspCommand::NoteOff);

        // Wait for gap threshold (0.1s)
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Should now be in Respond state — check for gate activity
        let results = tick_n(&mut cell, (SR * 0.5) as usize);
        let has_gate = results.iter().any(|r| r[0] > 0.5);
        assert!(has_gate, "response should produce gate-on events");

        let has_pitch = results.iter().any(|r| r[1] > 20.0);
        assert!(has_pitch, "response should produce non-zero pitch");
    }

    #[test]
    fn echo_mode_chaos_zero() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Play C4
        cell.handle_command(&DspCommand::NoteOn { freq: 261.63, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        // Wait for gap
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Response should echo the original pitch exactly (chaos=0)
        let results = tick_n(&mut cell, (SR * 0.5) as usize);
        let pitches: Vec<f32> = results.iter()
            .filter(|r| r[0] > 0.5)
            .map(|r| r[1])
            .collect();

        assert!(!pitches.is_empty(), "should have response notes");
        for &p in &pitches {
            assert!(
                (p - 261.63).abs() < 0.1,
                "chaos=0 should echo exact pitch 261.63, got {p}"
            );
        }
    }

    #[test]
    fn mirror_mode_reverses() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "mirror");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Play two notes: C4 then E4
        cell.handle_command(&DspCommand::NoteOn { freq: 261.63, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.05) as usize);
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, 10);
        cell.handle_command(&DspCommand::NoteOn { freq: 329.63, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.05) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        // Collect ALL output from NoteOff onward (includes gap detection + full response)
        let results = tick_n(&mut cell, (SR * 1.0) as usize);
        let mut pitch_transitions: Vec<f32> = Vec::new();
        let mut prev_gate = false;
        for r in &results {
            let gate = r[0] > 0.5;
            if gate && !prev_gate {
                pitch_transitions.push(r[1]);
            }
            prev_gate = gate;
        }

        assert!(pitch_transitions.len() >= 2, "should have at least 2 notes in response");
        // Mirror mode: E4 then C4
        assert!(
            (pitch_transitions[0] - 329.63).abs() < 1.0,
            "mirror first note should be ~329.63, got {}",
            pitch_transitions[0]
        );
        assert!(
            (pitch_transitions[1] - 261.63).abs() < 1.0,
            "mirror second note should be ~261.63, got {}",
            pitch_transitions[1]
        );
    }

    #[test]
    fn gap_detection_waits_for_silence() {
        let dna = make_cr_dna(0.0, 0.0, 0.3, "gap", "forward"); // 300ms gap
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        // Only 100ms of silence — should still be in Listen
        tick_n(&mut cell, (SR * 0.1) as usize);
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Listen, "should still be listening before gap threshold");
    }

    #[test]
    fn beat_aligned_boundary() {
        let dna = make_cr_dna(0.0, 0.0, 0.3, "beat", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // At 120 BPM, one beat = 0.5s = 22050 samples
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        // Run until past beat boundary (~0.5s from start)
        let results = tick_n(&mut cell, (SR * 0.8) as usize);

        // Should eventually transition to Respond
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        let state = cr.state;
        let has_gate = results.iter().any(|r| r[0] > 0.5);
        assert!(
            state == CRState::Respond || state == CRState::Idle || has_gate,
            "beat mode should trigger response near beat boundary"
        );
    }

    #[test]
    fn fixed_length_boundary() {
        // 120 BPM, fixed_beats=2 → 1 second window
        let mut params = BTreeMap::new();
        params.insert("chaos".into(), 0.0f32);
        params.insert("gravity".into(), 0.0);
        params.insert("gap_threshold".into(), 2.0); // High to prevent gap trigger
        params.insert("contour_fidelity".into(), 0.0);
        params.insert("rhythm_jitter".into(), 0.0);
        params.insert("response_tempo".into(), 1.0);
        params.insert("max_pitch_deviation".into(), 5.0);
        params.insert("drift_rate".into(), 0.0);
        params.insert("insert_probability".into(), 0.0);
        params.insert("delete_probability".into(), 0.0);
        params.insert("post_silence".into(), 0.05);
        params.insert("fixed_beats".into(), 2.0);
        params.insert("bpm".into(), 120.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("phrase_mode".into(), "fixed".into());
        string_params.insert("response_mode".into(), "forward".into());

        let dna = CellDna {
            cell_type: "call_response_cell".into(),
            params,
            string_params,
        };

        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.2) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        // 2 beats at 120 BPM = 1 second
        // Run for 1.2 seconds — should have triggered response
        let results = tick_n(&mut cell, (SR * 1.2) as usize);
        let has_gate = results.iter().any(|r| r[0] > 0.5);
        assert!(has_gate, "fixed mode should trigger response after N beats");
    }

    #[test]
    fn scale_quantization_in_response() {
        let dna = make_cr_dna(0.5, 1.0, 0.1, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Set C major scale
        let mut weights = [0.0f32; 12];
        weights[0] = 1.0; // C
        weights[2] = 1.0; // D
        weights[4] = 1.0; // E
        weights[5] = 1.0; // F
        weights[7] = 1.0; // G
        weights[9] = 1.0; // A
        weights[11] = 1.0; // B
        cell.handle_command(&DspCommand::SetScaleWeights(weights, 1.0));

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);

        tick_n(&mut cell, (SR * 0.15) as usize);
        let results = tick_n(&mut cell, (SR * 0.5) as usize);

        let pitches: Vec<f32> = results.iter()
            .filter(|r| r[0] > 0.5)
            .map(|r| r[1])
            .collect();

        // All response pitches should be valid Hz
        for &p in &pitches {
            assert!(p >= 20.0 && p <= 20000.0, "pitch should be in valid range, got {p}");
        }
    }

    #[test]
    fn drift_accumulates() {
        let mut params = BTreeMap::new();
        params.insert("chaos".into(), 0.1f32);
        params.insert("gravity".into(), 0.0);
        params.insert("gap_threshold".into(), 0.1);
        params.insert("contour_fidelity".into(), 0.0);
        params.insert("rhythm_jitter".into(), 0.0);
        params.insert("response_tempo".into(), 1.0);
        params.insert("max_pitch_deviation".into(), 5.0);
        params.insert("drift_rate".into(), 0.3); // High drift
        params.insert("insert_probability".into(), 0.0);
        params.insert("delete_probability".into(), 0.0);
        params.insert("post_silence".into(), 0.05);
        params.insert("fixed_beats".into(), 4.0);
        params.insert("bpm".into(), 120.0);

        let mut string_params = BTreeMap::new();
        string_params.insert("phrase_mode".into(), "gap".into());
        string_params.insert("response_mode".into(), "forward".into());

        let dna = CellDna {
            cell_type: "call_response_cell".into(),
            params,
            string_params,
        };

        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Play same phrase multiple times, each response should drift more
        for cycle in 0..3 {
            cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
            tick_n(&mut cell, (SR * 0.1) as usize);
            cell.handle_command(&DspCommand::NoteOff);
            tick_n(&mut cell, (SR * 0.15) as usize);

            // Consume response
            tick_n(&mut cell, (SR * 0.5) as usize);

            let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
            assert_eq!(
                cr.repetition_count as usize,
                cycle + 1,
                "repetition count should increment"
            );
        }
    }

    #[test]
    fn reset_clears_state() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Get into respond state
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, (SR * 0.15) as usize);

        cell.handle_command(&DspCommand::Reset);

        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Idle, "reset should return to idle");
        assert_eq!(cr.captured.len, 0, "reset should clear captured phrase");
        assert_eq!(cr.repetition_count, 0, "reset should clear repetition count");
    }

    #[test]
    fn new_call_interrupts_response() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Get into respond state
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Start response
        tick_n(&mut cell, (SR * 0.05) as usize);

        // New NoteOn should interrupt → back to Listen
        cell.handle_command(&DspCommand::NoteOn { freq: 523.25, velocity: 0.9 });
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Listen, "new NoteOn should interrupt response");
    }

    #[test]
    fn output_channels_is_two() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (cell, _) = CallResponseCell::new(&dna, SR).unwrap();
        assert_eq!(cell.output_channels(), 2);
    }

    #[test]
    fn set_global_bpm_updates() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "beat", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::SetGlobalBpm(180.0));
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!((cr.bpm - 180.0).abs() < 0.01);
    }

    #[test]
    fn augment_doubles_duration() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "augment");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize); // 100ms note
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Response should be ~200ms (augmented)
        let results = tick_n(&mut cell, (SR * 0.5) as usize);
        let gate_on_count = results.iter().filter(|r| r[0] > 0.5).count();
        // Augmented duration should be roughly 2× original
        let original_dur = (SR * 0.1) as usize;
        assert!(
            gate_on_count > original_dur, // Should be longer than original
            "augment should produce longer response: gate_on={}, original_dur={}",
            gate_on_count, original_dur
        );
    }

    #[test]
    fn diminish_halves_duration() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "diminish");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.2) as usize); // 200ms note
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Response should be ~100ms (diminished)
        let results = tick_n(&mut cell, (SR * 0.5) as usize);
        let gate_on_count = results.iter().filter(|r| r[0] > 0.5).count();
        let original_dur = (SR * 0.2) as usize;
        assert!(
            gate_on_count < original_dur, // Should be shorter than original
            "diminish should produce shorter response: gate_on={}, original_dur={}",
            gate_on_count, original_dur
        );
    }

    // ─── Pass-through tests ─────────────────────────────────────────────

    #[test]
    fn passthrough_in_idle() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // NoteOn sets passthrough, but with listening=true it also transitions to Listen
        // Use SetListening(false) to stay in Idle
        cell.handle_command(&DspCommand::SetListening(false));
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.7 });

        let mut out = [0.0f32; 2];
        cell.tick(&[], &mut out);
        assert!((out[0] - 0.7).abs() < 0.01, "idle passthrough gate should be 0.7, got {}", out[0]);
        assert!((out[1] - 440.0).abs() < 0.1, "idle passthrough pitch should be 440, got {}", out[1]);

        // NoteOff clears gate
        cell.handle_command(&DspCommand::NoteOff);
        cell.tick(&[], &mut out);
        assert!(out[0] < 0.01, "after noteoff, gate should be 0");
    }

    #[test]
    fn passthrough_does_not_capture_when_not_listening() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::SetListening(false));
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });

        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Idle, "should stay in Idle when not listening");
    }

    #[test]
    fn set_listening_cancels_capture() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Start listening
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Listen);

        // Turn off listening mid-capture → should return to Idle
        cell.handle_command(&DspCommand::SetListening(false));
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Idle, "SetListening(false) should cancel capture");
        assert_eq!(cr.captured.len, 0, "partial capture should be discarded");
    }

    // ─── Variance computation tests ─────────────────────────────────────

    #[test]
    fn variance_empty_phrase() {
        let phrase = Phrase::new();
        let (pv, rv) = compute_phrase_variance(&phrase);
        assert_eq!(pv, 0.0);
        assert_eq!(rv, 0.0);
    }

    #[test]
    fn variance_single_note() {
        let mut phrase = Phrase::new();
        phrase.push(CapturedNote {
            freq_hz: 440.0, velocity: 0.8,
            duration_samples: 4410, ioi_samples: 4410,
        });
        let (pv, rv) = compute_phrase_variance(&phrase);
        assert_eq!(pv, 0.0);
        assert_eq!(rv, 0.0);
    }

    #[test]
    fn variance_constant_phrase() {
        // All same pitch and rhythm → zero variance
        let mut phrase = Phrase::new();
        for _ in 0..4 {
            phrase.push(CapturedNote {
                freq_hz: 440.0, velocity: 0.8,
                duration_samples: 4410, ioi_samples: 8820,
            });
        }
        let (pv, rv) = compute_phrase_variance(&phrase);
        assert!(pv < 0.01, "constant pitch should have near-zero variance, got {pv}");
        assert!(rv < 0.01, "constant rhythm should have near-zero variance, got {rv}");
    }

    #[test]
    fn variance_varied_phrase() {
        // Different pitches and rhythms → non-zero variance
        let mut phrase = Phrase::new();
        phrase.push(CapturedNote { freq_hz: 261.63, velocity: 0.8, duration_samples: 4410, ioi_samples: 4410 });
        phrase.push(CapturedNote { freq_hz: 329.63, velocity: 0.7, duration_samples: 8820, ioi_samples: 8820 });
        phrase.push(CapturedNote { freq_hz: 440.0, velocity: 0.6, duration_samples: 2205, ioi_samples: 2205 });
        phrase.push(CapturedNote { freq_hz: 261.63, velocity: 0.8, duration_samples: 4410, ioi_samples: 4410 });
        let (pv, rv) = compute_phrase_variance(&phrase);
        assert!(pv > 0.1, "varied phrase should have pitch variance > 0.1, got {pv}");
        assert!(rv > 0.01, "varied rhythm should have rhythm variance > 0.01, got {rv}");
    }

    // ─── Auto-call tests ────────────────────────────────────────────────

    fn make_auto_call_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("chaos".into(), 0.1f32);
        params.insert("gravity".into(), 0.5);
        params.insert("gap_threshold".into(), 0.1);
        params.insert("contour_fidelity".into(), 0.7);
        params.insert("rhythm_jitter".into(), 0.0);
        params.insert("response_tempo".into(), 1.0);
        params.insert("max_pitch_deviation".into(), 3.0);
        params.insert("drift_rate".into(), 0.0);
        params.insert("insert_probability".into(), 0.0);
        params.insert("delete_probability".into(), 0.0);
        params.insert("post_silence".into(), 0.05);
        params.insert("fixed_beats".into(), 4.0);
        params.insert("bpm".into(), 120.0);
        params.insert("auto_call_delay".into(), 0.1); // Short delay for test
        params.insert("reseed_threshold".into(), 0.4);

        let mut string_params = BTreeMap::new();
        string_params.insert("phrase_mode".into(), "gap".into());
        string_params.insert("response_mode".into(), "forward".into());
        string_params.insert("auto_call".into(), "true".into());
        string_params.insert("auto_listen".into(), "true".into());
        string_params.insert("call_audible".into(), "true".into());
        string_params.insert("phrase_source".into(), "sakamoto-seeds".into());

        CellDna {
            cell_type: "call_response_cell".into(),
            params,
            string_params,
        }
    }

    #[test]
    fn auto_call_loads_phrase_bank() {
        let dna = make_auto_call_dna();
        let (cell, _) = CallResponseCell::new(&dna, SR).unwrap();
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!(cr.auto_call, "auto_call should be true from DNA");
        // If sakamoto-seeds.json exists, bank should be non-empty
        // (test may run from different CWD, so be lenient)
        if !cr.phrase_bank.is_empty() {
            assert!(cr.phrase_bank[0].len > 0, "loaded phrases should have notes");
        }
    }

    #[test]
    fn auto_call_triggers_after_delay() {
        let dna = make_auto_call_dna();
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        if cr.phrase_bank.is_empty() {
            // Skip if phrase bank didn't load (no assets/phrases dir in test CWD)
            return;
        }

        // Tick enough for auto_call_delay (0.1s = 4410 samples)
        let results = tick_n(&mut cell, (SR * 0.2) as usize);

        // Should have transitioned to Respond and be producing output
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Respond, "should auto-trigger response after delay");

        let has_gate = results.iter().any(|r| r[0] > 0.01);
        assert!(has_gate, "auto-call should produce gate output");
    }

    #[test]
    fn auto_call_interrupted_by_human() {
        let dna = make_auto_call_dna();
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        if cr.phrase_bank.is_empty() {
            return;
        }

        // Wait for auto-call to trigger
        tick_n(&mut cell, (SR * 0.2) as usize);

        // Human NoteOn should interrupt → Listen
        cell.handle_command(&DspCommand::NoteOn { freq: 523.25, velocity: 0.9 });
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Listen, "human NoteOn should interrupt auto-call");
    }

    #[test]
    fn generation_increments_in_auto_call_chain() {
        // Use short synthetic phrase bank to keep cycle time manageable
        let mut params = BTreeMap::new();
        params.insert("chaos".into(), 0.1f32);
        params.insert("gravity".into(), 0.0);
        params.insert("gap_threshold".into(), 0.1);
        params.insert("contour_fidelity".into(), 0.7);
        params.insert("rhythm_jitter".into(), 0.0);
        params.insert("response_tempo".into(), 1.0);
        params.insert("max_pitch_deviation".into(), 3.0);
        params.insert("drift_rate".into(), 0.0);
        params.insert("insert_probability".into(), 0.0);
        params.insert("delete_probability".into(), 0.0);
        params.insert("post_silence".into(), 0.05);
        params.insert("fixed_beats".into(), 4.0);
        params.insert("bpm".into(), 120.0);
        params.insert("auto_call_delay".into(), 0.05); // 50ms
        params.insert("reseed_threshold".into(), 0.4);

        let mut string_params = BTreeMap::new();
        string_params.insert("phrase_mode".into(), "gap".into());
        string_params.insert("response_mode".into(), "forward".into());
        string_params.insert("auto_call".into(), "true".into());
        string_params.insert("auto_listen".into(), "true".into());
        string_params.insert("call_audible".into(), "false".into()); // Skip call playback for speed

        let dna = CellDna {
            cell_type: "call_response_cell".into(),
            params,
            string_params, // No phrase_source — manually inject short phrases
        };

        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Inject a short 3-note phrase bank via downcast
        let cr = cell.as_any_mut().downcast_mut::<CallResponseCell>().unwrap();
        let mut short_phrase = Phrase::new();
        short_phrase.push(CapturedNote { freq_hz: 440.0, velocity: 0.8, duration_samples: 1000, ioi_samples: 1200 });
        short_phrase.push(CapturedNote { freq_hz: 523.25, velocity: 0.7, duration_samples: 1000, ioi_samples: 1200 });
        short_phrase.push(CapturedNote { freq_hz: 392.0, velocity: 0.6, duration_samples: 1000, ioi_samples: 1200 });
        cr.phrase_bank.push(short_phrase);
        let (pv, rv) = compute_phrase_variance(&cr.phrase_bank[0]);
        cr.seed_pitch_variance = pv;
        cr.seed_rhythm_variance = rv;

        assert_eq!(cr.generation, 0);

        // Each cycle: ~3600 samples (response) + 2205 (post-silence) + 2205 (idle delay) ≈ 8000 samples ≈ 0.18s
        // Run 5 seconds = plenty for multiple cycles
        tick_n(&mut cell, (SR * 5.0) as usize);

        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!(cr.generation > 0, "generation should increment after auto-call cycles, got {}", cr.generation);
    }

    // ─── Phrase bank unit tests ─────────────────────────────────────────

    #[test]
    fn phrase_bank_loading() {
        let phrases = phrase_bank::load_phrases("sakamoto-seeds", std::path::Path::new("assets/phrases"));
        // If the file exists, should load 4 phrases
        if !phrases.is_empty() {
            assert!(phrases.len() >= 3, "sakamoto-seeds should have at least 3 phrases, got {}", phrases.len());
            for p in &phrases {
                assert!(!p.notes.is_empty(), "phrase '{}' should have notes", p.name);
            }
        }
    }

    #[test]
    fn listening_accessor() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!(cr.listening(), "default auto_listen should be true");

        cell.handle_command(&DspCommand::SetListening(false));
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!(!cr.listening(), "SetListening(false) should disable listening");
    }

    #[test]
    fn noteoff_always_clears_passthrough() {
        let dna = make_cr_dna(0.0, 0.0, 0.4, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!((cr.passthrough_gate - 0.8).abs() < 0.01);

        cell.handle_command(&DspCommand::NoteOff);
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert!(cr.passthrough_gate < 0.01, "NoteOff should always clear passthrough gate");
    }

    #[test]
    fn response_does_not_passthrough() {
        let dna = make_cr_dna(0.0, 0.0, 0.1, "gap", "forward");
        let (mut cell, _) = CallResponseCell::new(&dna, SR).unwrap();

        // Play a note, get into respond state
        cell.handle_command(&DspCommand::NoteOn { freq: 440.0, velocity: 0.8 });
        tick_n(&mut cell, (SR * 0.1) as usize);
        cell.handle_command(&DspCommand::NoteOff);
        tick_n(&mut cell, (SR * 0.15) as usize);

        // Should be in Respond — output comes from response, not passthrough
        let cr = cell.as_any().downcast_ref::<CallResponseCell>().unwrap();
        assert_eq!(cr.state, CRState::Respond);

        // Response output should be the generated phrase, not passthrough
        let results = tick_n(&mut cell, (SR * 0.3) as usize);
        let has_response = results.iter().any(|r| r[0] > 0.01);
        assert!(has_response, "respond state should produce output from response phrase");
    }
}
