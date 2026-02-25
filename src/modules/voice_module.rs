use std::any::Any;
use std::collections::HashMap;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::substrate::audio::{AudioAnalysis, AudioCommand, VoiceParam};
use crate::substrate::channel::{Receiver, Sender};

/// Default filter cutoff in Hz.
const DEFAULT_CUTOFF: f32 = 2000.0;
/// Default voice amplitude.
const DEFAULT_AMPLITUDE: f32 = 0.3;
/// Safety-net auto-release timeout (seconds). With proper note-off this
/// should rarely fire — only catches orphaned voices from lost events.
const AUTO_RELEASE_SECS: f32 = 10.0;
/// Offset added to note numbers to create the [10,16] range that avoids
/// spurious edge connections with [0,1] normalized signals.
const NOTE_OFFSET: f32 = 10.0;

/// Audio output module — bridges the affinity graph to the audio thread.
///
/// Receives `note_on` and `note_off` signals for voice lifecycle, plus
/// continuous `pitch_hz`, `filter_cutoff`, and `amplitude` for parameter
/// modulation. Sends AudioCommands to the VoicePool via lock-free ring
/// buffer, and emits analysis signals (rms, peak) back into the graph.
pub struct VoiceModule {
    schema: ModuleSchema,
    cmd_tx: Sender<AudioCommand>,
    analysis_rx: Receiver<AudioAnalysis>,
    // Current parameter state
    current_cutoff: f32,
    current_amplitude: f32,
    // Analysis from audio thread
    current_rms: f32,
    current_peak: f32,
    active_voices: u32,
    // Voice tracking: note_number → voice_id
    next_voice_id: u64,
    note_voices: HashMap<u8, u64>,
    pending_kills: Vec<(u64, f32)>, // (voice_id, time_remaining_secs)
    // Port IDs
    pitch_hz_port: PortId,
    note_on_port: PortId,
    note_off_port: PortId,
    filter_cutoff_port: PortId,
    amplitude_port: PortId,
    rms_port: PortId,
    peak_port: PortId,
    is_active_port: PortId,
    voice_count_port: PortId,
}

/// Convert a note number (0-6) to frequency in Hz.
/// Maps 2 octaves from C4: note 0 = 261.63 Hz (C4), note 6 = 1046.5 Hz (C6).
fn note_to_hz(note: u8) -> f32 {
    261.63 * 2.0f32.powf(note as f32 / 6.0 * 2.0)
}

