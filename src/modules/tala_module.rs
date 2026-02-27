use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::tuning::rhythm_gravity::{euclidean_rhythm, TalaGrid, TalaRegistry};

const DEFAULT_TEMPO: f64 = 120.0;
const MIN_TEMPO: f64 = 20.0;
const MAX_TEMPO: f64 = 300.0;

/// TalaModule — infrastructure module for rhythm gravity and beat generation.
///
/// Drives a TalaGrid that emits beat events, gated by an optional euclidean pattern.
/// Cycling through talas via the `tala_cycle` trigger input.
pub struct TalaModule {
    schema: ModuleSchema,
    grid: TalaGrid,
    registry: TalaRegistry,
    current_tala_index: usize,
    euclidean_pattern: Vec<bool>,
    euclidean_hits: u32,
    // Port IDs — inputs
    tempo_delta_port: PortId,
    tala_cycle_port: PortId,
    gravity_override_port: PortId,
    euclidean_hits_port: PortId,
    // Port IDs — outputs
    beat_trigger_port: PortId,
    beat_phase_port: PortId,
    beat_weight_port: PortId,
    is_sam_port: PortId,
}

impl TalaModule {
    pub fn new() -> Self {
        let registry = TalaRegistry::new();
        let tala = registry.get_by_index(0).unwrap().clone();
        let beats = tala.beats;
        let grid = TalaGrid::new(tala, DEFAULT_TEMPO);

        // Inputs
        let tempo_delta_in = Port::input("tempo_delta", SignalType::Float, PortRate::Event)
            .with_range(-5.0, 5.0)
            .with_description("Tempo adjustment from keyboard arrows");
        let tala_cycle_in = Port::input("tala_cycle", SignalType::Trigger, PortRate::Event)
            .with_description("Cycle to next tala (from keyboard T key)");
        let gravity_override_in =
            Port::input("gravity_override", SignalType::Float, PortRate::Block)
                .with_range(0.0, 1.0)
                .with_description("Override rhythm gravity [0,1]");
        let euclidean_hits_in =
            Port::input("euclidean_hits", SignalType::Float, PortRate::Block)
                .with_range(0.0, 16.0)
                .with_description("Number of hits in euclidean pattern [0,16]");

        // Outputs
        let beat_trigger_out = Port::output("beat_trigger", SignalType::Trigger, PortRate::Event)
            .with_description("Fires on each beat (gated by euclidean pattern)");
        let beat_phase_out = Port::output("beat_phase", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Current position in the tala cycle [0,1]");
        let beat_weight_out = Port::output("beat_weight", SignalType::Float, PortRate::Event)
            .with_range(0.0, 2.0)
            .with_description("Weight of the current beat");
        let is_sam_out = Port::output("is_sam", SignalType::Bool, PortRate::Event)
            .with_description("True when beat is sam (first beat of cycle)");

        let tempo_delta_port = tempo_delta_in.id;
        let tala_cycle_port = tala_cycle_in.id;
        let gravity_override_port = gravity_override_in.id;
        let euclidean_hits_port = euclidean_hits_in.id;
        let beat_trigger_port = beat_trigger_out.id;
        let beat_phase_port = beat_phase_out.id;
        let beat_weight_port = beat_weight_out.id;
        let is_sam_port = is_sam_out.id;

        let schema = ModuleSchema::new("tala", ModuleCategory::Processing)
            .with_description("Rhythm gravity — tala grid with beat generation and euclidean gating")
            .with_tier(ModuleTier::Infrastructure)
            .with_input(tempo_delta_in)
            .with_input(tala_cycle_in)
            .with_input(gravity_override_in)
            .with_input(euclidean_hits_in)
            .with_output(beat_trigger_out)
            .with_output(beat_phase_out)
            .with_output(beat_weight_out)
            .with_output(is_sam_out);

        let default_euclidean = euclidean_rhythm(beats, beats); // all hits by default

        Self {
            schema,
            grid,
            registry,
            current_tala_index: 0,
            euclidean_pattern: default_euclidean,
            euclidean_hits: beats,
            tempo_delta_port,
            tala_cycle_port,
            gravity_override_port,
            euclidean_hits_port,
            beat_trigger_port,
            beat_phase_port,
            beat_weight_port,
            is_sam_port,
        }
    }

    fn cycle_tala(&mut self) {
        self.current_tala_index = (self.current_tala_index + 1) % self.registry.len();
        let tala = self.registry.get_by_index(self.current_tala_index).unwrap().clone();
        let tempo = self.grid.tempo_bpm;
        let gravity = self.grid.gravity;
        let beats = tala.beats;
        self.grid = TalaGrid::new(tala, tempo);
        self.grid.gravity = gravity;
        // Reset euclidean pattern for new beat count
        self.euclidean_hits = self.euclidean_hits.min(beats);
        self.euclidean_pattern = euclidean_rhythm(self.euclidean_hits, beats);

        log::info!(
            "[tala] cycled to '{}' ({} beats)",
            self.registry.get_by_index(self.current_tala_index).unwrap().name,
            beats
        );
    }

    fn update_euclidean(&mut self, hits: u32) {
        let beats = self.grid.tala.beats;
        self.euclidean_hits = hits.min(beats);
        self.euclidean_pattern = euclidean_rhythm(self.euclidean_hits, beats);
    }

    pub fn current_tala_name(&self) -> &str {
        &self.grid.tala.name
    }

    pub fn tempo_bpm(&self) -> f64 {
        self.grid.tempo_bpm
    }

    /// Set tala by name. Returns true if found.
    pub fn set_tala_by_name(&mut self, name: &str) -> bool {
        let list = self.registry.list();
        if let Some(idx) = list.iter().position(|n| *n == name) {
            if idx == self.current_tala_index {
                return true;
            }
            self.current_tala_index = idx;
            let tala = self.registry.get_by_index(idx).unwrap().clone();
            let tempo = self.grid.tempo_bpm;
            let gravity = self.grid.gravity;
            let beats = tala.beats;
            self.grid = TalaGrid::new(tala, tempo);
            self.grid.gravity = gravity;
            self.euclidean_hits = self.euclidean_hits.min(beats);
            self.euclidean_pattern = euclidean_rhythm(self.euclidean_hits, beats);
            log::info!("[tala] set to '{}' ({} beats)", name, beats);
            true
        } else {
            false
        }
    }

    /// List all available tala names.
    pub fn tala_list(&self) -> Vec<&str> {
        self.registry.list()
    }

    /// Set tempo in BPM, clamped to [20, 300].
    pub fn set_tempo(&mut self, bpm: f64) {
        self.grid.tempo_bpm = bpm.clamp(MIN_TEMPO, MAX_TEMPO);
    }

    /// Current euclidean hit count.
    pub fn euclidean_hits(&self) -> u32 {
        self.euclidean_hits
    }

    /// Beat count of the current tala.
    pub fn beat_count(&self) -> u32 {
        self.grid.tala.beats
    }
}

impl ModuleCore for TalaModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        // Always emit beat_phase
        buffer.push((
            self.beat_phase_port,
            Signal::Float(self.grid.phase_fraction() as f32),
        ));

