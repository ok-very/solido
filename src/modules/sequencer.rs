use std::sync::Arc;

use crate::dsp::shared::Shared;
use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::reactor::GlobalClock;

/// 16-step pattern sequencer (Infrastructure tier).
///
/// Human writes pattern via UI, module emits pitch/gate/accent/beat_phase per step.
/// Swing offsets even steps. BPM and step_count are runtime configurable.
pub struct SequencerModule {
    schema: ModuleSchema,

    // Pattern data (16 steps)
    steps: [StepData; 16],

    // Transport
    bpm: Shared,
    playing: Shared,
    step_count: u8,
    swing: Shared,

    // Runtime
    phase: f64,
    current_step: usize,
    last_step: usize,
    gate_fired_this_step: bool,

    // Port IDs
    step_pitch_port: PortId,
    step_gate_port: PortId,
    step_accent_port: PortId,
    beat_phase_port: PortId,
    step_index_port: PortId,
    /// Global clock reference — when set, overrides local bpm/playing.
    clock: Option<Arc<GlobalClock>>,
}

/// Per-step pattern data with Shared handles for UI control.
#[derive(Clone)]
pub struct StepData {
    pub pitch_hz: Shared,
    pub gate: Shared,
    pub accent: Shared,
    pub slide: Shared,
}

