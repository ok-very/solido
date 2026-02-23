use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::tuning::pitch_gravity::{PitchGravity, PitchSmoother};
use crate::tuning::TuningRegistry;

const DEFAULT_SLEW_RATE: f64 = 2000.0;
const DEFAULT_TUNING: &str = "bhairav";

/// Pitch quantizer processing module.
///
/// Receives raw_pitch (Float) through the affinity graph and emits
/// quantized pitch_hz (Float) and nearest_degree (Float).
/// First Processing module in L3 — proves input->processing routing.
pub struct QuantizerModule {
    schema: ModuleSchema,
    gravity: PitchGravity,
    smoother: PitchSmoother,
    tuning_registry: TuningRegistry,
    current_tuning: String,
    override_gravity: Option<f32>,
    last_raw_pitch: Option<f32>,
    output_hz: f64,
    output_degree: f32,
    // Port IDs
    raw_pitch_port: PortId,
    gravity_override_port: PortId,
    pitch_hz_port: PortId,
    nearest_degree_port: PortId,
}

impl QuantizerModule {
    pub fn new() -> Self {
        let mut registry = TuningRegistry::new();
        registry.load_builtins();

        let default_ts = registry
            .get(DEFAULT_TUNING)
            .expect("default tuning must exist in builtins")
            .clone();
        let gravity = PitchGravity::new(default_ts);
        let smoother = PitchSmoother::new(DEFAULT_SLEW_RATE);

        let raw_pitch_in = Port::input("raw_pitch", SignalType::Float, PortRate::Event)
            .with_range(0.0, 1.0)
            .with_description("Normalized pitch input [0,1] from keyboard/cursor");
        let gravity_override_in =
            Port::input("gravity_override", SignalType::Float, PortRate::Block)
                .with_range(0.0, 1.0)
                .with_description("External gravity control [0,1]");

        let pitch_hz_out = Port::output("pitch_hz", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Quantized pitch in Hz");
        let nearest_degree_out =
            Port::output("nearest_degree", SignalType::Float, PortRate::Block)
                .with_range(0.0, 127.0)
                .with_description("Index of nearest scale degree");

        let raw_pitch_port = raw_pitch_in.id;
        let gravity_override_port = gravity_override_in.id;
        let pitch_hz_port = pitch_hz_out.id;
        let nearest_degree_port = nearest_degree_out.id;

        let schema = ModuleSchema::new("quantizer", ModuleCategory::Processing)
            .with_description("Pitch gravity quantizer — pulls pitch toward tuning degrees")
            .with_input(raw_pitch_in)
            .with_input(gravity_override_in)
            .with_output(pitch_hz_out)
            .with_output(nearest_degree_out);

        Self {
            schema,
            gravity,
            smoother,
            tuning_registry: registry,
            current_tuning: DEFAULT_TUNING.to_string(),
            override_gravity: None,
            last_raw_pitch: None,
            output_hz: 261.63,
            output_degree: 0.0,
            raw_pitch_port,
            gravity_override_port,
            pitch_hz_port,
            nearest_degree_port,
        }
    }

    /// Switch to a different tuning by name. Returns false if not found.
    pub fn set_tuning(&mut self, name: &str) -> bool {
        if let Some(ts) = self.tuning_registry.get(name) {
            self.gravity.tuning = ts.clone();
            self.gravity.degree_weights = vec![1.0; ts.cents.len()];
            self.current_tuning = name.to_string();
            true
        } else {
            false
        }
    }

    pub fn current_tuning(&self) -> &str {
        &self.current_tuning
    }

    pub fn available_tunings(&self) -> Vec<&str> {
        self.tuning_registry.list()
    }
}

impl ModuleCore for QuantizerModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.pitch_hz_port, Signal::Float(self.output_hz as f32)));
        buffer.push((self.nearest_degree_port, Signal::Float(self.output_degree)));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.raw_pitch_port {
            if let Signal::Float(v) = signal {
                self.last_raw_pitch = Some(v.clamp(0.0, 1.0));
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.gravity_override_port {
            if let Signal::Float(v) = signal {
                self.override_gravity = Some(v.clamp(0.0, 1.0));
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
        if let Some(g) = self.override_gravity {
            self.gravity.gravity = g;
        }

        if let Some(raw) = self.last_raw_pitch {
            let hz = self.gravity.quantize(raw);
            self.smoother.set_target(hz);

            let raw_cents = self.gravity.raw_to_cents(raw);
            let (degree_idx, _) = self.gravity.tuning.nearest_degree(raw_cents);
            self.output_degree = degree_idx as f32;
        }

        self.output_hz = self.smoother.tick(dt);

        log::debug!(
            "[quantizer] pitch_hz={:.2} degree={} gravity={:.2} tuning={}",
            self.output_hz,
            self.output_degree,
            self.gravity.gravity,
            self.current_tuning
        );
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_correct_ports() {
        let module = QuantizerModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "quantizer");
        assert_eq!(schema.category, ModuleCategory::Processing);
        assert_eq!(schema.inputs.len(), 2);
        assert_eq!(schema.outputs.len(), 2);
        assert!(schema.input("raw_pitch").is_some());
        assert!(schema.input("gravity_override").is_some());
        assert!(schema.output("pitch_hz").is_some());
        assert!(schema.output("nearest_degree").is_some());
    }

    #[test]
    fn receives_raw_pitch() {
        let mut module = QuantizerModule::new();
        let result = module.receive_signal(module.raw_pitch_port, Signal::Float(0.5));
        assert!(result.is_ok());
        assert_eq!(module.last_raw_pitch, Some(0.5));
    }

    #[test]
    fn receives_gravity_override() {
        let mut module = QuantizerModule::new();
        let result = module.receive_signal(module.gravity_override_port, Signal::Float(0.8));
        assert!(result.is_ok());
        assert_eq!(module.override_gravity, Some(0.8));
    }

    #[test]
    fn rejects_wrong_type() {
        let mut module = QuantizerModule::new();
        let result = module.receive_signal(module.raw_pitch_port, Signal::Bool(true));
        assert!(matches!(
            result,
            Err(SignalError::WrongType {
                expected: SignalType::Float,
                ..
            })
        ));
    }

    #[test]
    fn rejects_unknown_port() {
        let mut module = QuantizerModule::new();
        let result = module.receive_signal(PortId(99999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn emits_pitch_hz_after_tick() {
        let mut module = QuantizerModule::new();
        module
            .receive_signal(module.raw_pitch_port, Signal::Float(0.5))
            .unwrap();
        module.tick(1.0 / 60.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].0, module.pitch_hz_port);
        assert_eq!(buffer[1].0, module.nearest_degree_port);

        if let Signal::Float(hz) = &buffer[0].1 {
            assert!(*hz > 0.0, "pitch_hz should be positive: {hz}");
        } else {
            panic!("expected Float signal for pitch_hz");
        }
    }

    #[test]
    fn set_tuning_changes_output() {
        let mut module = QuantizerModule::new();
        assert_eq!(module.current_tuning(), "bhairav");

        assert!(module.set_tuning("12tet"));
        assert_eq!(module.current_tuning(), "12tet");

        assert!(!module.set_tuning("nonexistent"));
        assert_eq!(module.current_tuning(), "12tet");
    }

    #[test]
    fn available_tunings_lists_all_builtins() {
        let module = QuantizerModule::new();
        let tunings = module.available_tunings();
        assert_eq!(tunings.len(), 9);
        assert!(tunings.contains(&"bhairav"));
        assert!(tunings.contains(&"12tet"));
        assert!(tunings.contains(&"bohlen_pierce"));
    }

    #[test]
    fn gravity_override_affects_quantization() {
        let mut module = QuantizerModule::new();

        // Gravity = 0 (free)
        module
            .receive_signal(module.gravity_override_port, Signal::Float(0.0))
            .unwrap();
        module
            .receive_signal(module.raw_pitch_port, Signal::Float(0.333))
            .unwrap();
        module.tick(1.0 / 60.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        let hz_free = if let Signal::Float(v) = &buffer[0].1 {
            *v
        } else {
            0.0
        };

        // Gravity = 1 (hard snap)
        module
            .receive_signal(module.gravity_override_port, Signal::Float(1.0))
            .unwrap();
        module
            .receive_signal(module.raw_pitch_port, Signal::Float(0.333))
            .unwrap();
        for _ in 0..60 {
            module.tick(1.0 / 60.0);
        }

        buffer.clear();
        module.emit_signals(&mut buffer);
        let hz_snap = if let Signal::Float(v) = &buffer[0].1 {
            *v
        } else {
            0.0
        };

        assert!(
            (hz_free - hz_snap).abs() > 0.1,
            "gravity should affect output: free={hz_free}, snap={hz_snap}"
        );
    }

    #[test]
    fn default_emits_c4_before_input() {
        let mut module = QuantizerModule::new();
        module.tick(1.0 / 60.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        if let Signal::Float(hz) = &buffer[0].1 {
            assert!((*hz - 261.63).abs() < 1.0, "default should emit ~C4: {hz}");
        }
    }

    #[test]
    fn clamps_raw_pitch_input() {
        let mut module = QuantizerModule::new();
        module
            .receive_signal(module.raw_pitch_port, Signal::Float(2.0))
            .unwrap();
        assert_eq!(module.last_raw_pitch, Some(1.0));

        module
            .receive_signal(module.raw_pitch_port, Signal::Float(-0.5))
            .unwrap();
        assert_eq!(module.last_raw_pitch, Some(0.0));
    }

    #[test]
    fn integration_with_reactor() {
        use crate::modules::keyboard_input::KeyboardInputModule;
        use crate::reactor::SeedReactor;

        let mut reactor = SeedReactor::new();
        let _kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let _quant_id = reactor.register(Box::new(QuantizerModule::new()));

        assert!(
            reactor.edge_count() >= 1,
            "should discover edges: {}",
            reactor.edge_count()
        );

        for _ in 0..10 {
            reactor.tick(1.0 / 60.0);
        }
    }
}
