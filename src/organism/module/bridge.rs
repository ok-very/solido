//! S33 Scale/rhythm/gamaka bridge — receives infrastructure signals from
//! RagaModule and TalaModule and forwards DSP commands to the audio thread.

use crate::dsp::command::DspCommand;
use crate::module::signal::{Signal, SignalType};
use crate::module::{PortId, SignalError};

use super::OrganismModule;

impl OrganismModule {
    /// Try to handle a signal on one of the S33 bridge ports.
    /// Returns `Some(result)` if the port matched, `None` to fall through.
    pub(super) fn receive_bridge_signal(
        &mut self,
        port: PortId,
        signal: Signal,
    ) -> Option<Result<(), SignalError>> {
        if port == self.gravity_weights_port {
            return Some(self.receive_gravity_weights(signal));
        }
        if port == self.beat_phase_port {
            return Some(self.receive_beat_phase(signal));
        }
        if port == self.beat_trigger_port {
            return Some(self.receive_beat_trigger(signal));
        }
        if port == self.gamaka_config_port {
            return Some(self.receive_gamaka_config(signal));
        }
        None
    }

    /// Soft-sync nudge logic, called each tick.
    pub(super) fn tick_bridge(&mut self) {
        if self.rhythm_sync == 1 && self.dna.rhythm_affinity > 0.01 {
            // Nudge near beat boundaries (phase near 0 or 1)
            if self.current_beat_phase < 0.1 || self.current_beat_phase > 0.9 {
                let nudge = (self.current_beat_phase - 0.5).signum()
                    * 0.02
                    * self.dna.rhythm_affinity;
                let _ = self.cmd_tx.try_send(DspCommand::NudgePhase(nudge));
            }
        }
    }

    fn receive_gravity_weights(&mut self, signal: Signal) -> Result<(), SignalError> {
        if let Signal::Pattern(weights) = signal {
            self.current_gravity_weights = Some(weights);
            self.musical_context.scale_active = true;
            self.musical_context.scale_blend =
                self.dna.scale_affinity * self.dna.fidelity;
            // Environment dispatch in app.rs is the sole SetScaleWeights path.
            // We store weights here for inspection but do NOT send to audio thread.
            Ok(())
        } else {
            Err(SignalError::WrongType {
                expected: SignalType::Pattern,
                got: signal.signal_type(),
            })
        }
    }

    fn receive_beat_phase(&mut self, signal: Signal) -> Result<(), SignalError> {
        if let Signal::Float(phase) = signal {
            self.current_beat_phase = phase;
            self.musical_context.beat_phase = phase;
            // Soft sync nudge applied in tick()
            Ok(())
        } else {
            Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            })
        }
    }

    fn receive_beat_trigger(&mut self, signal: Signal) -> Result<(), SignalError> {
        if let Signal::Trigger = signal {
            self.musical_context.ticks_since_beat = 0;
            if self.rhythm_sync == 2 && self.dna.rhythm_affinity > 0.01 {
                // Hard sync: reset seq_cell phase to downbeat.
                // NudgePhase is additive, -1.0 wraps to 0.0 via rem_euclid(1.0).
                let _ = self.cmd_tx.try_send(DspCommand::NudgePhase(-1.0));
            }
            Ok(())
        } else {
            Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            })
        }
    }

    fn receive_gamaka_config(&mut self, signal: Signal) -> Result<(), SignalError> {
        if let Signal::Pattern(config) = signal {
            if config.len() >= 3 {
                let blend = self.dna.scale_affinity * self.dna.fidelity;
                // slide_ms → seconds, scaled by affinity
                let slide_s = (config[0] / 1000.0 * blend).max(0.001);
                let vib_depth = config[1] * blend;
                let vib_rate = config[2];
                let _ = self.cmd_tx.try_send(DspCommand::SetSlewRate(slide_s, slide_s));
                let _ = self.cmd_tx.try_send(DspCommand::SetLfoParams(vib_rate, vib_depth));
                // Update context: depth > threshold → vibrating, else idle
                self.musical_context.gamaka_depth = vib_depth;
                self.musical_context.gamaka_state = if vib_depth > 0.1 { 2 } else { 0 };
            }
            Ok(())
        } else {
            Err(SignalError::WrongType {
                expected: SignalType::Pattern,
                got: signal.signal_type(),
            })
        }
    }
}
