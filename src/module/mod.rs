pub mod port;
pub mod schema;
pub mod signal;
#[cfg(feature = "ui-egui")]
pub mod ui;

pub use port::{Port, PortDirection, PortId, PortRate, PortRegistry};
pub use schema::{ModuleCategory, ModuleSchema};
pub use signal::{FrameBuffer, Signal, SignalType};
#[cfg(feature = "ui-egui")]
pub use ui::ModuleUi;

pub type ModuleId = u64;

/// Error returned when a signal delivery fails.
#[derive(Debug)]
pub enum SignalError {
    WrongType {
        expected: SignalType,
        got: SignalType,
    },
    UnknownPort(PortId),
    BufferFull,
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalError::WrongType { expected, got } => {
                write!(f, "wrong signal type: expected {expected}, got {got}")
            }
            SignalError::UnknownPort(id) => write!(f, "unknown port: {id}"),
            SignalError::BufferFull => write!(f, "signal buffer full"),
        }
    }
}

impl std::error::Error for SignalError {}

/// The core contract every module implements.
///
/// Every module in the system — keyboard input, pitch quantizer, voice output,
/// camera capture, LLaVA vision, ISF shader, data diagram — implements this
/// trait. The SeedReactor calls these methods each tick to route signals
/// through the affinity graph.
///
/// **No GUI dependency.** This trait is pure data/signal contract.
/// UI is handled separately via `ModuleUi` (feature-gated behind `ui-egui`)
/// so modules can run headless — bottled scenarios, CLI mode, audio servers.
pub trait ModuleCore: Send {
    /// Declare this module's ports, category, and metadata.
    fn schema(&self) -> &ModuleSchema;

    /// Produce signals on output ports. Called once per tick.
    /// Push `(PortId, Signal)` tuples into the reusable buffer.
    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>);

    /// Receive a signal on an input port. PortId is Copy (u32).
    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError>;

    /// Advance internal state by `dt` seconds.
    fn tick(&mut self, dt: f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_error_display() {
        let e = SignalError::WrongType {
            expected: SignalType::Float,
            got: SignalType::Bool,
        };
        assert!(e.to_string().contains("Float"));
        assert!(e.to_string().contains("Bool"));
    }

    #[test]
    fn port_rejects_wrong_type() {
        let port = Port::input("pitch", SignalType::Float, PortRate::Block);
        let good = Signal::Float(440.0);
        let bad = Signal::Bool(true);
        assert!(port.accepts(&good));
        assert!(!port.accepts(&bad));
    }

    #[test]
    fn schema_round_trip() {
        let schema = ModuleSchema::new("test_module", ModuleCategory::Processing)
            .with_description("A test module")
            .with_input(Port::input("in_pitch", SignalType::Float, PortRate::Block))
            .with_output(Port::output("out_hz", SignalType::Float, PortRate::Block))
            .with_side_effect("audio_output");

        assert_eq!(schema.name, "test_module");
        assert_eq!(schema.inputs.len(), 1);
        assert_eq!(schema.outputs.len(), 1);
        assert_eq!(schema.side_effects, vec!["audio_output"]);
        assert!(schema.input("in_pitch").is_some());
        assert!(schema.output("out_hz").is_some());
        assert!(schema.input("nonexistent").is_none());
    }

    #[test]
    fn port_id_is_copy() {
        let port = Port::output("pitch_hz", SignalType::Float, PortRate::Block);
        let id = port.id;
        let copied = id; // Copy, not move
        assert_eq!(id, copied);
    }

    #[test]
    fn port_registry_lookup() {
        let port = Port::output("rms", SignalType::Float, PortRate::Block);
        let mut reg = PortRegistry::new();
        reg.register(port.id, "rms");
        assert_eq!(reg.name(port.id), Some("rms"));
        assert_eq!(reg.name(PortId(99999)), None);
    }

    #[test]
    fn port_registry_from_schema() {
        let schema = ModuleSchema::new("test", ModuleCategory::Processing)
            .with_input(Port::input("in_a", SignalType::Float, PortRate::Block))
            .with_output(Port::output("out_b", SignalType::Float, PortRate::Block));

        let mut reg = PortRegistry::new();
        reg.register_schema(&schema);

        assert_eq!(reg.name(schema.inputs[0].id), Some("in_a"));
        assert_eq!(reg.name(schema.outputs[0].id), Some("out_b"));
    }
}
