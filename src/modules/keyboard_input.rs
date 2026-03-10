use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};

use super::key::SolidoKey;

/// Event: a key was pressed. Send via `receive_event`.
pub struct KeyPress(pub SolidoKey);

/// Event: a key was released. Send via `receive_event`.
pub struct KeyRelease(pub SolidoKey);

/// Keyboard input module — maps key presses to typed signals.
///
/// Number keys 1-7 emit pitch values on `raw_pitch` and a trigger.
/// Arrow keys emit gravity/tempo deltas. Space emits a trigger.
pub struct KeyboardInputModule {
    schema: ModuleSchema,
    pending_keys: Vec<SolidoKey>,
    pending_releases: Vec<SolidoKey>,
    raw_pitch_port: PortId,
    trigger_port: PortId,
    gravity_delta_port: PortId,
    tempo_delta_port: PortId,
    release_pitch_port: PortId,
    note_on_port: PortId,
    note_off_port: PortId,
    raga_cycle_port: PortId,
    tala_cycle_port: PortId,
    scale_cycle_port: PortId,
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
        let release_pitch = Port::output("release_pitch", SignalType::Float, PortRate::Event)
            .with_range(0.0, 1.0)
            .with_description("Normalized pitch of released key — triggers note-off");
        let note_on = Port::output("note_on", SignalType::Float, PortRate::Event)
            .with_range(10.0, 16.0)
            .with_description("Note-on: value = note_number + 10 (range avoids spurious edges)");
        let note_off = Port::output("note_off", SignalType::Float, PortRate::Event)
            .with_range(10.0, 16.0)
            .with_description("Note-off: value = note_number + 10 (range avoids spurious edges)");
        let raga_cycle = Port::output("raga_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Fires on R key — cycles raga selection");
        let tala_cycle = Port::output("tala_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Fires on T key — cycles tala selection");
        let scale_cycle = Port::output("scale_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Fires on S key — cycles Western scale selection");

        let raw_pitch_port = raw_pitch.id;
        let trigger_port = trigger.id;
        let gravity_delta_port = gravity_delta.id;
        let tempo_delta_port = tempo_delta.id;
        let release_pitch_port = release_pitch.id;
        let note_on_port = note_on.id;
        let note_off_port = note_off.id;
        let raga_cycle_port = raga_cycle.id;
        let tala_cycle_port = tala_cycle.id;
        let scale_cycle_port = scale_cycle.id;

        let schema = ModuleSchema::new("keyboard_input", ModuleCategory::Input)
            .with_description("Maps keyboard events to pitch, trigger, and control signals")
            .with_tier(ModuleTier::Infrastructure)
            .with_output(raw_pitch)
            .with_output(trigger)
            .with_output(gravity_delta)
            .with_output(tempo_delta)
            .with_output(release_pitch)
            .with_output(note_on)
            .with_output(note_off)
            .with_output(raga_cycle)
            .with_output(tala_cycle)
            .with_output(scale_cycle);

        Self {
            schema,
            pending_keys: Vec::new(),
            pending_releases: Vec::new(),
            raw_pitch_port,
            trigger_port,
            gravity_delta_port,
            tempo_delta_port,
            release_pitch_port,
            note_on_port,
            note_off_port,
            raga_cycle_port,
            tala_cycle_port,
            scale_cycle_port,
        }
    }

    /// Feed a key press event. Call before reactor tick.
    pub fn feed_key(&mut self, key: SolidoKey) {
        self.pending_keys.push(key);
    }

    /// Feed a key release event. Call before reactor tick.
    pub fn feed_key_release(&mut self, key: SolidoKey) {
        self.pending_releases.push(key);
    }
}

impl ModuleCore for KeyboardInputModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        for key in self.pending_keys.drain(..) {
            // Note keys → raw_pitch + note_on + trigger
            if let Some(pitch) = key.pitch_value() {
                buffer.push((self.raw_pitch_port, Signal::Float(pitch)));
            }
            if let Some(note) = key.note_number() {
                buffer.push((self.note_on_port, Signal::Float(note as f32 + 10.0)));
            }
            if key.pitch_value().is_some() {
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

            // Dedicated cycle triggers
            match key {
                SolidoKey::R => {
                    buffer.push((self.raga_cycle_port, Signal::Trigger));
                }
                SolidoKey::T => {
                    buffer.push((self.tala_cycle_port, Signal::Trigger));
                }
                SolidoKey::S => {
                    buffer.push((self.scale_cycle_port, Signal::Trigger));
                }
                _ => {}
            }

            // Space, R, T, S, P, Escape → generic trigger
            match key {
                SolidoKey::Space
                | SolidoKey::R
                | SolidoKey::T
                | SolidoKey::S
                | SolidoKey::P
                | SolidoKey::Escape => {
                    buffer.push((self.trigger_port, Signal::Trigger));
                }
                _ => {}
            }
        }

        // Key releases → release_pitch + note_off
        for key in self.pending_releases.drain(..) {
            if let Some(pitch) = key.pitch_value() {
                buffer.push((self.release_pitch_port, Signal::Float(pitch)));
            }
            if let Some(note) = key.note_number() {
                buffer.push((self.note_off_port, Signal::Float(note as f32 + 10.0)));
            }
        }
    }

    fn receive_signal(&mut self, port: PortId, _signal: Signal) -> Result<(), SignalError> {
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // pending_keys and pending_releases already drained in emit_signals
    }

    #[allow(deprecated)]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[allow(deprecated)]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn receive_event(&mut self, event: &dyn Any) {
        if let Some(KeyPress(key)) = event.downcast_ref::<KeyPress>() {
            self.feed_key(*key);
        } else if let Some(KeyRelease(key)) = event.downcast_ref::<KeyRelease>() {
            self.feed_key_release(*key);
        }
    }
}