impl VoiceModule {
    pub fn new(cmd_tx: Sender<AudioCommand>, analysis_rx: Receiver<AudioAnalysis>) -> Self {
        let pitch_hz_in = Port::input("pitch_hz", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Pitch frequency in Hz from quantizer (refines active voices)");
        let note_on_in = Port::input("note_on", SignalType::Float, PortRate::Event)
            .with_range(10.0, 16.0)
            .with_description("Note-on: value = note_number + 10. Spawns a voice.");
        let note_off_in = Port::input("note_off", SignalType::Float, PortRate::Event)
            .with_range(10.0, 16.0)
            .with_description("Note-off: value = note_number + 10. Releases the voice.");
        let filter_cutoff_in = Port::input("filter_cutoff", SignalType::Float, PortRate::Block)
            .with_range(100.0, 15000.0)
            .with_description("Filter cutoff frequency in Hz");
        let amplitude_in = Port::input("amplitude", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Voice amplitude 0.0-1.0");

        let rms_out = Port::output("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Audio RMS level");
        let peak_out = Port::output("peak", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Audio peak level");
        let is_active_out = Port::output("is_active", SignalType::Bool, PortRate::Block)
            .with_description("True when any voice is playing");
        let voice_count_out = Port::output("voice_count", SignalType::Float, PortRate::Block)
            .with_range(0.0, 8.0)
            .with_description("Number of active voices");

        let pitch_hz_port = pitch_hz_in.id;
        let note_on_port = note_on_in.id;
        let note_off_port = note_off_in.id;
        let filter_cutoff_port = filter_cutoff_in.id;
        let amplitude_port = amplitude_in.id;
        let rms_port = rms_out.id;
        let peak_port = peak_out.id;
        let is_active_port = is_active_out.id;
        let voice_count_port = voice_count_out.id;

        let schema = ModuleSchema::new("voice", ModuleCategory::Output)
            .with_description("Synthesis voice — sine oscillator + SVF filter + ADSR envelope")
            .with_tier(ModuleTier::Infrastructure)
            .with_input(pitch_hz_in)
            .with_input(note_on_in)
            .with_input(note_off_in)
            .with_input(filter_cutoff_in)
            .with_input(amplitude_in)
            .with_output(rms_out)
            .with_output(peak_out)
            .with_output(is_active_out)
            .with_output(voice_count_out)
            .with_side_effect("audio_output");

        Self {
            schema,
            cmd_tx,
            analysis_rx,
            current_cutoff: DEFAULT_CUTOFF,
            current_amplitude: DEFAULT_AMPLITUDE,
            current_rms: 0.0,
            current_peak: 0.0,
            active_voices: 0,
            next_voice_id: 1,
            note_voices: HashMap::new(),
            pending_kills: Vec::new(),
            pitch_hz_port,
            note_on_port,
            note_off_port,
            filter_cutoff_port,
            amplitude_port,
            rms_port,
            peak_port,
            is_active_port,
            voice_count_port,
        }
    }
}

impl ModuleCore for VoiceModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.rms_port, Signal::Float(self.current_rms)));
        buffer.push((self.peak_port, Signal::Float(self.current_peak)));
        buffer.push((
            self.is_active_port,
            Signal::Bool(self.active_voices > 0),
        ));
        buffer.push((
            self.voice_count_port,
            Signal::Float(self.active_voices as f32),
        ));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.note_on_port {
            if let Signal::Float(val) = signal {
                let note = (val - NOTE_OFFSET).round().max(0.0) as u8;
                let hz = note_to_hz(note);

                // Retrigger: if this note already has a voice, release it first
                if let Some(old_id) = self.note_voices.remove(&note) {
                    let _ = self.cmd_tx.try_send(AudioCommand::KillVoice(old_id));
                    self.pending_kills.retain(|(vid, _)| *vid != old_id);
                }

                let id = self.next_voice_id;
                self.next_voice_id += 1;

                let _ = self.cmd_tx.try_send(AudioCommand::SpawnVoice {
                    id,
                    freq: hz,
                    cutoff: self.current_cutoff,
                    amp: self.current_amplitude,
                });

                self.note_voices.insert(note, id);
                self.pending_kills.push((id, AUTO_RELEASE_SECS));

                // If we have more tracked voices than MAX, kill the oldest
                while self.note_voices.len() > 8 {
                    // Find the note with the lowest voice_id (oldest)
                    if let Some((&oldest_note, &oldest_id)) =
                        self.note_voices.iter().min_by_key(|(_, &vid)| vid)
                    {
                        let _ = self.cmd_tx.try_send(AudioCommand::KillVoice(oldest_id));
                        self.note_voices.remove(&oldest_note);
                        self.pending_kills.retain(|(vid, _)| *vid != oldest_id);
                    }
                }

                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.note_off_port {
            if let Signal::Float(val) = signal {
                let note = (val - NOTE_OFFSET).round().max(0.0) as u8;
                if let Some(voice_id) = self.note_voices.remove(&note) {
                    let _ = self.cmd_tx.try_send(AudioCommand::KillVoice(voice_id));
                    self.pending_kills.retain(|(vid, _)| *vid != voice_id);
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.pitch_hz_port {
            if let Signal::Float(hz) = signal {
                // Refine pitch on all active voices
                if hz >= 20.0 {
                    let hz = hz.clamp(20.0, 20000.0);
                    for &vid in self.note_voices.values() {
                        let _ = self.cmd_tx.try_send(AudioCommand::SetParam {
                            id: vid,
                            param: VoiceParam::Frequency,
                            value: hz,
                        });
                    }
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.filter_cutoff_port {
            if let Signal::Float(hz) = signal {
                self.current_cutoff = hz.clamp(20.0, 20000.0);
                for &vid in self.note_voices.values() {
                    let _ = self.cmd_tx.try_send(AudioCommand::SetParam {
                        id: vid,
                        param: VoiceParam::Cutoff,
                        value: self.current_cutoff,
                    });
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.amplitude_port {
            if let Signal::Float(a) = signal {
                self.current_amplitude = a.clamp(0.0, 1.0);
                for &vid in self.note_voices.values() {
                    let _ = self.cmd_tx.try_send(AudioCommand::SetParam {
                        id: vid,
                        param: VoiceParam::Amplitude,
                        value: self.current_amplitude,
                    });
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, dt: f32) {
        // Drain analysis from audio thread
        while let Some(analysis) = self.analysis_rx.try_recv() {
            self.current_rms = analysis.rms;
            self.current_peak = analysis.peak;
            self.active_voices = analysis.active_count;
        }

        // Safety-net auto-kill: decrement timers, send KillVoice for expired
        for kill in &mut self.pending_kills {
            kill.1 -= dt;
        }
        let expired: Vec<u64> = self
            .pending_kills
            .iter()
            .filter(|(_, t)| *t <= 0.0)
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            let _ = self.cmd_tx.try_send(AudioCommand::KillVoice(*id));
            self.note_voices.retain(|_, vid| vid != id);
        }
        self.pending_kills.retain(|(_, t)| *t > 0.0);

        if self.active_voices > 0 {
            log::debug!(
                "[voice] rms={:.3} peak={:.3} voices={}",
                self.current_rms,
                self.current_peak,
                self.active_voices
            );
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl VoiceModule {
    pub fn current_cutoff(&self) -> f32 {
        self.current_cutoff
    }

    pub fn current_amplitude(&self) -> f32 {
        self.current_amplitude
    }

    pub fn current_rms(&self) -> f32 {
        self.current_rms
    }

    pub fn current_peak(&self) -> f32 {
        self.current_peak
    }

    pub fn active_voices(&self) -> u32 {
        self.active_voices
    }

    pub fn pending_kills_count(&self) -> usize {
        self.pending_kills.len()
    }

    pub fn tracked_voice_count(&self) -> usize {
        self.note_voices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::channel;

    fn test_channels() -> (
        Sender<AudioCommand>,
        Receiver<AudioCommand>,
        Sender<AudioAnalysis>,
        Receiver<AudioAnalysis>,
    ) {
        let (cmd_tx, cmd_rx) = channel::channel::<AudioCommand>(64);
        let (analysis_tx, analysis_rx) = channel::channel::<AudioAnalysis>(64);
        (cmd_tx, cmd_rx, analysis_tx, analysis_rx)
    }

    #[test]
    fn schema_has_correct_ports() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let module = VoiceModule::new(cmd_tx, analysis_rx);
        let schema = module.schema();

        assert_eq!(schema.name, "voice");
        assert_eq!(schema.category, ModuleCategory::Output);
        assert_eq!(schema.inputs.len(), 5);
        assert_eq!(schema.outputs.len(), 4);
        assert!(schema.input("pitch_hz").is_some());
        assert!(schema.input("note_on").is_some());
        assert!(schema.input("note_off").is_some());
        assert!(schema.input("filter_cutoff").is_some());
        assert!(schema.input("amplitude").is_some());
        assert!(schema.output("rms").is_some());
        assert!(schema.output("peak").is_some());
        assert!(schema.output("is_active").is_some());
        assert!(schema.output("voice_count").is_some());
        assert!(schema.side_effects.contains(&"audio_output".to_string()));
    }

    #[test]
    fn note_on_spawns_voice() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Num4 = note 3, signal value = 13.0
        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();

        // Should get a SpawnVoice command
        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_some());
        if let Some(AudioCommand::SpawnVoice { freq, .. }) = cmd {
            // note 3 → 261.63 * 2^(3/6 * 2) = 261.63 * 2 = 523.26 Hz
            assert!(
                (freq - 523.26).abs() < 1.0,
                "Note 3 should be ~523 Hz, got {}",
                freq
            );
        } else {
            panic!("Expected SpawnVoice, got {:?}", cmd);
        }

        assert_eq!(module.tracked_voice_count(), 1);
    }

    #[test]
    fn note_off_releases_voice() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Note on
        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();
        // Drain SpawnVoice
        while cmd_rx.try_recv().is_some() {}

        // Note off
        module
            .receive_signal(module.note_off_port, Signal::Float(13.0))
            .unwrap();

        let cmd = cmd_rx.try_recv();
        assert!(
            matches!(cmd, Some(AudioCommand::KillVoice(_))),
            "Expected KillVoice, got {:?}",
            cmd
        );
        assert_eq!(module.tracked_voice_count(), 0);
    }

    #[test]
    fn retrigger_releases_old_voice() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // First note on
        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();
        while cmd_rx.try_recv().is_some() {}

        // Second note on (same note = retrigger)
        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();

        let mut cmds = Vec::new();
        while let Some(cmd) = cmd_rx.try_recv() {
            cmds.push(cmd);
        }

        // Should get KillVoice(old) then SpawnVoice(new)
        assert!(
            cmds.iter().any(|c| matches!(c, AudioCommand::KillVoice(_))),
            "Retrigger should kill old voice"
        );
        assert!(
            cmds.iter()
                .any(|c| matches!(c, AudioCommand::SpawnVoice { .. })),
            "Retrigger should spawn new voice"
        );
        assert_eq!(module.tracked_voice_count(), 1);
    }

    #[test]
    fn note_off_without_note_on_is_noop() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Note off for a note that was never on
        module
            .receive_signal(module.note_off_port, Signal::Float(13.0))
            .unwrap();

        assert!(cmd_rx.try_recv().is_none(), "Should not send any command");
    }

    #[test]
    fn polyphonic_notes() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Press 3 different notes
        for note_val in [10.0, 13.0, 16.0] {
            module
                .receive_signal(module.note_on_port, Signal::Float(note_val))
                .unwrap();
        }

        let mut spawn_count = 0;
        while let Some(cmd) = cmd_rx.try_recv() {
            if matches!(cmd, AudioCommand::SpawnVoice { .. }) {
                spawn_count += 1;
            }
        }
        assert_eq!(spawn_count, 3);
        assert_eq!(module.tracked_voice_count(), 3);

        // Release one note
        module
            .receive_signal(module.note_off_port, Signal::Float(13.0))
            .unwrap();
        assert_eq!(module.tracked_voice_count(), 2);
    }

    #[test]
    fn note_to_hz_values() {
        // Note 0 = C4 = 261.63 Hz
        assert!((note_to_hz(0) - 261.63).abs() < 1.0);
        // Note 3 = C5 = 523.26 Hz
        assert!((note_to_hz(3) - 523.26).abs() < 1.0);
        // Note 6 = C6 = 1046.5 Hz
        assert!((note_to_hz(6) - 1046.5).abs() < 1.0);
    }

    #[test]
    fn receive_filter_cutoff() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(module.filter_cutoff_port, Signal::Float(5000.0));
        assert!(result.is_ok());
        assert!((module.current_cutoff - 5000.0).abs() < 1e-6);
    }

    #[test]
    fn receive_amplitude() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(module.amplitude_port, Signal::Float(0.8));
        assert!(result.is_ok());
        assert!((module.current_amplitude - 0.8).abs() < 1e-6);
    }

    #[test]
    fn reject_wrong_type_on_note_on() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(module.note_on_port, Signal::Trigger);
        assert!(matches!(result, Err(SignalError::WrongType { .. })));
    }

    #[test]
    fn reject_unknown_port() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(PortId(99999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn emit_signals_returns_4() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 4);
    }

    #[test]
    fn tick_drains_analysis() {
        let (cmd_tx, _cmd_rx, mut analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        analysis_tx
            .try_send(AudioAnalysis {
                rms: 0.42,
                peak: 0.85,
                active_count: 3,
            })
            .unwrap();

        module.tick(1.0 / 60.0);

        assert!((module.current_rms - 0.42).abs() < 1e-6);
        assert!((module.current_peak - 0.85).abs() < 1e-6);
        assert_eq!(module.active_voices, 3);
    }

    #[test]
    fn max_voices_kills_oldest() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Spawn 9 notes (only 7 keys available, but test with values 10-18)
        // We'll use values that map to notes 0-8, but only 0-6 are valid keys
        // For the overflow test, use 9 distinct note values
        for i in 0..9u8 {
            module
                .receive_signal(module.note_on_port, Signal::Float(i as f32 + 10.0))
                .unwrap();
        }

        // Drain all commands
        let mut cmds = Vec::new();
        while let Some(cmd) = cmd_rx.try_recv() {
            cmds.push(cmd);
        }

        let spawn_count = cmds
            .iter()
            .filter(|c| matches!(c, AudioCommand::SpawnVoice { .. }))
            .count();
        let kill_count = cmds
            .iter()
            .filter(|c| matches!(c, AudioCommand::KillVoice(_)))
            .count();
        assert_eq!(spawn_count, 9);
        // 1 kill for overflow (oldest note stolen)
        assert_eq!(kill_count, 1);
        assert_eq!(module.tracked_voice_count(), 8);
    }

    #[test]
    fn auto_kill_fires_after_timeout() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();
        // Drain SpawnVoice
        while cmd_rx.try_recv().is_some() {}

        // Tick enough to exceed AUTO_RELEASE_SECS (10.0s)
        // 10.0s / (1/60) = 600 ticks
        for _ in 0..610 {
            module.tick(1.0 / 60.0);
        }

        // Should have sent KillVoice
        let cmd = cmd_rx.try_recv();
        assert!(
            matches!(cmd, Some(AudioCommand::KillVoice(_))),
            "Expected KillVoice after timeout, got {:?}",
            cmd
        );
        assert_eq!(module.tracked_voice_count(), 0);
    }

    #[test]
    fn clamp_amplitude() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        module
            .receive_signal(module.amplitude_port, Signal::Float(5.0))
            .unwrap();
        assert!((module.current_amplitude - 1.0).abs() < 1e-6);

        module
            .receive_signal(module.amplitude_port, Signal::Float(-1.0))
            .unwrap();
        assert!((module.current_amplitude - 0.0).abs() < 1e-6);
    }

    #[test]
    fn pitch_hz_refines_active_voices() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Spawn a voice
        module
            .receive_signal(module.note_on_port, Signal::Float(13.0))
            .unwrap();
        while cmd_rx.try_recv().is_some() {}

        // Send pitch_hz refinement
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(440.0))
            .unwrap();

        let cmd = cmd_rx.try_recv();
        assert!(
            matches!(cmd, Some(AudioCommand::SetParam { param: VoiceParam::Frequency, value, .. }) if (value - 440.0).abs() < 1e-3),
            "pitch_hz should send SetParam Frequency, got {:?}",
            cmd
        );
    }
}
