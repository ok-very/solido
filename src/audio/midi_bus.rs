//! MIDI input bus — bridges external MIDI controllers into Solido.
//!
//! Follows the same pattern as ReverbBus/TapeDelayBus:
//! - Control thread owns the `midir` connection
//! - midir callback thread parses raw bytes → `MidiEvent` and pushes to ringbuf
//! - Control thread drains events each frame and dispatches via CC mappings
//!
//! No MIDI output for now — input only.

use crate::substrate::channel::{self, Receiver};

/// Ringbuf capacity for MIDI events. 256 is plenty for one frame at 60fps
/// (MIDI is ~31.25 kBaud ≈ 1000 3-byte messages/sec ≈ 16 per frame).
const MIDI_RINGBUF_CAPACITY: usize = 256;

// ─── MidiEvent ───────────────────────────────────────────────────────

/// Raw MIDI event — Copy, stack-allocated, RT-safe to send through ringbuf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
    Clock,
    Start,
    Stop,
    Continue,
}

// ─── MIDI byte parsing ──────────────────────────────────────────────

/// Parse raw MIDI bytes into a `MidiEvent`.
/// Returns `None` for unrecognized or incomplete messages.
pub fn parse_midi_bytes(msg: &[u8]) -> Option<MidiEvent> {
    if msg.is_empty() {
        return None;
    }

    let status = msg[0];

    // System real-time messages (single byte, 0xF8–0xFF)
    match status {
        0xF8 => return Some(MidiEvent::Clock),
        0xFA => return Some(MidiEvent::Start),
        0xFB => return Some(MidiEvent::Continue),
        0xFC => return Some(MidiEvent::Stop),
        _ => {}
    }

    // Channel messages require at least 2 bytes
    if msg.len() < 2 {
        return None;
    }

    let channel = status & 0x0F;
    let msg_type = status & 0xF0;

    match msg_type {
        0x80 => {
            // Note Off
            Some(MidiEvent::NoteOff {
                channel,
                note: msg[1] & 0x7F,
            })
        }
        0x90 => {
            // Note On (velocity 0 = Note Off per MIDI spec)
            let note = msg[1] & 0x7F;
            let velocity = msg.get(2).copied().unwrap_or(0) & 0x7F;
            if velocity == 0 {
                Some(MidiEvent::NoteOff { channel, note })
            } else {
                Some(MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                })
            }
        }
        0xB0 => {
            // Control Change
            if msg.len() < 3 {
                return None;
            }
            Some(MidiEvent::ControlChange {
                channel,
                cc: msg[1] & 0x7F,
                value: msg[2] & 0x7F,
            })
        }
        0xE0 => {
            // Pitch Bend
            if msg.len() < 3 {
                return None;
            }
            let lsb = msg[1] as i16 & 0x7F;
            let msb = msg[2] as i16 & 0x7F;
            let value = ((msb << 7) | lsb) - 8192; // Center at 0
            Some(MidiEvent::PitchBend { channel, value })
        }
        _ => None,
    }
}

// ─── MidiBus ─────────────────────────────────────────────────────────

/// Control-thread MIDI bus. Owns the midir connection and receives parsed events.
pub struct MidiBus {
    /// Active midir connection (Option so we can disconnect gracefully).
    _connection: Option<midir::MidiInputConnection<()>>,
    /// Available input port names (snapshot from last enumeration).
    available_ports: Vec<String>,
    /// Currently connected port name.
    connected_port: Option<String>,
}

impl MidiBus {
    /// Enumerate available MIDI input ports. Non-blocking.
    /// Returns empty Vec if no MIDI subsystem is available (not an error).
    pub fn list_ports() -> Vec<String> {
        let midi_in = match midir::MidiInput::new("solido-probe") {
            Ok(m) => m,
            Err(e) => {
                log::warn!("MidiBus: failed to create MIDI input for enumeration: {e}");
                return Vec::new();
            }
        };
        midi_in
            .ports()
            .iter()
            .filter_map(|p| midi_in.port_name(p).ok())
            .collect()
    }

