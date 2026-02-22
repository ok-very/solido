use std::collections::HashMap;

use crate::affinity::graph::AffinityGraph;
use crate::module::{ModuleId, ModuleSchema, PortId, Signal, SignalType};

/// A pending delivery: which module/port should receive what signal.
///
/// Routing is **multi-cast**: every connected target receives the full signal
/// (cloned via Arc for heap-heavy variants). Softmax weights in the AffinityGraph
/// determine which edges strengthen/weaken through Hebbian learning — they do NOT
/// scale signal amplitude. This means all compatible modules "hear" every signal;
/// the learning system decides which connections to keep.
pub struct Delivery {
    pub target_module: ModuleId,
    pub target_port: PortId,
    pub target_type: SignalType,
    pub signal: Signal,
}

/// Cached routing table: for each (source_module, output_port), which
/// (target_module, input_port, weight) triples should receive the signal?
///
/// Rebuilt from the AffinityGraph each tick (cheap — just iterates edges).
pub struct RoutingTable {
    routes: HashMap<(ModuleId, PortId), Vec<(ModuleId, PortId, SignalType, f32)>>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Rebuild the routing table from the current affinity graph state.
    /// Uses softmax-normalized weights per output port.
    pub fn rebuild(&mut self, graph: &AffinityGraph, schemas: &HashMap<ModuleId, ModuleSchema>) {
        self.routes.clear();

        // Collect all unique (source_module, output_port) pairs from edges
        let mut output_ports: Vec<(ModuleId, PortId)> = Vec::new();
        for &(src, src_port, _, _) in graph.edges.keys() {
            let key = (src, src_port);
            if !output_ports.contains(&key) {
                output_ports.push(key);
            }
        }

        for (src, src_port) in output_ports {
            let weighted = graph.routing_weights_for_port(src, src_port);
            let targets: Vec<(ModuleId, PortId, SignalType, f32)> = weighted
                .into_iter()
                .filter_map(|((_, _, dst, dst_port), w)| {
                    // Look up the target port's expected signal type
                    let dst_schema = schemas.get(&dst)?;
                    let port = dst_schema.inputs.iter().find(|p| p.id == dst_port)?;
                    Some((dst, dst_port, port.signal_type.clone(), w))
                })
                .collect();

            if !targets.is_empty() {
                self.routes.insert((src, src_port), targets);
            }
        }
    }

    /// Route a signal from (source_module, output_port) to all connected targets.
    /// Multi-cast: every target gets a clone of the signal.
    pub fn route(
        &self,
        source: ModuleId,
        port: PortId,
        signal: &Signal,
    ) -> Vec<Delivery> {
        let Some(targets) = self.routes.get(&(source, port)) else {
            return Vec::new();
        };

        targets
            .iter()
            .map(|(dst, dst_port, dst_type, _weight)| Delivery {
                target_module: *dst,
                target_port: *dst_port,
                target_type: dst_type.clone(),
                signal: signal.clone(),
            })
            .collect()
    }
}
