//! Phrase bank — loads melodic seed phrases from JSON or MIDI files.
//!
//! All loading happens at construction time (control thread). Produces `PhraseNote`
//! structs that `call_response_cell` converts to `CapturedNote` at the target sample rate.
//!
//! # Supported formats
//! - **JSON**: `assets/phrases/{name}.json` — hand-authored seed phrases
//! - **MIDI**: `assets/phrases/{name}.mid` — Standard MIDI File (SMF), track 0

use std::path::Path;

/// A note in a phrase bank entry (control-thread format).
#[derive(Debug, Clone)]
pub struct PhraseNote {
    /// MIDI note number (60 = C4, 69 = A4).
    pub midi_note: u8,
    /// Velocity [0.0, 1.0].
    pub velocity: f32,
    /// Duration in seconds.
    pub duration_sec: f32,
    /// Inter-onset interval in seconds (time from this note's start to next note's start).
    pub ioi_sec: f32,
}

/// A named phrase from the bank.
#[derive(Debug, Clone)]
pub struct PhraseData {
    pub name: String,
    pub notes: Vec<PhraseNote>,
}

/// Load phrases from a source name, searching `base_dir` for `.json` or `.mid` files.
///
/// Tries `{base_dir}/{source}.json` first, then `{base_dir}/{source}.mid`.
/// Returns empty Vec if neither exists.
pub fn load_phrases(source: &str, base_dir: &Path) -> Vec<PhraseData> {
    if source.is_empty() {
        return Vec::new();
    }

    // Try JSON first
    let json_path = base_dir.join(format!("{}.json", source));
    if json_path.exists() {
        match load_json(&json_path) {
            Ok(phrases) => {
                log::info!("phrase_bank: loaded {} phrases from {:?}", phrases.len(), json_path);
                return phrases;
            }
            Err(e) => {
                log::warn!("phrase_bank: failed to load JSON {:?}: {}", json_path, e);
            }
        }
    }

    // Try MIDI
    let mid_path = base_dir.join(format!("{}.mid", source));
    if mid_path.exists() {
        match load_midi(&mid_path) {
            Ok(phrases) => {
                log::info!("phrase_bank: loaded {} phrases from {:?}", phrases.len(), mid_path);
                return phrases;
            }
            Err(e) => {
                log::warn!("phrase_bank: failed to load MIDI {:?}: {}", mid_path, e);
            }
        }
    }

    log::warn!("phrase_bank: no file found for source '{}' in {:?}", source, base_dir);
    Vec::new()
}

/// Convert MIDI note number to Hz. A4 = MIDI 69 = 440 Hz.
pub fn midi_to_hz(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

// ─── JSON loading ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct JsonBank {
    phrases: Vec<JsonPhrase>,
}

#[derive(serde::Deserialize)]
struct JsonPhrase {
    name: String,
    notes: Vec<JsonNote>,
}

#[derive(serde::Deserialize)]
struct JsonNote {
    midi_note: u8,
    #[serde(default = "default_vel")]
    velocity: f32,
    duration_sec: f32,
    ioi_sec: f32,
}

fn default_vel() -> f32 { 0.7 }

fn load_json(path: &Path) -> Result<Vec<PhraseData>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let bank: JsonBank = serde_json::from_str(&text).map_err(|e| format!("parse: {e}"))?;

    Ok(bank
        .phrases
        .into_iter()
        .filter(|p| !p.notes.is_empty())
        .map(|p| PhraseData {
            name: p.name,
            notes: p
                .notes
                .into_iter()
                .map(|n| PhraseNote {
                    midi_note: n.midi_note,
                    velocity: n.velocity.clamp(0.05, 1.0),
                    duration_sec: n.duration_sec.max(0.01),
                    ioi_sec: n.ioi_sec.max(0.01),
                })
                .collect(),
        })
        .collect())
}

// ─── MIDI loading ────────────────────────────────────────────────────