        // Emit beat events gated by euclidean pattern
        let events = self.grid.drain_events();
        for event in events {
            let slot = event.beat_index as usize % self.euclidean_pattern.len().max(1);
            let gated = self.euclidean_pattern.get(slot).copied().unwrap_or(true);

            if gated {
                buffer.push((self.beat_trigger_port, Signal::Trigger));
                buffer.push((self.beat_weight_port, Signal::Float(event.weight)));
                buffer.push((self.is_sam_port, Signal::Bool(event.is_sam)));

                log::debug!(
                    "[tala] beat={} weight={:.1} sam={} tempo={:.0}",
                    event.beat_index,
                    event.weight,
                    event.is_sam,
                    self.grid.tempo_bpm
                );
            }
        }
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.tempo_delta_port {
            if let Signal::Float(delta) = signal {
                self.grid.tempo_bpm = (self.grid.tempo_bpm + delta as f64).clamp(MIN_TEMPO, MAX_TEMPO);
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.tala_cycle_port {
            if let Signal::Trigger = signal {
                self.cycle_tala();
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        if port == self.gravity_override_port {
            if let Signal::Float(v) = signal {
                self.grid.gravity = v.clamp(0.0, 1.0);
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.euclidean_hits_port {
            if let Signal::Float(v) = signal {
                self.update_euclidean(v.clamp(0.0, 16.0) as u32);
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
        self.grid.advance(dt as f64);
    }

    fn as_any(&self) -> &dyn Any {
        self
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
        let module = TalaModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "tala");
        assert_eq!(schema.category, ModuleCategory::Processing);
        assert_eq!(schema.inputs.len(), 4);
        assert_eq!(schema.outputs.len(), 4);
        assert!(schema.input("tempo_delta").is_some());
        assert!(schema.input("tala_cycle").is_some());
        assert!(schema.input("gravity_override").is_some());
        assert!(schema.input("euclidean_hits").is_some());
        assert!(schema.output("beat_trigger").is_some());
        assert!(schema.output("beat_phase").is_some());
        assert!(schema.output("beat_weight").is_some());
        assert!(schema.output("is_sam").is_some());
    }

    #[test]
    fn tempo_adjustment() {
        let mut module = TalaModule::new();
        assert!((module.tempo_bpm() - 120.0).abs() < 1e-5);

        module
            .receive_signal(module.tempo_delta_port, Signal::Float(5.0))
            .unwrap();
        assert!((module.tempo_bpm() - 125.0).abs() < 1e-5);

        module
            .receive_signal(module.tempo_delta_port, Signal::Float(-10.0))
            .unwrap();
        assert!((module.tempo_bpm() - 115.0).abs() < 1e-5);
    }

    #[test]
    fn tempo_clamps() {
        let mut module = TalaModule::new();
        module
            .receive_signal(module.tempo_delta_port, Signal::Float(-200.0))
            .unwrap();
        assert!((module.tempo_bpm() - MIN_TEMPO).abs() < 1e-5);

        module
            .receive_signal(module.tempo_delta_port, Signal::Float(500.0))
            .unwrap();
        assert!((module.tempo_bpm() - MAX_TEMPO).abs() < 1e-5);
    }

    #[test]
    fn beat_events_fire() {
        let mut module = TalaModule::new();
        // At 120 BPM, 2 beats/sec. Advance 1 second = 2 beat crossings
        module.tick(1.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // Should have: beat_phase + (beat_trigger + beat_weight + is_sam) * 2
        let triggers: Vec<_> = buffer
            .iter()
            .filter(|(_, s)| matches!(s, Signal::Trigger))
            .collect();
        assert!(
            triggers.len() >= 2,
            "should have at least 2 beat triggers: {}",
            triggers.len()
        );
    }

    #[test]
    fn euclidean_gating() {
        let mut module = TalaModule::new();
        // Set euclidean to 0 hits — should gate all beats
        module.update_euclidean(0);

        module.tick(1.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        // Should only have beat_phase, no triggers
        let triggers: Vec<_> = buffer
            .iter()
            .filter(|(_, s)| matches!(s, Signal::Trigger))
            .collect();
        assert_eq!(triggers.len(), 0, "0 euclidean hits should gate all beats");
    }

    #[test]
    fn tala_cycle() {
        let mut module = TalaModule::new();
        assert_eq!(module.current_tala_name(), "teentaal");

        module
            .receive_signal(module.tala_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_tala_name(), "rupak");

        module
            .receive_signal(module.tala_cycle_port, Signal::Trigger)
            .unwrap();
        assert_eq!(module.current_tala_name(), "jhaptal");
    }

    #[test]
    fn set_tala_by_name_switches_tala() {
        let mut module = TalaModule::new();
        assert_eq!(module.current_tala_name(), "teentaal");

        assert!(module.set_tala_by_name("rupak"));
        assert_eq!(module.current_tala_name(), "rupak");
        assert_eq!(module.beat_count(), 7);
    }

    #[test]
    fn set_tala_by_name_returns_false_for_unknown() {
        let mut module = TalaModule::new();
        assert!(!module.set_tala_by_name("nonexistent"));
        assert_eq!(module.current_tala_name(), "teentaal");
    }

    #[test]
    fn set_tempo_clamps() {
        let mut module = TalaModule::new();
        module.set_tempo(500.0);
        assert!((module.tempo_bpm() - MAX_TEMPO).abs() < 1e-5);

        module.set_tempo(-10.0);
        assert!((module.tempo_bpm() - MIN_TEMPO).abs() < 1e-5);

        module.set_tempo(90.0);
        assert!((module.tempo_bpm() - 90.0).abs() < 1e-5);
    }

    #[test]
    fn tala_list_is_nonempty() {
        let module = TalaModule::new();
        let list = module.tala_list();
        assert!(list.len() >= 3);
        assert!(list.contains(&"teentaal"));
    }

    #[test]
    fn always_emits_beat_phase() {
        let mut module = TalaModule::new();
        module.tick(0.001); // tiny advance

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let phase_signals: Vec<_> = buffer
            .iter()
            .filter(|(port, _)| *port == module.beat_phase_port)
            .collect();
        assert_eq!(
            phase_signals.len(),
            1,
            "should always emit exactly one beat_phase"
        );
    }

    #[test]
    fn integration_with_keyboard() {
        use crate::modules::keyboard_input::KeyboardInputModule;
        use crate::reactor::SeedReactor;

        let mut reactor = SeedReactor::new();
        let _kbd_id = reactor.register(Box::new(KeyboardInputModule::new()));
        let _tala_id = reactor.register(Box::new(TalaModule::new()));

        // Should discover: kbd.tempo_delta → tala.tempo_delta, kbd.tala_cycle → tala.tala_cycle
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