impl SequencerModule {
    pub fn new() -> Self {
        let step_pitch_out = Port::output("step_pitch", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Frequency of current step (Hz)");
        let step_gate_out = Port::output("step_gate", SignalType::Trigger, PortRate::Block)
            .with_description("Fires on gate-on steps");
        let step_accent_out = Port::output("step_accent", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Accent level of current step (0.0=normal, 1.0=accent)");
        let beat_phase_out = Port::output("beat_phase", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("0.0–1.0 ramp per beat");
        let step_index_out = Port::output("step_index", SignalType::Float, PortRate::Block)
            .with_range(0.0, 15.0)
            .with_description("Current step index (0–15)");

        let step_pitch_port = step_pitch_out.id;
        let step_gate_port = step_gate_out.id;
        let step_accent_port = step_accent_out.id;
        let beat_phase_port = beat_phase_out.id;
        let step_index_port = step_index_out.id;

        let schema = ModuleSchema::new("Sequencer", ModuleCategory::Input)
            .with_description("16-step pattern sequencer (Infrastructure tier)")
            .with_tier(ModuleTier::Infrastructure)
            .with_output(step_pitch_out)
            .with_output(step_gate_out)
            .with_output(step_accent_out)
            .with_output(beat_phase_out)
            .with_output(step_index_out);

        // Initialize 16 steps with default values
        let steps: [StepData; 16] = std::array::from_fn(|i| {
            let pitch = if i < 4 {
                match i {
                    0 => 261.63, // C4
                    1 => 293.66, // D4
                    2 => 329.63, // E4
                    3 => 392.0,  // G4
                    _ => 261.63,
                }
            } else {
                261.63
            };
            StepData {
                pitch_hz: Shared::new(pitch),
                gate: Shared::new(if i % 2 == 0 { 1.0 } else { 0.0 }),
                accent: Shared::new(if i == 0 { 1.0 } else { 0.0 }),
                slide: Shared::new(0.0),
            }
        });

        Self {
            schema,
            steps,
            bpm: Shared::new(120.0),
            playing: Shared::new(1.0),
            step_count: 16,
            swing: Shared::new(0.0),
            phase: 0.0,
            current_step: 0,
            last_step: usize::MAX,
            gate_fired_this_step: false,
            step_pitch_port,
            step_gate_port,
            step_accent_port,
            beat_phase_port,
            step_index_port,
            clock: None,
        }
    }

    /// Attach a global clock. SequencerModule will read BPM/playing from it.
    pub fn with_clock(mut self, clock: Arc<GlobalClock>) -> Self {
        self.bpm.set(clock.bpm_value());
        self.clock = Some(clock);
        self
    }

    /// Get step data at index for UI editing.
    pub fn step(&self, index: usize) -> Option<&StepData> {
        self.steps.get(index)
    }

    /// Get BPM handle.
    pub fn bpm(&self) -> &Shared {
        &self.bpm
    }

    /// Get playing handle.
    pub fn playing(&self) -> &Shared {
        &self.playing
    }

    /// Get swing handle.
    pub fn swing(&self) -> &Shared {
        &self.swing
    }

    /// Set the active step count (1-16).
    pub fn set_step_count(&mut self, count: u8) {
        self.step_count = count.clamp(1, 16);
    }

    /// Get the active step count.
    pub fn step_count(&self) -> u8 {
        self.step_count
    }

    /// Get current step index.
    pub fn current_step(&self) -> usize {
        self.current_step
    }
}

impl Default for SequencerModule {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleCore for SequencerModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        let step = &self.steps[self.current_step];
        let pitch = step.pitch_hz.value();
        let accent = step.accent.value();
        let beat_phase = (self.phase - self.current_step as f64) as f32;

        buffer.push((self.step_pitch_port, Signal::Float(pitch)));
        buffer.push((self.step_accent_port, Signal::Float(accent)));
        buffer.push((
            self.beat_phase_port,
            Signal::Float(beat_phase.clamp(0.0, 1.0)),
        ));
        buffer.push((
            self.step_index_port,
            Signal::Float(self.current_step as f32),
        ));

        // Emit gate trigger on step boundary
        if self.current_step != self.last_step && !self.gate_fired_this_step {
            let gate = step.gate.value();
            if gate > 0.5 {
                buffer.push((self.step_gate_port, Signal::Trigger));
                self.gate_fired_this_step = true;
            }
        }
    }

    fn receive_signal(&mut self, port: PortId, _signal: Signal) -> Result<(), SignalError> {
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, dt: f32) {
        // Read from global clock if attached, otherwise use local handles
        let (bpm, playing) = if let Some(ref clock) = self.clock {
            (clock.bpm_value(), clock.is_playing())
        } else {
            (self.bpm.value().clamp(20.0, 300.0), self.playing.value() > 0.5)
        };

        if !playing {
            return; // stopped
        }
        let swing_amt = self.swing.value().clamp(0.0, 1.0);

        // Calculate step duration in seconds
        let beats_per_sec = bpm / 60.0;
        let step_duration = 1.0 / beats_per_sec;

        // Apply swing offset to even steps
        let swing_offset = if self.current_step % 2 == 1 {
            swing_amt * step_duration * 0.5
        } else {
            0.0
        };

        let effective_dt = dt as f64 + swing_offset as f64;
        self.phase += beats_per_sec as f64 * effective_dt;

        // Advance to next step if phase crosses boundary
        let new_step = (self.phase.floor() as usize) % self.step_count as usize;
        if new_step != self.current_step {
            self.last_step = self.current_step;
            self.current_step = new_step;
            self.gate_fired_this_step = false;
        }
    }

    #[allow(deprecated)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[allow(deprecated)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_5_outputs() {
        let seq = SequencerModule::new();
        let schema = seq.schema();
        assert_eq!(schema.outputs.len(), 5);
        assert!(schema.output("step_pitch").is_some());
        assert!(schema.output("step_gate").is_some());
        assert!(schema.output("step_accent").is_some());
        assert!(schema.output("beat_phase").is_some());
        assert!(schema.output("step_index").is_some());
    }

    #[test]
    fn schema_tier_is_infrastructure() {
        let seq = SequencerModule::new();
        assert_eq!(seq.schema().tier, ModuleTier::Infrastructure);
    }

    #[test]
    fn default_step_count_is_16() {
        let seq = SequencerModule::new();
        assert_eq!(seq.step_count(), 16);
    }

    #[test]
    fn set_step_count_clamps() {
        let mut seq = SequencerModule::new();
        seq.set_step_count(0);
        assert_eq!(seq.step_count(), 1);
        seq.set_step_count(20);
        assert_eq!(seq.step_count(), 16);
        seq.set_step_count(8);
        assert_eq!(seq.step_count(), 8);
    }

    #[test]
    fn tick_advances_step() {
        let mut seq = SequencerModule::new();
        seq.bpm.set(120.0);
        assert_eq!(seq.current_step(), 0);

        // 120 BPM = 2 beats/sec = 0.5 sec/beat
        seq.tick(0.5);
        assert_eq!(seq.current_step(), 1);

        seq.tick(0.5);
        assert_eq!(seq.current_step(), 2);
    }

    #[test]
    fn tick_wraps_at_step_count() {
        let mut seq = SequencerModule::new();
        seq.bpm.set(120.0);
        seq.set_step_count(4);

        for _ in 0..4 {
            seq.tick(0.5);
        }
        assert_eq!(seq.current_step(), 0, "should wrap to step 0");
    }

    #[test]
    fn emit_signals_includes_gate_on_first_step() {
        let mut seq = SequencerModule::new();
        let mut buffer = Vec::new();
        seq.emit_signals(&mut buffer);
        // First emit on step 0 (gate enabled): pitch, accent, beat_phase, step_index, gate trigger
        assert_eq!(buffer.len(), 5);
        let has_gate = buffer
            .iter()
            .any(|(port, sig)| *port == seq.step_gate_port && matches!(sig, Signal::Trigger));
        assert!(has_gate, "should emit gate on first active step");
    }

    #[test]
    fn emit_gate_on_active_steps() {
        let mut seq = SequencerModule::new();
        seq.steps[1].gate.set(1.0); // step 1 is active
        seq.bpm.set(120.0);

        // Advance to step 1
        seq.tick(0.5);
        assert_eq!(seq.current_step(), 1);

        let mut buffer = Vec::new();
        seq.emit_signals(&mut buffer);

        let has_gate = buffer
            .iter()
            .any(|(port, sig)| *port == seq.step_gate_port && matches!(sig, Signal::Trigger));
        assert!(has_gate, "should emit gate trigger on active step");
    }

    #[test]
    fn no_gate_on_rest_steps() {
        let mut seq = SequencerModule::new();
        seq.steps[1].gate.set(0.0); // step 1 is rest
        seq.bpm.set(120.0);

        seq.tick(0.5);
        assert_eq!(seq.current_step(), 1);

        let mut buffer = Vec::new();
        seq.emit_signals(&mut buffer);

        let has_gate = buffer
            .iter()
            .any(|(port, sig)| *port == seq.step_gate_port && matches!(sig, Signal::Trigger));
        assert!(!has_gate, "should not emit gate on rest step");
    }

    #[test]
    fn beat_phase_ramps_0_to_1() {
        let mut seq = SequencerModule::new();
        seq.bpm.set(120.0);

        // Start of step 0
        let mut buffer = Vec::new();
        seq.emit_signals(&mut buffer);
        let phase = buffer
            .iter()
            .find(|(port, _)| *port == seq.beat_phase_port)
            .map(|(_, sig)| {
                if let Signal::Float(v) = sig {
                    *v
                } else {
                    -1.0
                }
            })
            .unwrap();
        assert!(phase >= 0.0 && phase < 0.5, "phase should be near 0.0");

        // Halfway through step
        seq.tick(0.25);
        buffer.clear();
        seq.emit_signals(&mut buffer);
        let phase = buffer
            .iter()
            .find(|(port, _)| *port == seq.beat_phase_port)
            .map(|(_, sig)| {
                if let Signal::Float(v) = sig {
                    *v
                } else {
                    -1.0
                }
            })
            .unwrap();
        assert!(phase >= 0.4 && phase <= 0.6, "phase should be ~0.5");
    }

    #[test]
    fn stop_halts_playback() {
        let mut seq = SequencerModule::new();
        seq.bpm.set(120.0);
        seq.playing.set(0.0); // stop

        let step_before = seq.current_step();
        seq.tick(1.0);
        assert_eq!(seq.current_step(), step_before, "should not advance when stopped");
    }

    #[test]
    fn step_data_accessible() {
        let seq = SequencerModule::new();
        let step0 = seq.step(0).unwrap();
        assert!(step0.pitch_hz.value() > 0.0);
        assert!(seq.step(16).is_none());
    }
}