fn load_midi(path: &Path) -> Result<Vec<PhraseData>, String> {
    let data = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let smf = midly::Smf::parse(&data).map_err(|e| format!("parse: {e}"))?;

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(tpb) => tpb.as_int() as f64,
        midly::Timing::Timecode(fps, sub) => {
            // Fallback: treat as ~120 BPM equivalent
            let _ = (fps, sub);
            480.0
        }
    };

    let mut phrases = Vec::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        // Find tempo (default 120 BPM = 500000 us/beat)
        let mut us_per_beat: f64 = 500_000.0;
        let mut notes: Vec<(u8, f32, f64, f64)> = Vec::new(); // (midi, vel, start_sec, dur_sec)
        let mut pending: std::collections::HashMap<u8, (f32, f64)> = std::collections::HashMap::new();
        let mut abs_tick: u64 = 0;

        for event in track {
            abs_tick += event.delta.as_int() as u64;
            let time_sec = abs_tick as f64 * us_per_beat / (ticks_per_beat * 1_000_000.0);

            match event.kind {
                midly::TrackEventKind::Meta(midly::MetaMessage::Tempo(t)) => {
                    us_per_beat = t.as_int() as f64;
                }
                midly::TrackEventKind::Midi { message, .. } => match message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        if vel.as_int() == 0 {
                            // NoteOn vel=0 is NoteOff
                            if let Some((v, start)) = pending.remove(&key.as_int()) {
                                notes.push((key.as_int(), v, start, time_sec - start));
                            }
                        } else {
                            pending.insert(
                                key.as_int(),
                                (vel.as_int() as f32 / 127.0, time_sec),
                            );
                        }
                    }
                    midly::MidiMessage::NoteOff { key, .. } => {
                        if let Some((v, start)) = pending.remove(&key.as_int()) {
                            notes.push((key.as_int(), v, start, time_sec - start));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Flush pending (unterminated notes get 0.5s duration)
        for (key, (vel, start)) in pending.drain() {
            let end = abs_tick as f64 * us_per_beat / (ticks_per_beat * 1_000_000.0);
            let dur = (end - start).max(0.1);
            notes.push((key, vel, start, dur));
        }

        if notes.is_empty() {
            continue;
        }

        // Sort by start time
        notes.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // Convert to PhraseNote with IOI
        let mut phrase_notes = Vec::new();
        for (i, &(midi, vel, start, dur)) in notes.iter().enumerate() {
            let ioi = if i + 1 < notes.len() {
                (notes[i + 1].2 - start).max(0.01)
            } else {
                dur.max(0.01) // Last note: IOI = duration
            };
            phrase_notes.push(PhraseNote {
                midi_note: midi,
                velocity: vel.clamp(0.05, 1.0),
                duration_sec: dur.max(0.01) as f32,
                ioi_sec: ioi as f32,
            });
        }

        let name = if smf.tracks.len() == 1 {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("track-{}", track_idx))
        } else {
            format!(
                "{}-track{}",
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                track_idx
            )
        };

        phrases.push(PhraseData {
            name,
            notes: phrase_notes,
        });
    }

    Ok(phrases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_to_hz_a4() {
        let hz = midi_to_hz(69);
        assert!((hz - 440.0).abs() < 0.01);
    }

    #[test]
    fn midi_to_hz_c4() {
        let hz = midi_to_hz(60);
        assert!((hz - 261.63).abs() < 0.1);
    }

    #[test]
    fn load_nonexistent_source_returns_empty() {
        let phrases = load_phrases("nonexistent", Path::new("nonexistent/dir"));
        assert!(phrases.is_empty());
    }

    #[test]
    fn load_empty_source_returns_empty() {
        let phrases = load_phrases("", Path::new("assets/phrases"));
        assert!(phrases.is_empty());
    }

    #[test]
    fn json_parsing() {
        let json = r#"{
            "phrases": [
                {
                    "name": "test",
                    "notes": [
                        { "midi_note": 60, "velocity": 0.8, "duration_sec": 0.5, "ioi_sec": 0.6 },
                        { "midi_note": 64, "velocity": 0.6, "duration_sec": 0.3, "ioi_sec": 0.4 }
                    ]
                }
            ]
        }"#;

        let bank: JsonBank = serde_json::from_str(json).unwrap();
        assert_eq!(bank.phrases.len(), 1);
        assert_eq!(bank.phrases[0].notes.len(), 2);
        assert_eq!(bank.phrases[0].notes[0].midi_note, 60);
    }

    #[test]
    fn phrase_note_velocity_default() {
        let json = r#"{ "midi_note": 60, "duration_sec": 0.5, "ioi_sec": 0.6 }"#;
        let note: JsonNote = serde_json::from_str(json).unwrap();
        assert!((note.velocity - 0.7).abs() < 0.01, "default vel should be 0.7");
    }
}
