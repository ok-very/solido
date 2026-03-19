use std::any::Any;
use std::sync::Arc;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::tuning::scale::ScaleRegistry;

/// Event: set scale by name. Send via `receive_event`.
pub struct SetScale(pub String);

/// ScaleModule — infrastructure module for Western scale selection.
///
/// Cycles through 6 Music Mouse-inspired scales, emitting 12-element
/// gravity weights compatible with `quantize_to_scale_fast`.
pub struct ScaleModule {
    schema: ModuleSchema,
    registry: ScaleRegistry,
    current_index: usize,
    // Port IDs — input
    scale_cycle_port: PortId,
    // Port IDs — outputs
    gravity_weights_port: PortId,
    scale_hue_port: PortId,
}

impl ScaleModule {
    pub fn new() -> Self {
        let registry = ScaleRegistry::new();

        // Input
        let scale_cycle_in = Port::input("scale_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Cycle to next Western scale (from keyboard S key)");

        // Outputs
        let gravity_weights_out =
            Port::output("gravity_weights", SignalType::Pattern, PortRate::Block)
                .with_description("12-element chromatic gravity weights for scale quantization");
        let scale_hue_out = Port::output("scale_hue", SignalType::Float, PortRate::Block)
            .with_range(0.0, 360.0)
            .with_description("HSV hue for visual coloring of current scale");

        let scale_cycle_port = scale_cycle_in.id;
        let gravity_weights_port = gravity_weights_out.id;
        let scale_hue_port = scale_hue_out.id;

        let schema = ModuleSchema::new("scale", ModuleCategory::Processing)
            .with_description("Western scale selection — cycles 6 Music Mouse-inspired modes")
            .with_tier(ModuleTier::Infrastructure)
            .with_input(scale_cycle_in)
            .with_output(gravity_weights_out)
            .with_output(scale_hue_out);

        Self {
            schema,
            registry,
            current_index: 0,
            scale_cycle_port,
            gravity_weights_port,
            scale_hue_port,
        }
    }

    fn cycle_scale(&mut self) {
        self.current_index = (self.current_index + 1) % self.registry.len();
        let scale = self.registry.get_by_index(self.current_index).unwrap();
        log::info!(
            "[scale] cycled to '{}' (hue={:.0})",
            scale.name,
            scale.hue
        );
    }

    pub fn current_scale_name(&self) -> &str {
        self.registry.get_by_index(self.current_index).unwrap().name
    }

    pub fn current_index(&self) -> usize {
        self.current_index
    }

    pub fn registry(&self) -> &crate::tuning::scale::ScaleRegistry {
        &self.registry
    }

    #[allow(dead_code)]
    pub fn current_hue(&self) -> f32 {
        self.registry.get_by_index(self.current_index).unwrap().hue
    }

    /// Set scale by name. Returns true if found.
    pub fn set_scale_by_name(&mut self, name: &str) -> bool {
        let list = self.registry.list();
        if let Some(idx) = list.iter().position(|n| *n == name) {
            self.current_index = idx;
            let scale = self.registry.get_by_index(idx).unwrap();
            log::info!("[scale] set to '{}' (hue={:.0})", scale.name, scale.hue);
            true
        } else {
            false
        }
    }

    pub fn scale_list(&self) -> Vec<&str> {
        self.registry.list()
    }

    /// Current 12-element gravity weights (C-rooted).
    /// Used by GravityField to compute per-organism transposed weights.
    pub fn current_weights(&self) -> [f32; 12] {
        self.registry.get_by_index(self.current_index).unwrap().gravity_weights
    }
}

impl ModuleCore for ScaleModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        let scale = self.registry.get_by_index(self.current_index).unwrap();

        // Emit 12-element gravity weights
        buffer.push((
            self.gravity_weights_port,
            Signal::Pattern(Arc::new(scale.gravity_weights.to_vec())),
        ));

        // Emit hue
        buffer.push((self.scale_hue_port, Signal::Float(scale.hue)));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.scale_cycle_port {
            if let Signal::Trigger = signal {
                self.cycle_scale();
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // No per-tick logic needed (no morph transitions)
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
        if let Some(SetScale(name)) = event.downcast_ref::<SetScale>() {
            self.set_scale_by_name(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_correct_ports() {
        let module = ScaleModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "scale");
        assert_eq!(schema.category, ModuleCategory::Processing);
        assert_eq!(schema.inputs.len(), 1);
        assert_eq!(schema.outputs.len(), 2);
        assert!(schema.input("scale_cycle").is_some());
        assert!(schema.output("gravity_weights").is_some());
        assert!(schema.output("scale_hue").is_some());
    }

    #[test]
    fn scale_cycling() {
        let mut module = ScaleModule::new();
        assert_eq!(module.current_scale_name(), "Chromatic");

        module
            .receive_signal(module.scale_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_scale_name(), "Diatonic");

        module
            .receive_signal(module.scale_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_scale_name(), "Pentatonic");
    }

    #[test]
    fn scale_cycling_wraps() {
        let mut module = ScaleModule::new();
        let count = module.scale_list().len();
        for _ in 0..count {
            module
                .receive_signal(module.scale_cycle_port, Signal::Trigger)
                .unwrap();
        }
        assert_eq!(module.current_scale_name(), "Chromatic");
    }

    #[test]
    fn emits_all_outputs() {
        let mut module = ScaleModule::new();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 2, "should emit 2 signals");
        assert!(matches!(buffer[0].1, Signal::Pattern(_)));
        assert!(matches!(buffer[1].1, Signal::Float(_)));
    }

    #[test]
    fn emits_12_element_weights() {
        let mut module = ScaleModule::new();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        if let Signal::Pattern(ref weights) = buffer[0].1 {
            assert_eq!(weights.len(), 12, "should emit 12-element weights");
        } else {
            panic!("expected Pattern signal");
        }
    }

    #[test]
    fn set_scale_by_name_works() {
        let mut module = ScaleModule::new();
        assert!(module.set_scale_by_name("Quartal"));
        assert_eq!(module.current_scale_name(), "Quartal");
    }

    #[test]
    fn set_scale_by_name_returns_false_for_unknown() {
        let mut module = ScaleModule::new();
        assert!(!module.set_scale_by_name("nonexistent"));
        assert_eq!(module.current_scale_name(), "Chromatic");
    }

    #[test]
    fn scale_list_is_nonempty() {
        let module = ScaleModule::new();
        let list = module.scale_list();
        assert_eq!(list.len(), 18);
        assert!(list.contains(&"Chromatic"));
    }

    #[test]
    fn wrong_signal_type_returns_error() {
        let mut module = ScaleModule::new();
        let result = module.receive_signal(module.scale_cycle_port, Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::WrongType { .. })));
    }

    #[test]
    fn unknown_port_returns_error() {
        let mut module = ScaleModule::new();
        let result = module.receive_signal(PortId(999), Signal::Trigger);
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn integration_with_keyboard() {
        use crate::modules::keyboard_input::KeyboardInputModule;
        use crate::reactor::SeedReactor;

        let mut reactor = SeedReactor::new();
        let _kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let _scale_id = reactor.register(Box::new(ScaleModule::new()));

        // Should discover: kbd.scale_cycle → scale.scale_cycle
        assert!(
            reactor.edge_count() >= 1,
            "should discover at least 1 edge: {}",
            reactor.edge_count()
        );

        for _ in 0..10 {
            reactor.tick(1.0 / 60.0);
        }
    }
}
