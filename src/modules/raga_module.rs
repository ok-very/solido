use std::any::Any;
use std::sync::Arc;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::dsp::command::MAX_MICRO_DEGREES;
use crate::tuning::gamaka::GamakaConfig;
use crate::tuning::raga::{self, RagaRegistry, VADI_BOOST_BASE, SAMVADI_BOOST_BASE, VADI_AROUSAL_SCALE};
use crate::tuning::scale_morph::ScaleMorph;
use crate::tuning::TuningRegistry;

/// Event: set raga by name. Send via `receive_event`.
pub struct SetRaga(pub String);

/// Duration of raga morph transition in ticks (~2 seconds at 60fps).
const MORPH_BLOCKS: u32 = 120;

/// RagaModule — infrastructure module for raga selection, morphing, and gamaka.
///
/// Outputs gravity weights for the QuantizerModule, gamaka configuration,
/// and a hue value for visual coloring.
pub struct RagaModule {
    schema: ModuleSchema,
    registry: RagaRegistry,
    tuning_registry: TuningRegistry,
    current_raga_index: usize,
    current_weights: Vec<f32>,
    current_hue: f32,
    morph: Option<ScaleMorph>,
    gamaka_config: GamakaConfig,
    arousal: f32,
    // Cached microtonal overlay (recomputed on raga change / morph / arousal change)
    cached_micro_cents: [f32; MAX_MICRO_DEGREES],
    cached_micro_weights: [f32; MAX_MICRO_DEGREES],
    cached_micro_count: u8,
    // Port IDs — inputs
    raga_cycle_port: PortId,
    morph_target_port: PortId,
    arousal_port: PortId,
    // Port IDs — outputs
    gravity_weights_port: PortId,
    gamaka_config_port: PortId,
    raga_hue_port: PortId,
}