impl KeyboardInputModule {
    pub fn pending_key_count(&self) -> usize {
        self.pending_keys.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_key_emits_pitch_note_on_and_trigger() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Num4);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // raw_pitch + note_on + trigger = 3 signals
        assert_eq!(buffer.len(), 3);
        // First: raw_pitch
        assert_eq!(buffer[0].0, module.raw_pitch_port);
        if let Signal::Float(v) = &buffer[0].1 {
            assert!((v - 0.5).abs() < 1e-5, "Num4 should be 0.5, got {v}");
        } else {
            panic!("expected Float signal for raw_pitch");
        }
        // Second: note_on (Num4 = note 3 + 10 = 13.0)
        assert_eq!(buffer[1].0, module.note_on_port);
        if let Signal::Float(v) = &buffer[1].1 {
            assert!((v - 13.0).abs() < 1e-5, "Num4 note_on should be 13.0, got {v}");
        } else {
            panic!("expected Float signal for note_on");
        }
        // Third: trigger
        assert_eq!(buffer[2].0, module.trigger_port);
        assert!(matches!(buffer[2].1, Signal::Trigger));
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
        // P and Escape emit only generic trigger (1 signal)
        for key in [SolidoKey::P, SolidoKey::Escape] {
            let mut module = KeyboardInputModule::new();
            module.feed_key(key);

            let mut buffer = Vec::new();
            module.emit_signals(&mut buffer);

            assert_eq!(buffer.len(), 1, "key {:?} should emit exactly 1 trigger", key);
            assert_eq!(buffer[0].0, module.trigger_port);
            assert!(matches!(buffer[0].1, Signal::Trigger));
        }

        // R emits raga_cycle + generic trigger (2 signals)
        {
            let mut module = KeyboardInputModule::new();
            module.feed_key(SolidoKey::R);
            let mut buffer = Vec::new();
            module.emit_signals(&mut buffer);
            assert_eq!(buffer.len(), 2, "R should emit raga_cycle + trigger");
            assert_eq!(buffer[0].0, module.raga_cycle_port);
            assert!(matches!(buffer[0].1, Signal::Trigger));
            assert_eq!(buffer[1].0, module.trigger_port);
        }

        // T emits tala_cycle + generic trigger (2 signals)
        {
            let mut module = KeyboardInputModule::new();
            module.feed_key(SolidoKey::T);
            let mut buffer = Vec::new();
            module.emit_signals(&mut buffer);
            assert_eq!(buffer.len(), 2, "T should emit tala_cycle + trigger");
            assert_eq!(buffer[0].0, module.tala_cycle_port);
            assert!(matches!(buffer[0].1, Signal::Trigger));
            assert_eq!(buffer[1].0, module.trigger_port);
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

        // Num1: raw_pitch + note_on + trigger, Num7: same = 6 signals
        assert_eq!(buffer.len(), 6);
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
        assert_eq!(schema.outputs.len(), 10);
        assert_eq!(schema.inputs.len(), 0);
        assert!(schema.output("raw_pitch").is_some());
        assert!(schema.output("trigger").is_some());
        assert!(schema.output("gravity_delta").is_some());
        assert!(schema.output("tempo_delta").is_some());
        assert!(schema.output("release_pitch").is_some());
        assert!(schema.output("note_on").is_some());
        assert!(schema.output("note_off").is_some());
        assert!(schema.output("raga_cycle").is_some());
        assert!(schema.output("tala_cycle").is_some());
        assert!(schema.output("scale_cycle").is_some());
    }

    #[test]
    fn key_release_emits_release_pitch_and_note_off() {
        let mut module = KeyboardInputModule::new();
        module.feed_key_release(SolidoKey::Num4);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // release_pitch + note_off = 2 signals
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].0, module.release_pitch_port);
        if let Signal::Float(v) = &buffer[0].1 {
            assert!((v - 0.5).abs() < 1e-5, "Num4 release should be 0.5, got {v}");
        } else {
            panic!("expected Float signal for release_pitch");
        }
        // note_off (Num4 = note 3 + 10 = 13.0)
        assert_eq!(buffer[1].0, module.note_off_port);
        if let Signal::Float(v) = &buffer[1].1 {
            assert!((v - 13.0).abs() < 1e-5, "Num4 note_off should be 13.0, got {v}");
        } else {
            panic!("expected Float signal for note_off");
        }
    }

    #[test]
    fn non_note_key_release_no_signal() {
        let mut module = KeyboardInputModule::new();
        module.feed_key_release(SolidoKey::Space);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert!(buffer.is_empty(), "Space release should not emit release_pitch");
    }

    #[test]
    fn keys_cleared_after_emit() {
        let mut module = KeyboardInputModule::new();
        module.feed_key(SolidoKey::Num1);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 3);

        // Second emit should produce nothing
        buffer.clear();
        module.emit_signals(&mut buffer);
        assert!(buffer.is_empty());
    }
}
