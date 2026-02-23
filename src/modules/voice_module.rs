use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::substrate::audio::{AudioAnalysis, AudioCommand, VoiceParam};
use crate::substrate::channel::{Receiver, Sender};

/// Default filter cutoff in Hz.
const DEFAULT_CUTOFF: f32 = 2000.0;
/// Default voice amplitude.
const DEFAULT_AMPLITUDE: f32 = 0.5;
/// How long a voice plays before auto-release (seconds).
const AUTO_RELEASE_SECS: f32 = 0.5;

/// Audio output module — bridges the affinity graph to the audio thread.
///
/// Receives `pitch_hz` and `trigger` signals from upstream modules (e.g.
/// QuantizerModule), sends AudioCommands to the VoicePool via lock-free
/// ring buffer, and emits analysis signals (rms, peak) back into the graph.
pub struct VoiceModule {
    schema: ModuleSchema,
    cmd_tx: Sender<AudioCommand>,
    analysis_rx: Receiver<AudioAnalysis>,
    // Current parameter state
    current_pitch_hz: f32,
    current_cutoff: f32,
    current_amplitude: f32,
    // Analysis from audio thread
    current_rms: f32,
    current_peak: f32,
    active_voices: u32,
    // Voice tracking
    next_voice_id: u64,
    voice_ids: Vec<u64>,
    pending_kills: Vec<(u64, f32)>, // (voice_id, time_remaining_secs)
    // Port IDs
    pitch_hz_port: PortId,
    trigger_port: PortId,
    filter_cutoff_port: PortId,
    amplitude_port: PortId,
    rms_port: PortId,
    peak_port: PortId,
    is_active_port: PortId,
    voice_count_port: PortId,
}