impl RagaModule {
    pub fn new() -> Self {
        let registry = RagaRegistry::new();
        let mut tuning_registry = TuningRegistry::new();
        tuning_registry.load_builtins();

        let initial = registry.get_by_index(0).unwrap();
        let current_weights = initial.gravity_weights.clone();
        let current_hue = initial.hue;

        // Compute initial micro tuning from first raga
        let (cached_micro_cents, cached_micro_weights, cached_micro_count) =
            if let Some(tuning) = tuning_registry.get(&initial.tuning) {
                raga::raga_to_micro_tuning(initial, tuning, VADI_BOOST_BASE, SAMVADI_BOOST_BASE)
            } else {
                ([0.0; MAX_MICRO_DEGREES], [0.0; MAX_MICRO_DEGREES], 0)
            };

        // Inputs
        let raga_cycle_in = Port::input("raga_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Cycle to next raga (from keyboard R key)");
        let morph_target_in = Port::input("morph_target", SignalType::Text, PortRate::Event)
            .with_description("Target raga name for morph transition (future use)");
        let arousal_in = Port::input("arousal", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Arousal level — modulates gamaka depth");

        // Outputs
        let gravity_weights_out =
            Port::output("gravity_weights", SignalType::Pattern, PortRate::Block)
                .with_description("Per-degree gravity weights for QuantizerModule");
        let gamaka_config_out =
            Port::output("gamaka_config", SignalType::Pattern, PortRate::Block)
                .with_description("Gamaka params [slide_ms, vib_depth_cents, vib_rate_hz]");
        let raga_hue_out = Port::output("raga_hue", SignalType::Float, PortRate::Block)
            .with_range(0.0, 360.0)
            .with_description("HSV hue for visual coloring of current raga");

        let raga_cycle_port = raga_cycle_in.id;
        let morph_target_port = morph_target_in.id;
        let arousal_port = arousal_in.id;
        let gravity_weights_port = gravity_weights_out.id;
        let gamaka_config_port = gamaka_config_out.id;
        let raga_hue_port = raga_hue_out.id;

        let schema = ModuleSchema::new("raga", ModuleCategory::Processing)
            .with_description("Raga selection, morph transitions, and gamaka configuration")
            .with_tier(ModuleTier::Infrastructure)
            .with_input(raga_cycle_in)
            .with_input(morph_target_in)
            .with_input(arousal_in)
            .with_output(gravity_weights_out)
            .with_output(gamaka_config_out)
            .with_output(raga_hue_out);

        Self {
            schema,
            registry,
            tuning_registry,
            current_raga_index: 0,
            current_weights,
            current_hue,
            morph: None,
            gamaka_config: GamakaConfig::default(),
            arousal: 0.0,
            cached_micro_cents,
            cached_micro_weights,
            cached_micro_count,
            raga_cycle_port,
            morph_target_port,
            arousal_port,
            gravity_weights_port,
            gamaka_config_port,
            raga_hue_port,
        }
    }

    fn cycle_raga(&mut self) {
        let old_weights = self.current_weights.clone();
        self.current_raga_index = (self.current_raga_index + 1) % self.registry.len();
        let target_weights = self.registry.get_by_index(self.current_raga_index).unwrap().gravity_weights.clone();
        let hue = self.registry.get_by_index(self.current_raga_index).unwrap().hue;
        let raga_name = self.registry.get_by_index(self.current_raga_index).unwrap().name.clone();

        // Start morph if weight lengths match, otherwise snap
        match ScaleMorph::new(old_weights, target_weights.clone(), MORPH_BLOCKS) {
            Some(morph) => {
                self.morph = Some(morph);
            }
            None => {
                // Snap — different scale sizes
                self.current_weights = target_weights;
            }
        }

        self.current_hue = hue;
        self.recompute_micro_tuning();

        log::info!(
            "[raga] cycled to '{}' (hue={:.0})",
            raga_name,
            hue
        );
    }

    pub fn current_raga_name(&self) -> &str {
        &self.registry.get_by_index(self.current_raga_index).unwrap().name
    }

    #[allow(dead_code)]
    pub fn current_hue(&self) -> f32 {
        self.current_hue
    }

    /// Set raga by name, starting a morph transition. Returns true if found.
    pub fn set_raga_by_name(&mut self, name: &str) -> bool {
        let list = self.registry.list();
        if let Some(idx) = list.iter().position(|n| *n == name) {
            if idx == self.current_raga_index {
                return true; // already selected
            }
            let old_weights = self.current_weights.clone();
            self.current_raga_index = idx;
            let target_weights = self.registry.get_by_index(idx).unwrap().gravity_weights.clone();
            let hue = self.registry.get_by_index(idx).unwrap().hue;
            let raga_name = self.registry.get_by_index(idx).unwrap().name.clone();
            match ScaleMorph::new(old_weights, target_weights.clone(), MORPH_BLOCKS) {
                Some(morph) => self.morph = Some(morph),
                None => self.current_weights = target_weights,
            }
            self.current_hue = hue;
            self.recompute_micro_tuning();
            log::info!("[raga] set to '{}' (hue={:.0})", raga_name, hue);
            true
        } else {
            false
        }
    }

    /// List all available raga names.
    pub fn raga_list(&self) -> Vec<&str> {
        self.registry.list()
    }

    /// Get the current micro tuning overlay (cents, weights, count).
    /// Used by app.rs to send SetMicroTuning to the audio thread.
    pub fn micro_tuning(&self) -> ([f32; MAX_MICRO_DEGREES], [f32; MAX_MICRO_DEGREES], u8) {
        (self.cached_micro_cents, self.cached_micro_weights, self.cached_micro_count)
    }

    /// Recompute cached micro tuning from current raga + arousal-driven vadi/samvadi boosts.
    fn recompute_micro_tuning(&mut self) {
        let raga = self.registry.get_by_index(self.current_raga_index).unwrap();
        if let Some(tuning) = self.tuning_registry.get(&raga.tuning) {
            let vadi_boost = VADI_BOOST_BASE + self.arousal * VADI_AROUSAL_SCALE;
            let samvadi_boost = SAMVADI_BOOST_BASE + self.arousal * (VADI_AROUSAL_SCALE * 0.6);
            let (cents, weights, count) = raga::raga_to_micro_tuning(raga, tuning, vadi_boost, samvadi_boost);
            self.cached_micro_cents = cents;
            self.cached_micro_weights = weights;
            self.cached_micro_count = count;
        }
    }

    /// Get the current raga definition (for direction preference queries).
    pub fn current_raga(&self) -> &crate::tuning::raga::RagaMode {
        self.registry.get_by_index(self.current_raga_index).unwrap()
    }
}

impl ModuleCore for RagaModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        // Emit current gravity weights
        buffer.push((
            self.gravity_weights_port,
            Signal::Pattern(Arc::new(self.current_weights.clone())),
        ));

