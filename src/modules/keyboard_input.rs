use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};

use super::key::SolidoKey;

/// Keyboard input module — maps key presses to typed signals.
///
/// Number keys 1-7 emit pitch values on `raw_pitch` and a trigger.
/// Arrow keys emit gravity/tempo deltas. Space emits a trigger.
pub struct KeyboardInputModule {
    schema: ModuleSchema,
    pending_keys: Vec<SolidoKey>,
    raw_pitch_port: PortId,
    trigger_port: PortId,
    gravity_delta_port: PortId,
    tempo_delta_port: PortId,
}

impl KeyboardInputModule {
    pub fn new() -> Self {
        let raw_pitch = Port::output("raw_pitch", SignalType::Float, PortRate::Event)
            .with_range(0.0, 1.0)
            .with_description("Normalized pitch from number keys 1-7");
        let trigger = Port::output("trigger", SignalType::Trigger, PortRate::Event)
            .with_description("Bang on any note key or space");
        let gravity_delta = Port::output("gravity_delta", SignalType::Float, PortRate::Event)
            .with_range(-1.0, 1.0)
            .with_description("Pitch gravity adjustment from arrow up/down");
        let tempo_delta = Port::output("tempo_delta", SignalType::Float, PortRate::Event)
            .with_range(-1.0, 1.0)
            .with_description("Tempo adjustment from arrow left/right");

        let raw_pitch_port = raw_pitch.id;
        let trigger_port = trigger.id;
        let gravity_delta_port = gravity_delta.id;
        let tempo_delta_port = tempo_delta.id;

        let schema = ModuleSchema::new("keyboard_input", ModuleCategory::Input)
            .with_description("Maps keyboard events to pitch, trigger, and control signals")
            .with_output(raw_pitch)
            .with_output(trigger)
            .with_output(gravity_delta)
            .with_output(tempo_delta);

        Self {
            schema,
            pending_keys: Vec::new(),
            raw_pitch_port,
            trigger_port,
            gravity_delta_port,
            tempo_delta_port,
        }
    }

    /// Feed a key press event. Call before reactor tick.
    pub fn feed_key(&mut self, key: SolidoKey) {
        self.pending_keys.push(key);
    }

}

impl ModuleCore for KeyboardInputModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        for key in self.pending_keys.drain(..) {
            // Note keys → pitch + trigger
            if let Some(pitch) = key.pitch_value() {
                buffer.push((self.raw_pitch_port, Signal::Float(pitch)));
                buffer.push((self.trigger_port, Signal::Trigger));
            }

            // Arrow up/down → gravity delta
            if let Some(delta) = key.gravity_delta() {
                buffer.push((self.gravity_delta_port, Signal::Float(delta)));
            }

            // Arrow left/right → tempo delta
            if let Some(delta) = key.tempo_delta() {
                buffer.push((self.tempo_delta_port, Signal::Float(delta)));
            }

            // Space, R, T, P, Escape → trigger
            match key {
                SolidoKey::Space
                | SolidoKey::R
                | SolidoKey::T
                | SolidoKey::P
                | SolidoKey::Escape => {
                    buffer.push((self.trigger_port, Signal::Trigger));
                }
                _ => {}
            }
        }
    }

    fn receive_signal(&mut self, port: PortId, _signal: Signal) -> Result<(), SignalError> {
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // pending_keys already drained in emit_signals
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_key_emits_pitch_and_trigger() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Num4);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 2);
        // First: pitch float
        assert_eq!(buffer[0].0, module.raw_pitch_port);
        if let Signal::Float(v) = &buffer[0].1 {
            assert!((v - 0.5).abs() < 1e-5, "Num4 should be 0.5, got {v}");
        } else {
            panic!("expected Float signal");
        }
        // Second: trigger
        assert_eq!(buffer[1].0, module.trigger_port);
        assert!(matches!(buffer[1].1, Signal::Trigger));
    }

    #[test]
    fn arrow_keys_emit_deltas() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::ArrowUp);
        module.feed_key(SolidoKey::ArrowLeft);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 2);
        // ArrowUp → gravity_delta +0.1
        assert_eq!(buffer[0].0, module.gravity_delta_port);
        if let Signal::Float(v) = &buffer[0].1 {
            assert!((v - 0.1).abs() < 1e-5);
        } else {
            panic!("expected Float signal for gravity_delta");
        }
        // ArrowLeft → tempo_delta -5.0
        assert_eq!(buffer[1].0, module.tempo_delta_port);
        if let Signal::Float(v) = &buffer[1].1 {
            assert!((v - (-5.0)).abs() < 1e-5);
        } else {
            panic!("expected Float signal for tempo_delta");
        }
    }

    #[test]
    fn action_keys_emit_triggers() {
        for key in [SolidoKey::R, SolidoKey::T, SolidoKey::P, SolidoKey::Escape] {
            let mut module = KeyboardInputModule::new();
            module.feed_key(key);

            let mut buffer = Vec::new();
            module.emit_signals(&mut buffer);

            assert_eq!(buffer.len(), 1, "key {:?} should emit exactly 1 trigger", key);
            assert_eq!(buffer[0].0, module.trigger_port);
            assert!(matches!(buffer[0].1, Signal::Trigger));
        }
    }

    #[test]
    fn space_emits_trigger_only() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Space);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].0, module.trigger_port);
        assert!(matches!(buffer[0].1, Signal::Trigger));
    }

    #[test]
    fn multiple_keys_in_one_tick() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Num1);
        module.feed_key(SolidoKey::Num7);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // Num1: pitch + trigger, Num7: pitch + trigger = 4 signals
        assert_eq!(buffer.len(), 4);
    }

    #[test]
    fn no_keys_no_signals() {
        let mut module = KeyboardInputModule::new();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert!(buffer.is_empty());
    }

    #[test]
    fn receive_signal_returns_unknown_port() {
        let mut module = KeyboardInputModule::new();
        let result = module.receive_signal(PortId(999), Signal::Float(1.0));
        assert!(result.is_err());
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn schema_has_correct_ports() {
        let module = KeyboardInputModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "keyboard_input");
        assert_eq!(schema.category, ModuleCategory::Input);
        assert_eq!(schema.outputs.len(), 4);
        assert_eq!(schema.inputs.len(), 0);
        assert!(schema.output("raw_pitch").is_some());
        assert!(schema.output("trigger").is_some());
        assert!(schema.output("gravity_delta").is_some());
        assert!(schema.output("tempo_delta").is_some());
    }

    #[test]
    fn keys_cleared_after_emit() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Num1);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 2);

        // Second emit should produce nothing
        buffer.clear();
        module.emit_signals(&mut buffer);
        assert!(buffer.is_empty());
    }
}
