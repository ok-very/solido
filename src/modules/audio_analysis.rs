use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};

/// Default threshold for the is_active boolean output.
const DEFAULT_THRESHOLD: f32 = 0.01;

/// Audio analysis module — receives RMS/peak from the audio substrate
/// and re-emits them as routable signals plus an activity flag.
///
/// This is a Processing module because it transforms raw audio metrics
/// into the signal graph. The actual audio capture lives in the substrate.
pub struct AudioAnalysisModule {
    schema: ModuleSchema,
    rms: f32,
    peak: f32,
    threshold: f32,
    // Input ports
    rms_input_port: PortId,
    peak_input_port: PortId,
    // Output ports
    rms_output_port: PortId,
    peak_output_port: PortId,
    is_active_port: PortId,
}

impl AudioAnalysisModule {
    /// Feed RMS and peak values from the audio substrate. Call before reactor tick.
    pub fn feed_metrics(&mut self, rms: f32, peak: f32) {
        self.rms = rms;
        self.peak = peak;
    }

    pub fn new() -> Self {
        let rms_in = Port::input("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("RMS level from audio substrate");
        let peak_in = Port::input("peak", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Peak level from audio substrate");

        let rms_out = Port::output("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("RMS level for downstream modules");
        let peak_out = Port::output("peak", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Peak level for downstream modules");
        let is_active = Port::output("is_active", SignalType::Bool, PortRate::Block)
            .with_description("True when RMS exceeds threshold");

        let rms_input_port = rms_in.id;
        let peak_input_port = peak_in.id;
        let rms_output_port = rms_out.id;
        let peak_output_port = peak_out.id;
        let is_active_port = is_active.id;

        let schema = ModuleSchema::new("audio_analysis", ModuleCategory::Processing)
            .with_description("Bridges audio substrate metrics into the signal graph")
            .with_tier(ModuleTier::Infrastructure)
            .with_input(rms_in)
            .with_input(peak_in)
            .with_output(rms_out)
            .with_output(peak_out)
            .with_output(is_active);

        Self {
            schema,
            rms: 0.0,
            peak: 0.0,
            threshold: DEFAULT_THRESHOLD,
            rms_input_port,
            peak_input_port,
            rms_output_port,
            peak_output_port,
            is_active_port,
        }
    }
}

impl ModuleCore for AudioAnalysisModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.rms_output_port, Signal::Float(self.rms)));
        buffer.push((self.peak_output_port, Signal::Float(self.peak)));
        buffer.push((self.is_active_port, Signal::Bool(self.rms > self.threshold)));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.rms_input_port {
            if let Signal::Float(v) = signal {
                self.rms = v;
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }
        if port == self.peak_input_port {
            if let Signal::Float(v) = signal {
                self.peak = v;
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // Pure passthrough — values updated via receive_signal
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receives_and_emits_rms() {
        let mut module = AudioAnalysisModule::new();

        // Feed RMS via receive_signal
        let result = module.receive_signal(module.rms_input_port, Signal::Float(0.5));
        assert!(result.is_ok());

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // Find the rms output
        let rms_signal = buffer.iter().find(|(p, _)| *p == module.rms_output_port);
        assert!(rms_signal.is_some());
        if let Signal::Float(v) = &rms_signal.unwrap().1 {
            assert!((v - 0.5).abs() < 1e-5);
        } else {
            panic!("expected Float signal");
        }
    }

    #[test]
    fn receives_and_emits_peak() {
        let mut module = AudioAnalysisModule::new();

        let result = module.receive_signal(module.peak_input_port, Signal::Float(0.8));
        assert!(result.is_ok());

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let peak_signal = buffer.iter().find(|(p, _)| *p == module.peak_output_port);
        assert!(peak_signal.is_some());
        if let Signal::Float(v) = &peak_signal.unwrap().1 {
            assert!((v - 0.8).abs() < 1e-5);
        } else {
            panic!("expected Float signal");
        }
    }

    #[test]
    fn is_active_threshold() {
        let mut module = AudioAnalysisModule::new();

        // Below threshold → inactive
        module.receive_signal(module.rms_input_port, Signal::Float(0.005)).unwrap();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        let active = buffer.iter().find(|(p, _)| *p == module.is_active_port);
        assert!(matches!(active.unwrap().1, Signal::Bool(false)));

        // Above threshold → active
        module.receive_signal(module.rms_input_port, Signal::Float(0.1)).unwrap();
        buffer.clear();
        module.emit_signals(&mut buffer);
        let active = buffer.iter().find(|(p, _)| *p == module.is_active_port);
        assert!(matches!(active.unwrap().1, Signal::Bool(true)));
    }

    #[test]
    fn rejects_wrong_type() {
        let mut module = AudioAnalysisModule::new();
        let result = module.receive_signal(module.rms_input_port, Signal::Bool(true));
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
        let mut module = AudioAnalysisModule::new();
        let result = module.receive_signal(PortId(99999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn schema_has_correct_ports() {
        let module = AudioAnalysisModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "audio_analysis");
        assert_eq!(schema.category, ModuleCategory::Processing);
        assert_eq!(schema.inputs.len(), 2);
        assert_eq!(schema.outputs.len(), 3);
        assert!(schema.input("rms").is_some());
        assert!(schema.input("peak").is_some());
        assert!(schema.output("rms").is_some());
        assert!(schema.output("peak").is_some());
        assert!(schema.output("is_active").is_some());
    }

    #[test]
    fn default_values_are_zero() {
        let mut module = AudioAnalysisModule::new();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let rms = buffer.iter().find(|(p, _)| *p == module.rms_output_port);
        if let Signal::Float(v) = &rms.unwrap().1 {
            assert_eq!(*v, 0.0);
        }
        let active = buffer.iter().find(|(p, _)| *p == module.is_active_port);
        assert!(matches!(active.unwrap().1, Signal::Bool(false)));
    }
}
