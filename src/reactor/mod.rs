pub mod routing;

use std::collections::HashMap;

use crate::affinity::graph::{AffinityGraph, DeliveryRecord, ModuleTickStats};
use crate::module::port::{ranges_compatible, rates_compatible};
use crate::module::{ModuleCore, ModuleId, ModuleSchema, PortId, Signal};

use routing::RoutingTable;

/// The central hub: every module registers with the SeedReactor.
///
/// Each tick, the reactor:
/// 1. Calls `emit_signals()` on every module
/// 2. Routes signals through the AffinityGraph via the RoutingTable
/// 3. Delivers signals to receiving modules via `receive_signal()`
/// 4. Updates emotions, runs Hebbian learning, explores, prunes
/// 5. Records everything in the ledger
pub struct SeedReactor {
    modules: HashMap<ModuleId, Box<dyn ModuleCore>>,
    pub graph: AffinityGraph,
    schemas: HashMap<ModuleId, ModuleSchema>,
    routing: RoutingTable,
    next_id: ModuleId,
    tick_count: u64,
    /// Reusable buffer for emit_signals to avoid per-tick allocation.
    emit_buffer: Vec<(PortId, Signal)>,
}

impl SeedReactor {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            graph: AffinityGraph::new(42),
            schemas: HashMap::new(),
            routing: RoutingTable::new(),
            next_id: 0,
            tick_count: 0,
            emit_buffer: Vec::new(),
        }
    }

    /// Register a module. Returns its assigned ModuleId.
    /// Automatically registers emotion state and discovers compatible edges.
    pub fn register(&mut self, module: Box<dyn ModuleCore>) -> ModuleId {
        let id = self.next_id;
        self.next_id += 1;

        let schema = module.schema().clone();
        // Default target activity = 1.0 — most modules receive a few signals
        // per tick. Modules can adjust their homeostatic setpoint over time.
        self.graph.register_module(id, 1.0);
        self.schemas.insert(id, schema);
        self.modules.insert(id, module);

        // Auto-discover compatible edges with existing modules
        self.discover_edges(id);

        // Rebuild routing table (topology just changed)
        self.routing.rebuild(&self.graph, &self.schemas);
        self.graph.topology_dirty = false;

        id
    }

    /// Remove a module and all its edges.
    pub fn unregister(&mut self, id: ModuleId) {
        self.modules.remove(&id);
        self.schemas.remove(&id);
        self.graph.unregister_module(id);
        self.routing.rebuild(&self.graph, &self.schemas);
        self.graph.topology_dirty = false;
    }

    /// Discover and create edges between a new module's ports and all
    /// existing modules' compatible ports.
    fn discover_edges(&mut self, new_id: ModuleId) {
        let Some(new_schema) = self.schemas.get(&new_id) else {
            return;
        };
        let new_outputs = new_schema.outputs.clone();
        let new_inputs = new_schema.inputs.clone();

        for (&other_id, other_schema) in &self.schemas {
            if other_id == new_id {
                continue;
            }

            // New module's outputs → other module's inputs
            for out_port in &new_outputs {
                for in_port in &other_schema.inputs {
                    if out_port.signal_type == in_port.signal_type
                        && ranges_compatible(out_port, in_port)
                        && rates_compatible(out_port, in_port)
                    {
                        self.graph
                            .add_edge((new_id, out_port.id, other_id, in_port.id));
                    }
                }
            }

            // Other module's outputs → new module's inputs
            for out_port in &other_schema.outputs {
                for in_port in &new_inputs {
                    if out_port.signal_type == in_port.signal_type
                        && ranges_compatible(out_port, in_port)
                        && rates_compatible(out_port, in_port)
                    {
                        self.graph
                            .add_edge((other_id, out_port.id, new_id, in_port.id));
                    }
                }
            }
        }
    }

    /// Run one tick of the entire system.
    pub fn tick(&mut self, dt: f32) {
        self.tick_count += 1;

        // 1. Tick all modules (advance internal state)
        for module in self.modules.values_mut() {
            module.tick(dt);
        }

        // 2. Collect emitted signals from all modules
        let module_ids: Vec<ModuleId> = self.modules.keys().copied().collect();
        let mut all_emissions: Vec<(ModuleId, PortId, Signal)> = Vec::new();

        for &id in &module_ids {
            if let Some(module) = self.modules.get_mut(&id) {
                self.emit_buffer.clear();
                module.emit_signals(&mut self.emit_buffer);
                for (port, signal) in self.emit_buffer.drain(..) {
                    all_emissions.push((id, port, signal));
                }
            }
        }

        // Log emissions
        for (id, port, signal) in &all_emissions {
            log::debug!("[emit] module:{} port:{} signal:{:?}", id, port, signal.signal_type());
        }

        // 3. Route signals through the routing table and deliver
        let mut deliveries = Vec::new();
        let mut module_stats: HashMap<ModuleId, (u32, u32)> = HashMap::new();

        for (src_id, src_port, signal) in &all_emissions {
            let routes = self.routing.route(*src_id, *src_port, signal);

            for delivery in routes {
                let type_valid = signal.matches_type(&delivery.target_type);
                let magnitude = signal.magnitude();

                // Find the edge_id for this delivery
                let edge_id = (*src_id, *src_port, delivery.target_module, delivery.target_port);

                log::debug!(
                    "[deliver] {}:{} -> {}:{} (weight={:.3})",
                    src_id, src_port, delivery.target_module, delivery.target_port, delivery.weight
                );

                deliveries.push(DeliveryRecord {
                    edge_id,
                    type_valid,
                    magnitude,
                });

                // Deliver to target module
                let stats = module_stats
                    .entry(delivery.target_module)
                    .or_insert((0, 0));
                stats.0 += 1; // signals_received

                if let Some(target) = self.modules.get_mut(&delivery.target_module) {
                    if target
                        .receive_signal(delivery.target_port, delivery.signal)
                        .is_err()
                    {
                        stats.1 += 1; // errors
                    }
                }
            }
        }

        // 4. Build module tick stats (include modules that received nothing)
        let tick_stats: Vec<ModuleTickStats> = module_ids
            .iter()
            .map(|&id| {
                let (received, errors) = module_stats.get(&id).copied().unwrap_or((0, 0));
                ModuleTickStats {
                    module_id: id,
                    signals_received: received,
                    errors,
                }
            })
            .collect();

        // 5. Update affinity graph (emotion, decay, Hebbian, prune)
        self.graph.tick(&deliveries, &tick_stats);

        // 6. Exploration: let bored modules try new edges
        for &id in &module_ids {
            self.graph.maybe_explore(id, &self.schemas);
        }

        // 7. Rebuild routing table only when topology changed (edges added/pruned).
        // Weight-only changes don't require a rebuild — softmax is recomputed
        // inside rebuild, but topology is the expensive part.
        if self.graph.topology_dirty {
            self.routing.rebuild(&self.graph, &self.schemas);
            self.graph.topology_dirty = false;
        }
    }

    /// Get a mutable reference to a module by ID.
    /// Used by the app layer to downcast and feed input events.
    pub fn module_mut(&mut self, id: ModuleId) -> Option<&mut Box<dyn ModuleCore>> {
        self.modules.get_mut(&id)
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edges.len()
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn schemas(&self) -> &HashMap<ModuleId, ModuleSchema> {
        &self.schemas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::port::{Port, PortRate};
    use crate::module::{ModuleSchema, ModuleCategory, SignalType, SignalError};
    use crate::affinity::ledger::LedgerEventType;

    /// Stub producer: emits Float(0.7) on its output port every tick.
    struct StubProducer {
        schema: ModuleSchema,
        out_port: PortId,
    }

    impl StubProducer {
        fn new() -> Self {
            let out = Port::output("pitch", SignalType::Float, PortRate::Block);
            let out_port = out.id;
            let schema = ModuleSchema::new("producer", ModuleCategory::Input)
                .with_output(out);
            Self { schema, out_port }
        }
    }

    impl ModuleCore for StubProducer {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
            buffer.push((self.out_port, Signal::Float(0.7)));
        }
        fn receive_signal(&mut self, _port: PortId, _signal: Signal) -> Result<(), SignalError> {
            Ok(())
        }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// Stub processor: receives Float on "raw_pitch", emits Float on "hz".
    struct StubProcessor {
        schema: ModuleSchema,
        out_port: PortId,
        in_port: PortId,
        last_value: f32,
    }

    impl StubProcessor {
        fn new() -> Self {
            let inp = Port::input("raw_pitch", SignalType::Float, PortRate::Block);
            let out = Port::output("hz", SignalType::Float, PortRate::Block);
            let in_port = inp.id;
            let out_port = out.id;
            let schema = ModuleSchema::new("processor", ModuleCategory::Processing)
                .with_input(inp)
                .with_output(out);
            Self { schema, out_port, in_port, last_value: 0.0 }
        }
    }

    impl ModuleCore for StubProcessor {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
            if self.last_value > 0.0 {
                buffer.push((self.out_port, Signal::Float(self.last_value * 440.0)));
            }
        }
        fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
            if port == self.in_port {
                if let Signal::Float(v) = signal {
                    self.last_value = v;
                    return Ok(());
                }
                return Err(SignalError::WrongType {
                    expected: SignalType::Float,
                    got: signal.signal_type(),
                });
            }
            Err(SignalError::UnknownPort(port))
        }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// Stub consumer: receives Float on "freq", accumulates count.
    struct StubConsumer {
        schema: ModuleSchema,
        in_port: PortId,
        received_count: u32,
    }

    impl StubConsumer {
        fn new() -> Self {
            let inp = Port::input("freq", SignalType::Float, PortRate::Block);
            let in_port = inp.id;
            let schema = ModuleSchema::new("consumer", ModuleCategory::Output)
                .with_input(inp);
            Self { schema, in_port, received_count: 0 }
        }
    }

    impl ModuleCore for StubConsumer {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, _buffer: &mut Vec<(PortId, Signal)>) {}
        fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
            if port == self.in_port {
                if let Signal::Float(_) = signal {
                    self.received_count += 1;
                    return Ok(());
                }
                return Err(SignalError::WrongType {
                    expected: SignalType::Float,
                    got: signal.signal_type(),
                });
            }
            Err(SignalError::UnknownPort(port))
        }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    #[test]
    fn register_discovers_compatible_edges() {
        let mut reactor = SeedReactor::new();
        let _a = reactor.register(Box::new(StubProducer::new()));
        let _b = reactor.register(Box::new(StubProcessor::new()));
        let _c = reactor.register(Box::new(StubConsumer::new()));

        assert_eq!(reactor.module_count(), 3);
        // producer.pitch→processor.raw_pitch, processor.hz→consumer.freq
        assert!(reactor.edge_count() >= 2, "should auto-discover edges: {}", reactor.edge_count());
    }

    #[test]
    fn signals_route_end_to_end() {
        let mut reactor = SeedReactor::new();
        reactor.register(Box::new(StubProducer::new()));
        reactor.register(Box::new(StubProcessor::new()));
        reactor.register(Box::new(StubConsumer::new()));

        // Run enough ticks for signals to propagate through the chain
        for _ in 0..10 {
            reactor.tick(1.0 / 60.0);
        }

        // The ledger should have recorded edge activity
        assert!(
            !reactor.graph.ledger.is_empty(),
            "ledger should record events"
        );
    }

    #[test]
    fn convergence_over_1000_ticks() {
        let mut reactor = SeedReactor::new();
        reactor.register(Box::new(StubProducer::new()));
        reactor.register(Box::new(StubProcessor::new()));
        reactor.register(Box::new(StubConsumer::new()));

        for _ in 0..1000 {
            reactor.tick(1.0 / 60.0);
        }

        // All weights should remain bounded
        for (_, edge) in &reactor.graph.edges {
            assert!(
                edge.weight >= 0.0 && edge.weight <= 1.0,
                "weight out of bounds: {}",
                edge.weight
            );
        }

        // Active edges (producer→processor, processor→consumer) should be stronger
        // than the initial 0.5 since they carry valid signals
        let strong_edges: Vec<_> = reactor
            .graph
            .edges
            .values()
            .filter(|e| e.weight > 0.5)
            .collect();
        assert!(
            !strong_edges.is_empty(),
            "some edges should strengthen from valid signal flow"
        );

        // Ledger should have many events from 1000 ticks of activity
        assert!(
            reactor.graph.ledger.len() > 50,
            "ledger should be well-populated: {}",
            reactor.graph.ledger.len()
        );
    }

    #[test]
    fn unregister_cleans_up() {
        let mut reactor = SeedReactor::new();
        let a = reactor.register(Box::new(StubProducer::new()));
        let _b = reactor.register(Box::new(StubProcessor::new()));
        let initial_edges = reactor.edge_count();

        reactor.unregister(a);
        assert_eq!(reactor.module_count(), 1);
        assert!(
            reactor.edge_count() < initial_edges,
            "edges should be removed"
        );
    }

    #[test]
    fn emotions_respond_to_activity() {
        let mut reactor = SeedReactor::new();
        let _a = reactor.register(Box::new(StubProducer::new()));
        let b = reactor.register(Box::new(StubProcessor::new()));

        for _ in 0..100 {
            reactor.tick(1.0 / 60.0);
        }

        // Processor receives signals, so its emotion should show activity
        let emotion = &reactor.graph.emotions[&b];
        assert!(
            emotion.activity > 0.0,
            "processor should show activity: {}",
            emotion.activity
        );
    }

    #[test]
    fn ledger_traces_edge_history() {
        let mut reactor = SeedReactor::new();
        reactor.register(Box::new(StubProducer::new()));
        reactor.register(Box::new(StubProcessor::new()));

        for _ in 0..50 {
            reactor.tick(1.0 / 60.0);
        }

        // Pick any edge and verify we can trace its history
        if let Some((&edge_id, _)) = reactor.graph.edges.iter().next() {
            let history = reactor.graph.ledger.events_for_edge(&edge_id);
            assert!(
                !history.is_empty(),
                "should have ledger history for active edge"
            );
            // History should include creation + Hebbian updates
            let created = history.iter().any(|e| e.event_type == LedgerEventType::Created);
            assert!(created, "history should include edge creation");
        }
    }

    #[test]
    fn softmax_normalization_per_port() {
        let mut reactor = SeedReactor::new();
        reactor.register(Box::new(StubProducer::new()));
        reactor.register(Box::new(StubProcessor::new()));
        reactor.register(Box::new(StubConsumer::new()));

        // For each output port that has edges, softmax weights should sum to ~1.0
        for (_, schema) in reactor.schemas() {
            for port in &schema.outputs {
                let weights = reactor.graph.routing_weights_for_port(0, port.id);
                if !weights.is_empty() {
                    let total: f32 = weights.iter().map(|(_, w)| w).sum();
                    assert!(
                        (total - 1.0).abs() < 0.01,
                        "softmax should sum to ~1.0: {}",
                        total
                    );
                }
            }
        }
    }

    // --- S02b: Range-aware edge discovery ---

    /// Normalized output [0,1] — should NOT connect to Hz input [20,20000].
    struct StubNormalizedProducer {
        schema: ModuleSchema,
        out_port: PortId,
    }

    impl StubNormalizedProducer {
        fn new() -> Self {
            let out = Port::output("raw_pitch", SignalType::Float, PortRate::Event)
                .with_range(0.0, 1.0);
            let out_port = out.id;
            let schema = ModuleSchema::new("normalized_producer", ModuleCategory::Input)
                .with_output(out);
            Self { schema, out_port }
        }
    }

    impl ModuleCore for StubNormalizedProducer {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
            buffer.push((self.out_port, Signal::Float(0.5)));
        }
        fn receive_signal(&mut self, _port: PortId, _signal: Signal) -> Result<(), SignalError> { Ok(()) }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// Hz output [20,20000] — SHOULD connect to Hz input [20,20000].
    struct StubHzProducer {
        schema: ModuleSchema,
        out_port: PortId,
    }

    impl StubHzProducer {
        fn new() -> Self {
            let out = Port::output("pitch_hz", SignalType::Float, PortRate::Block)
                .with_range(20.0, 20000.0);
            let out_port = out.id;
            let schema = ModuleSchema::new("hz_producer", ModuleCategory::Processing)
                .with_output(out);
            Self { schema, out_port }
        }
    }

    impl ModuleCore for StubHzProducer {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
            buffer.push((self.out_port, Signal::Float(440.0)));
        }
        fn receive_signal(&mut self, _port: PortId, _signal: Signal) -> Result<(), SignalError> { Ok(()) }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// Hz consumer [20,20000] — only Hz-range producers should connect.
    struct StubHzConsumer {
        schema: ModuleSchema,
        in_port: PortId,
        last_value: f32,
    }

    impl StubHzConsumer {
        fn new() -> Self {
            let inp = Port::input("pitch_hz", SignalType::Float, PortRate::Block)
                .with_range(20.0, 20000.0);
            let in_port = inp.id;
            let schema = ModuleSchema::new("hz_consumer", ModuleCategory::Output)
                .with_input(inp);
            Self { schema, in_port, last_value: 0.0 }
        }
    }

    impl ModuleCore for StubHzConsumer {
        fn schema(&self) -> &ModuleSchema { &self.schema }
        fn emit_signals(&mut self, _buffer: &mut Vec<(PortId, Signal)>) {}
        fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
            if port == self.in_port {
                if let Signal::Float(v) = signal { self.last_value = v; return Ok(()); }
                return Err(SignalError::WrongType { expected: SignalType::Float, got: signal.signal_type() });
            }
            Err(SignalError::UnknownPort(port))
        }
        fn tick(&mut self, _dt: f32) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    #[test]
    fn incompatible_ranges_no_edge() {
        let mut reactor = SeedReactor::new();
        let _norm = reactor.register(Box::new(StubNormalizedProducer::new()));
        let _hz = reactor.register(Box::new(StubHzConsumer::new()));

        // [0,1] output should NOT connect to [20,20000] input
        assert_eq!(
            reactor.edge_count(), 0,
            "incompatible ranges should create no edges, got {}",
            reactor.edge_count()
        );
    }

    #[test]
    fn compatible_ranges_get_edge() {
        let mut reactor = SeedReactor::new();
        let _hz_prod = reactor.register(Box::new(StubHzProducer::new()));
        let _hz_cons = reactor.register(Box::new(StubHzConsumer::new()));

        // [20,20000] output SHOULD connect to [20,20000] input
        assert_eq!(
            reactor.edge_count(), 1,
            "compatible ranges should create an edge, got {}",
            reactor.edge_count()
        );
    }

    #[test]
    fn mixed_ranges_only_compatible_connect() {
        let mut reactor = SeedReactor::new();
        let _norm = reactor.register(Box::new(StubNormalizedProducer::new()));
        let _hz_prod = reactor.register(Box::new(StubHzProducer::new()));
        let _hz_cons = reactor.register(Box::new(StubHzConsumer::new()));

        // Only hz_producer → hz_consumer should connect (not norm → hz_consumer)
        assert_eq!(
            reactor.edge_count(), 1,
            "only compatible range edge should exist, got {}",
            reactor.edge_count()
        );
    }

    #[test]
    fn hz_signal_reaches_consumer_not_normalized() {
        let mut reactor = SeedReactor::new();
        let _norm = reactor.register(Box::new(StubNormalizedProducer::new()));
        let _hz_prod = reactor.register(Box::new(StubHzProducer::new()));
        let hz_cons_id = reactor.register(Box::new(StubHzConsumer::new()));

        for _ in 0..5 {
            reactor.tick(1.0 / 60.0);
        }

        // Consumer should have received 440.0, not 0.5
        let consumer = reactor.module_mut(hz_cons_id).unwrap();
        let consumer = consumer.as_any_mut().downcast_ref::<StubHzConsumer>().unwrap();
        assert!(
            (consumer.last_value - 440.0).abs() < 1e-3,
            "consumer should receive Hz value 440.0, got {}",
            consumer.last_value
        );
    }
}