        // Emit gamaka config as 3-element pattern
        buffer.push((
            self.gamaka_config_port,
            Signal::Pattern(Arc::new(vec![
                self.gamaka_config.slide_time_ms as f32,
                self.gamaka_config.vibrato_depth_cents as f32,
                self.gamaka_config.vibrato_rate_hz as f32,
            ])),
        ));

        // Emit hue
        buffer.push((
            self.raga_hue_port,
            Signal::Float(self.current_hue),
        ));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.raga_cycle_port {
            if let Signal::Trigger = signal {
                self.cycle_raga();
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        if port == self.morph_target_port {
            if let Signal::Text(_name) = signal {
                // Future: morph to named raga
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Text,
                got: signal.signal_type(),
            });
        }

        if port == self.arousal_port {
            if let Signal::Float(v) = signal {
                self.arousal = v.clamp(0.0, 1.0);
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
        // Advance morph if active
        if let Some(ref mut morph) = self.morph {
            let complete = morph.tick();
            self.current_weights = morph.current_weights();
            if complete {
                self.current_weights = morph.target_weights().to_vec();
                self.morph = None;
            }
        }

        // Recompute micro tuning (arousal-driven vadi/samvadi boost changes each tick)
        self.recompute_micro_tuning();

        // Modulate gamaka from arousal:
        // Higher arousal → deeper vibrato, faster vibrato rate
        let base = GamakaConfig::default();
        self.gamaka_config.slide_time_ms = base.slide_time_ms * (1.0 - self.arousal as f64 * 0.3);
        self.gamaka_config.vibrato_depth_cents =
            base.vibrato_depth_cents * (1.0 + self.arousal as f64 * 2.0);
        self.gamaka_config.vibrato_rate_hz =
            base.vibrato_rate_hz * (1.0 + self.arousal as f64 * 0.5);
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
        if let Some(SetRaga(name)) = event.downcast_ref::<SetRaga>() {
            self.set_raga_by_name(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_correct_ports() {
        let module = RagaModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "raga");
        assert_eq!(schema.category, ModuleCategory::Processing);
        assert_eq!(schema.inputs.len(), 3);
        assert_eq!(schema.outputs.len(), 3);
        assert!(schema.input("raga_cycle").is_some());
        assert!(schema.input("morph_target").is_some());
        assert!(schema.input("arousal").is_some());
        assert!(schema.output("gravity_weights").is_some());
        assert!(schema.output("gamaka_config").is_some());
        assert!(schema.output("raga_hue").is_some());
    }

    #[test]
    fn raga_cycling() {
        let mut module = RagaModule::new();
        assert_eq!(module.current_raga_name(), "bhairav");

        module
            .receive_signal(module.raga_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_raga_name(), "bhairavi");

        module
            .receive_signal(module.raga_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_raga_name(), "yaman");
    }

    #[test]
    fn morph_produces_intermediate_weights() {
        let mut module = RagaModule::new();
        let initial_weights = module.current_weights.clone();

        // Cycle to next raga (starts morph)
        module
            .receive_signal(module.raga_cycle_port, Signal::Trigger)
            .unwrap();

        // Tick partway through morph
        for _ in 0..(MORPH_BLOCKS / 2) {
            module.tick(1.0 / 60.0);
        }

        // Weights should be different from initial (interpolated)
        assert_ne!(
            module.current_weights, initial_weights,
            "weights should change during morph"
        );

        // Get target raga's weights
        let target = module.registry.get_by_index(1).unwrap();
        assert_ne!(
            module.current_weights,
            target.gravity_weights,
            "weights should not yet match target mid-morph"
        );
    }

    #[test]
    fn morph_completes() {
        let mut module = RagaModule::new();

        // Cycle to bhairavi
        module
            .receive_signal(module.raga_cycle_port, Signal::Trigger)
            .unwrap();

        // Complete the morph
        for _ in 0..(MORPH_BLOCKS + 10) {
            module.tick(1.0 / 60.0);
        }

        let target = module.registry.get_by_index(1).unwrap();
        assert_eq!(
            module.current_weights,
            target.gravity_weights,
            "weights should match target after morph completes"
        );
    }

    #[test]
    fn arousal_modulates_gamaka() {
        let mut module = RagaModule::new();

        // Default arousal = 0
        module.tick(1.0 / 60.0);
        let base_depth = module.gamaka_config.vibrato_depth_cents;

        // High arousal
        module
            .receive_signal(module.arousal_port, Signal::Float(1.0))
            .unwrap();
        module.tick(1.0 / 60.0);
        let high_depth = module.gamaka_config.vibrato_depth_cents;

        assert!(
            high_depth > base_depth,
            "high arousal should increase vibrato depth: {} > {}",
            high_depth,
            base_depth
        );
    }

    #[test]
    fn emits_all_outputs() {
        let mut module = RagaModule::new();
        module.tick(1.0 / 60.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 3, "should emit 3 signals");
        // gravity_weights (Pattern), gamaka_config (Pattern), raga_hue (Float)
        assert!(matches!(buffer[0].1, Signal::Pattern(_)));
        assert!(matches!(buffer[1].1, Signal::Pattern(_)));
        assert!(matches!(buffer[2].1, Signal::Float(_)));
    }

    #[test]
    fn set_raga_by_name_switches_raga() {
        let mut module = RagaModule::new();
        assert_eq!(module.current_raga_name(), "bhairav");

        assert!(module.set_raga_by_name("yaman"));
        // Complete the morph
        for _ in 0..(MORPH_BLOCKS + 10) {
            module.tick(1.0 / 60.0);
        }
        assert_eq!(module.current_raga_name(), "yaman");
    }

    #[test]
    fn set_raga_by_name_returns_false_for_unknown() {
        let mut module = RagaModule::new();
        assert!(!module.set_raga_by_name("nonexistent"));
        assert_eq!(module.current_raga_name(), "bhairav");
    }

    #[test]
    fn raga_list_is_nonempty() {
        let module = RagaModule::new();
        let list = module.raga_list();
        assert!(list.len() >= 3);
        assert!(list.contains(&"bhairav"));
    }

    #[test]
    fn integration_with_keyboard() {
        use crate::modules::keyboard_input::KeyboardInputModule;
        use crate::reactor::SeedReactor;

        let mut reactor = SeedReactor::new();
        let _kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let _raga_id = reactor.register(Box::new(RagaModule::new()));

        // Should discover: kbd.raga_cycle → raga.raga_cycle
        assert!(
            reactor.edge_count() >= 1,
            "should discover at least 1 edge: {}",
            reactor.edge_count()
        );

        for _ in 0..10 {
            reactor.tick(1.0 / 60.0);
        }
    }

    #[test]
    fn integration_with_quantizer() {
        use crate::modules::quantizer::QuantizerModule;
        use crate::reactor::SeedReactor;

        let mut reactor = SeedReactor::new();
        let _raga_id = reactor.register(Box::new(RagaModule::new()));
        let _quant_id = reactor.register(Box::new(QuantizerModule::new()));

        // Should discover: raga.gravity_weights → quant.gravity_weights,
        //                  raga.gamaka_config → quant.gamaka_config
        assert!(
            reactor.edge_count() >= 2,
            "should discover at least 2 edges: {}",
            reactor.edge_count()
        );

        for _ in 0..10 {
            reactor.tick(1.0 / 60.0);
        }
    }
}