    /// Connect to a named MIDI input port.
    /// Returns `(MidiBus, Receiver<MidiEvent>)` on success.
    /// The caller drains the receiver each frame.
    pub fn connect(port_name: &str) -> Option<(Self, Receiver<MidiEvent>)> {
        let midi_in = midir::MidiInput::new("solido-midi").ok()?;
        let ports = midi_in.ports();
        let port = ports
            .iter()
            .find(|p| midi_in.port_name(p).as_deref() == Ok(port_name))?;

        let (mut event_tx, event_rx) = channel::channel::<MidiEvent>(MIDI_RINGBUF_CAPACITY);

        let port_display = port_name.to_string();
        let connection = midi_in
            .connect(
                port,
                "solido-in",
                move |_timestamp, message, _| {
                    if let Some(event) = parse_midi_bytes(message) {
                        // Non-blocking push — drop if ringbuf is full (better than blocking)
                        let _ = event_tx.try_send(event);
                    }
                },
                (),
            )
            .ok()?;

        let available_ports = Self::list_ports();

        log::info!("MidiBus: connected to '{port_display}'");
        Some((
            MidiBus {
                _connection: Some(connection),
                available_ports,
                connected_port: Some(port_display),
            },
            event_rx,
        ))
    }

    /// Disconnect and release the MIDI port.
    pub fn disconnect(&mut self) {
        if let Some(name) = self.connected_port.take() {
            log::info!("MidiBus: disconnected from '{name}'");
        }
        self._connection = None;
    }

    /// Currently connected port name, if any.
    pub fn connected_port(&self) -> Option<&str> {
        self.connected_port.as_deref()
    }

    /// Available port names (from last enumeration).
    pub fn available_ports(&self) -> &[String] {
        &self.available_ports
    }

    /// Re-enumerate available ports (e.g., when user opens device selector).
    pub fn refresh_ports(&mut self) {
        self.available_ports = Self::list_ports();
    }
}

// ─── CC Mapping ──────────────────────────────────────────────────────

/// A single CC → parameter mapping, created via learn mode.
#[derive(Debug, Clone)]
pub struct CcMapping {
    /// MIDI CC number (0–127).
    pub cc: u8,
    /// MIDI channel filter. None = omni (any channel).
    pub channel: Option<u8>,
    /// Target parameter name (e.g., "cell2.cutoff", "master_gain").
    pub target: String,
    /// Output value range — CC 0 maps to range.0, CC 127 maps to range.1.
    pub range: (f32, f32),
}

impl CcMapping {
    /// Map a raw CC value (0–127) to the output range.
    pub fn map_value(&self, cc_value: u8) -> f32 {
        let t = cc_value as f32 / 127.0;
        self.range.0 + t * (self.range.1 - self.range.0)
    }
}

/// Mapping state for the CC learn workflow.
pub struct CcLearnState {
    /// All active CC → parameter mappings.
    pub mappings: Vec<CcMapping>,
    /// When true, the next CC event creates a mapping to `learn_target`.
    pub learning: bool,
    /// The parameter target waiting for a CC assignment.
    pub learn_target: Option<String>,
    /// Range for the pending learn target.
    pub learn_range: Option<(f32, f32)>,
}

impl CcLearnState {
    pub fn new() -> Self {
        CcLearnState {
            mappings: Vec::new(),
            learning: false,
            learn_target: None,
            learn_range: None,
        }
    }

    /// Start learning: next CC received will map to the given target.
    pub fn start_learn(&mut self, target: String, range: (f32, f32)) {
        self.learning = true;
        self.learn_target = Some(target);
        self.learn_range = Some(range);
    }

    /// Cancel learn mode without creating a mapping.
    pub fn cancel_learn(&mut self) {
        self.learning = false;
        self.learn_target = None;
        self.learn_range = None;
    }

    /// Process a CC event during learn mode. Returns true if a mapping was created.
    pub fn process_learn(&mut self, cc: u8, channel: u8) -> bool {
        if !self.learning {
            return false;
        }
        let target = match self.learn_target.take() {
            Some(t) => t,
            None => return false,
        };
        let range = self.learn_range.take().unwrap_or((0.0, 1.0));

        // Remove any existing mapping for the same target
        self.mappings.retain(|m| m.target != target);

        self.mappings.push(CcMapping {
            cc,
            channel: Some(channel),
            target,
            range,
        });
        self.learning = false;
        true
    }