impl VoiceModule {
    pub fn new(cmd_tx: Sender<AudioCommand>, analysis_rx: Receiver<AudioAnalysis>) -> Self {
        let pitch_hz_in = Port::input("pitch_hz", SignalType::Float, PortRate::Event)
            .with_range(20.0, 20000.0)
            .with_description("Pitch frequency in Hz from quantizer");
        let trigger_in = Port::input("trigger", SignalType::Trigger, PortRate::Event)
            .with_description("Note-on trigger");
        let filter_cutoff_in = Port::input("filter_cutoff", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
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
        let trigger_port = trigger_in.id;
        let filter_cutoff_port = filter_cutoff_in.id;
        let amplitude_port = amplitude_in.id;
        let rms_port = rms_out.id;
        let peak_port = peak_out.id;
        let is_active_port = is_active_out.id;
        let voice_count_port = voice_count_out.id;

        let schema = ModuleSchema::new("voice", ModuleCategory::Output)
            .with_description("Synthesis voice — sine oscillator + SVF filter + ADSR envelope")
            .with_input(pitch_hz_in)
            .with_input(trigger_in)
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
            current_pitch_hz: 261.63,
            current_cutoff: DEFAULT_CUTOFF,
            current_amplitude: DEFAULT_AMPLITUDE,
            current_rms: 0.0,
            current_peak: 0.0,
            active_voices: 0,
            next_voice_id: 1,
            voice_ids: Vec::new(),
            pending_kills: Vec::new(),
            pitch_hz_port,
            trigger_port,
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
        if port == self.pitch_hz_port {
            if let Signal::Float(hz) = signal {
                self.current_pitch_hz = hz.clamp(20.0, 20000.0);
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.trigger_port {
            if let Signal::Trigger = signal {
                let id = self.next_voice_id;
                self.next_voice_id += 1;

                let _ = self.cmd_tx.try_send(AudioCommand::SpawnVoice {
                    id,
                    freq: self.current_pitch_hz,
                    cutoff: self.current_cutoff,
                    amp: self.current_amplitude,
                });

                self.voice_ids.push(id);
                self.pending_kills.push((id, AUTO_RELEASE_SECS));

                // If we have more tracked voices than MAX, kill the oldest
                while self.voice_ids.len() > 8 {
                    if let Some(old_id) = self.voice_ids.first().copied() {
                        let _ = self.cmd_tx.try_send(AudioCommand::KillVoice(old_id));
                        self.voice_ids.remove(0);
                        self.pending_kills.retain(|(vid, _)| *vid != old_id);
                    }
                }

                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        if port == self.filter_cutoff_port {
            if let Signal::Float(hz) = signal {
                self.current_cutoff = hz.clamp(20.0, 20000.0);
                for &vid in &self.voice_ids {
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
                for &vid in &self.voice_ids {
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

        // Auto-kill: decrement timers, send KillVoice for expired
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
            self.voice_ids.retain(|v| v != id);
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

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
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
        assert_eq!(schema.inputs.len(), 4);
        assert_eq!(schema.outputs.len(), 4);
        assert!(schema.input("pitch_hz").is_some());
        assert!(schema.input("trigger").is_some());
        assert!(schema.input("filter_cutoff").is_some());
        assert!(schema.input("amplitude").is_some());
        assert!(schema.output("rms").is_some());
        assert!(schema.output("peak").is_some());
        assert!(schema.output("is_active").is_some());
        assert!(schema.output("voice_count").is_some());
        assert!(schema.side_effects.contains(&"audio_output".to_string()));
    }

    #[test]
    fn receive_pitch_hz() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(module.pitch_hz_port, Signal::Float(440.0));
        assert!(result.is_ok());
        assert!((module.current_pitch_hz - 440.0).abs() < 1e-6);
    }

    #[test]
    fn receive_trigger_sends_spawn() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Set pitch first
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(440.0))
            .unwrap();
        // Trigger
        module
            .receive_signal(module.trigger_port, Signal::Trigger)
            .unwrap();

        // Verify SpawnVoice was sent
        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_some());
        if let Some(AudioCommand::SpawnVoice { freq, .. }) = cmd {
            assert!((freq - 440.0).abs() < 1e-6);
        } else {
            panic!("Expected SpawnVoice, got {:?}", cmd);
        }
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
    fn reject_wrong_type_on_trigger() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        let result = module.receive_signal(module.trigger_port, Signal::Float(1.0));
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
    fn max_voices_sends_kill() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        // Trigger 9 times
        for _ in 0..9 {
            module
                .receive_signal(module.trigger_port, Signal::Trigger)
                .unwrap();
        }

        // Drain all commands
        let mut cmds = Vec::new();
        while let Some(cmd) = cmd_rx.try_recv() {
            cmds.push(cmd);
        }

        // Should have 9 SpawnVoice + 1 KillVoice (for the oldest)
        let spawn_count = cmds
            .iter()
            .filter(|c| matches!(c, AudioCommand::SpawnVoice { .. }))
            .count();
        let kill_count = cmds
            .iter()
            .filter(|c| matches!(c, AudioCommand::KillVoice(_)))
            .count();
        assert_eq!(spawn_count, 9);
        assert_eq!(kill_count, 1);
    }

    #[test]
    fn auto_kill_fires_after_timeout() {
        let (cmd_tx, mut cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);

        module
            .receive_signal(module.trigger_port, Signal::Trigger)
            .unwrap();
        // Drain the SpawnVoice
        cmd_rx.try_recv();

        // Tick enough to exceed AUTO_RELEASE_SECS (0.5s)
        for _ in 0..35 {
            module.tick(1.0 / 60.0);
        }

        // Should have sent KillVoice
        let cmd = cmd_rx.try_recv();
        assert!(
            matches!(cmd, Some(AudioCommand::KillVoice(_))),
            "Expected KillVoice after timeout, got {:?}",
            cmd
        );
    }

    #[test]
    fn clamp_pitch_hz() {
        let (cmd_tx, _cmd_rx, _analysis_tx, analysis_rx) = test_channels();
        let mut module = VoiceModule::new(cmd_tx, analysis_rx);
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(50000.0))
            .unwrap();
        assert!((module.current_pitch_hz - 20000.0).abs() < 1e-6);

        module
            .receive_signal(module.pitch_hz_port, Signal::Float(1.0))
            .unwrap();
        assert!((module.current_pitch_hz - 20.0).abs() < 1e-6);
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
}