    /// Find the mapping for a given CC/channel, if any.
    pub fn find_mapping(&self, cc: u8, channel: u8) -> Option<&CcMapping> {
        self.mappings.iter().find(|m| {
            m.cc == cc && m.channel.map_or(true, |ch| ch == channel)
        })
    }

    /// Remove all mappings targeting a given parameter name.
    pub fn unlearn_by_target(&mut self, target: &str) {
        self.mappings.retain(|m| m.target != target);
    }

    /// Remove all mappings for a given CC number.
    pub fn unlearn_by_cc(&mut self, cc: u8) {
        self.mappings.retain(|m| m.cc != cc);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_midi_bytes ──

    #[test]
    fn parse_note_on() {
        let msg = [0x90, 60, 100]; // Ch 0, C4, vel 100
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    }

    #[test]
    fn parse_note_on_velocity_zero_is_note_off() {
        let msg = [0x91, 64, 0]; // Ch 1, E4, vel 0
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::NoteOff { channel: 1, note: 64 });
    }

    #[test]
    fn parse_note_off() {
        let msg = [0x80, 60, 64]; // Ch 0, C4
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::NoteOff { channel: 0, note: 60 });
    }

    #[test]
    fn parse_control_change() {
        let msg = [0xB0, 74, 127]; // Ch 0, CC 74, value 127
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::ControlChange { channel: 0, cc: 74, value: 127 });
    }

    #[test]
    fn parse_pitch_bend_center() {
        let msg = [0xE0, 0x00, 0x40]; // Center: MSB=64, LSB=0 → 8192 - 8192 = 0
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::PitchBend { channel: 0, value: 0 });
    }

    #[test]
    fn parse_pitch_bend_max() {
        let msg = [0xE0, 0x7F, 0x7F]; // Max: (127<<7)|127 = 16383 - 8192 = 8191
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::PitchBend { channel: 0, value: 8191 });
    }

    #[test]
    fn parse_pitch_bend_min() {
        let msg = [0xE0, 0x00, 0x00]; // Min: 0 - 8192 = -8192
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::PitchBend { channel: 0, value: -8192 });
    }

    #[test]
    fn parse_clock() {
        assert_eq!(parse_midi_bytes(&[0xF8]), Some(MidiEvent::Clock));
    }

    #[test]
    fn parse_start() {
        assert_eq!(parse_midi_bytes(&[0xFA]), Some(MidiEvent::Start));
    }

    #[test]
    fn parse_stop() {
        assert_eq!(parse_midi_bytes(&[0xFC]), Some(MidiEvent::Stop));
    }

    #[test]
    fn parse_continue() {
        assert_eq!(parse_midi_bytes(&[0xFB]), Some(MidiEvent::Continue));
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_midi_bytes(&[]), None);
    }

    #[test]
    fn parse_unknown_status_returns_none() {
        assert_eq!(parse_midi_bytes(&[0xF5]), None); // Undefined system common
    }

    #[test]
    fn parse_truncated_cc_returns_none() {
        assert_eq!(parse_midi_bytes(&[0xB0, 74]), None); // CC needs 3 bytes
    }

    #[test]
    fn parse_channel_extraction() {
        // NoteOn on channel 15 (0x9F)
        let msg = [0x9F, 60, 100];
        let event = parse_midi_bytes(&msg).unwrap();
        assert_eq!(event, MidiEvent::NoteOn { channel: 15, note: 60, velocity: 100 });
    }

    // ── MidiEvent is Copy + small ──

    #[test]
    fn midi_event_is_copy() {
        let e = MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 };
        let e2 = e; // Copy
        assert_eq!(e, e2);
    }

    #[test]
    fn midi_event_size() {
        // Should be small enough for ringbuf (≤8 bytes ideally)
        assert!(std::mem::size_of::<MidiEvent>() <= 8);
    }

    // ── CC mapping ──

    #[test]
    fn cc_map_value_range() {
        let m = CcMapping {
            cc: 1,
            channel: None,
            target: "cell0.cutoff".into(),
            range: (20.0, 20000.0),
        };
        assert!((m.map_value(0) - 20.0).abs() < 0.01);
        assert!((m.map_value(127) - 20000.0).abs() < 0.01);
        // Midpoint
        let mid = m.map_value(64);
        assert!(mid > 9000.0 && mid < 11000.0);
    }

    // ── CC learn ──

    #[test]
    fn cc_learn_creates_mapping() {
        let mut state = CcLearnState::new();
        state.start_learn("cell2.cutoff".into(), (20.0, 20000.0));
        assert!(state.learning);

        // Simulate receiving CC 74 on channel 0
        let created = state.process_learn(74, 0);
        assert!(created);
        assert!(!state.learning);
        assert_eq!(state.mappings.len(), 1);
        assert_eq!(state.mappings[0].cc, 74);
        assert_eq!(state.mappings[0].target, "cell2.cutoff");
        assert_eq!(state.mappings[0].range, (20.0, 20000.0));
    }

    #[test]
    fn cc_learn_replaces_existing_target() {
        let mut state = CcLearnState::new();
        // First mapping: CC 1 → cutoff
        state.start_learn("cell0.cutoff".into(), (20.0, 20000.0));
        state.process_learn(1, 0);

        // Re-learn: CC 74 → same target
        state.start_learn("cell0.cutoff".into(), (100.0, 8000.0));
        state.process_learn(74, 0);

        assert_eq!(state.mappings.len(), 1, "should replace, not duplicate");
        assert_eq!(state.mappings[0].cc, 74);
        assert_eq!(state.mappings[0].range, (100.0, 8000.0));
    }

    #[test]
    fn cc_find_mapping() {
        let mut state = CcLearnState::new();
        state.mappings.push(CcMapping {
            cc: 74,
            channel: Some(0),
            target: "cell0.cutoff".into(),
            range: (20.0, 20000.0),
        });

        assert!(state.find_mapping(74, 0).is_some());
        assert!(state.find_mapping(74, 1).is_none()); // Wrong channel
        assert!(state.find_mapping(75, 0).is_none()); // Wrong CC
    }

    #[test]
    fn cc_find_mapping_omni() {
        let mut state = CcLearnState::new();
        state.mappings.push(CcMapping {
            cc: 1,
            channel: None, // Omni
            target: "master_gain".into(),
            range: (0.0, 1.0),
        });

        assert!(state.find_mapping(1, 0).is_some());
        assert!(state.find_mapping(1, 15).is_some()); // Any channel matches
    }

    #[test]
    fn cc_unlearn_by_target() {
        let mut state = CcLearnState::new();
        state.mappings.push(CcMapping {
            cc: 1,
            channel: None,
            target: "cell0.cutoff".into(),
            range: (0.0, 1.0),
        });
        state.mappings.push(CcMapping {
            cc: 2,
            channel: None,
            target: "master_gain".into(),
            range: (0.0, 1.0),
        });

        state.unlearn_by_target("cell0.cutoff");
        assert_eq!(state.mappings.len(), 1);
        assert_eq!(state.mappings[0].target, "master_gain");
    }

    #[test]
    fn cc_unlearn_by_cc() {
        let mut state = CcLearnState::new();
        state.mappings.push(CcMapping {
            cc: 74,
            channel: None,
            target: "cell0.cutoff".into(),
            range: (0.0, 1.0),
        });

        state.unlearn_by_cc(74);
        assert!(state.mappings.is_empty());
    }

    #[test]
    fn cc_cancel_learn() {
        let mut state = CcLearnState::new();
        state.start_learn("cell0.cutoff".into(), (0.0, 1.0));
        assert!(state.learning);

        state.cancel_learn();
        assert!(!state.learning);
        assert!(state.learn_target.is_none());
        assert!(state.mappings.is_empty());
    }

    // ── MidiBus ──

    #[test]
    fn list_ports_returns_vec() {
        // May be empty if no MIDI hardware — that's fine, just shouldn't panic
        let ports = MidiBus::list_ports();
        assert!(ports.len() < 1000, "sanity check");
    }

    #[test]
    fn midi_to_hz_roundtrip() {
        let hz = crate::samples::midi_to_hz(69);
        assert!((hz - 440.0).abs() < 0.01, "A4 = MIDI 69 = 440 Hz, got {hz}");
    }
}
