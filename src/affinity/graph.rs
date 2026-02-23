use std::collections::HashMap;

use rand::Rng;
use rand_xoshiro::Xoshiro256StarStar;

use crate::module::port::{ranges_compatible, rates_compatible};
use crate::module::{ModuleId, ModuleSchema, PortId};

use super::edge::{EdgeAffinity, EdgeId};
use super::emotion::ModuleEmotion;
use super::ledger::{LedgerEventType, LedgerReason, LedgerRingBuffer};

/// Arousal threshold above which a module tries to form new edges.
const EXPLORE_AROUSAL_THRESHOLD: f32 = 0.3;
/// Probability of exploration per tick when arousal is above threshold.
const EXPLORE_PROBABILITY: f32 = 0.1;
/// Minimum edge age before pruning is considered.
const PRUNE_MIN_AGE: u64 = 1000;
/// Weight threshold below which old edges get pruned.
const PRUNE_WEIGHT_THRESHOLD: f32 = 0.1;

/// Record of a signal delivery for the graph's tick update.
pub struct DeliveryRecord {
    pub edge_id: EdgeId,
    pub type_valid: bool,
    pub magnitude: f32,
}

/// Per-module tick stats collected during routing.
pub struct ModuleTickStats {
    pub module_id: ModuleId,
    pub signals_received: u32,
    pub errors: u32,
}

/// The affinity graph: edges between module ports that evolve via
/// Hebbian learning, homeostatic emotion, and arousal-driven exploration.
pub struct AffinityGraph {
    pub edges: HashMap<EdgeId, EdgeAffinity>,
    pub emotions: HashMap<ModuleId, ModuleEmotion>,
    pub ledger: LedgerRingBuffer,
    rng: Xoshiro256StarStar,
    tick_count: u64,
    /// True when topology changed (edge added/removed) and routing table
    /// needs rebuilding. Weight-only changes don't set this flag.
    pub topology_dirty: bool,
}

impl AffinityGraph {
    pub fn new(seed: u64) -> Self {
        use rand::SeedableRng;
        Self {
            edges: HashMap::new(),
            emotions: HashMap::new(),
            ledger: LedgerRingBuffer::new(),
            rng: Xoshiro256StarStar::seed_from_u64(seed),
            tick_count: 0,
            topology_dirty: false,
        }
    }

    /// Register a module's emotion state with default target activity.
    pub fn register_module(&mut self, id: ModuleId, target_activity: f32) {
        self.emotions
            .entry(id)
            .or_insert_with(|| ModuleEmotion::new(target_activity));
    }

    /// Remove a module and all its edges.
    pub fn unregister_module(&mut self, id: ModuleId) {
        self.emotions.remove(&id);
        let before = self.edges.len();
        self.edges.retain(|&(src, _, dst, _), _| src != id && dst != id);
        if self.edges.len() != before {
            self.topology_dirty = true;
        }
    }

    /// Create a new edge if it doesn't already exist. Returns true if created.
    pub fn add_edge(&mut self, edge_id: EdgeId) -> bool {
        if self.edges.contains_key(&edge_id) {
            return false;
        }
        let edge = EdgeAffinity::new();
        self.ledger.record(
            self.tick_count,
            edge_id,
            LedgerEventType::Created,
            0.0,
            edge.weight,
            LedgerReason::Exploration,
        );
        self.edges.insert(edge_id, edge);
        self.topology_dirty = true;
        true
    }

    /// Full tick update cycle.
    ///
    /// 1. Update module emotions from tick stats
    /// 2. All edges decay
    /// 3. Record deliveries (update goodput, impact, eligibility)
    /// 4. Reward-modulated Hebbian update
    /// 5. Prune old weak edges
    pub fn tick(
        &mut self,
        deliveries: &[DeliveryRecord],
        module_stats: &[ModuleTickStats],
    ) {
        self.tick_count += 1;

        // 1. Update module emotions
        for stats in module_stats {
            if let Some(emotion) = self.emotions.get_mut(&stats.module_id) {
                emotion.update(stats.signals_received, stats.errors);
            }
        }

        // 2. All edges decay
        for edge in self.edges.values_mut() {
            edge.tick_decay();
        }

        // 3. Record deliveries
        for delivery in deliveries {
            if let Some(edge) = self.edges.get_mut(&delivery.edge_id) {
                edge.on_delivery(delivery.type_valid, delivery.magnitude);
            }
        }

        // 4. Reward-modulated Hebbian update
        // For each edge, use the receiving module's valence as the reward signal
        let edge_ids: Vec<EdgeId> = self.edges.keys().copied().collect();
        for edge_id in &edge_ids {
            let (_, _, dst_module, _) = edge_id;
            let valence = self
                .emotions
                .get(dst_module)
                .map(|e| e.valence)
                .unwrap_or(0.0);

            if let Some(edge) = self.edges.get_mut(edge_id) {
                let weight_before = edge.weight;
                edge.apply_reward(valence);
                let weight_after = edge.weight;

                // Log if weight changed meaningfully
                if (weight_after - weight_before).abs() > 1e-6 {
                    let event_type = if weight_after > weight_before {
                        LedgerEventType::Strengthened
                    } else {
                        LedgerEventType::Weakened
                    };
                    log::debug!(
                        "[affinity] edge {:?}: {:.4} -> {:.4} (hebbian, valence={:.3})",
                        edge_id, weight_before, weight_after, valence
                    );
                    self.ledger.record(
                        self.tick_count,
                        *edge_id,
                        event_type,
                        weight_before,
                        weight_after,
                        LedgerReason::Hebbian,
                    );
                }
            }
        }

        // 5. Prune old weak edges
        let pruned: Vec<EdgeId> = self
            .edges
            .iter()
            .filter(|(_, e)| e.should_prune(PRUNE_MIN_AGE, PRUNE_WEIGHT_THRESHOLD))
            .map(|(id, _)| *id)
            .collect();

        for edge_id in &pruned {
            if let Some(edge) = self.edges.remove(edge_id) {
                log::debug!(
                    "[affinity] pruned edge {:?} (weight={:.4}, age={})",
                    edge_id, edge.weight, edge.age_blocks
                );
                self.ledger.record(
                    self.tick_count,
                    *edge_id,
                    LedgerEventType::Pruned,
                    edge.weight,
                    0.0,
                    LedgerReason::Decay,
                );
            }
        }
        if !pruned.is_empty() {
            self.topology_dirty = true;
        }
    }

    /// Arousal-gated exploration: if module is bored, try to form a new edge
    /// to a compatible port on another module.
    ///
    /// `schemas` provides port type info for compatibility checking.
    /// Returns the new edge ID if one was created.
    pub fn maybe_explore(
        &mut self,
        module_id: ModuleId,
        schemas: &HashMap<ModuleId, ModuleSchema>,
    ) -> Option<EdgeId> {
        let arousal = self.emotions.get(&module_id)?.arousal;
        if arousal < EXPLORE_AROUSAL_THRESHOLD {
            return None;
        }
        if self.rng.gen::<f32>() > EXPLORE_PROBABILITY {
            return None;
        }

        // Find candidate edges: this module's output ports → other modules' compatible input ports
        let my_schema = schemas.get(&module_id)?;
        let mut candidates = Vec::new();

        for output_port in &my_schema.outputs {
            for (&other_id, other_schema) in schemas {
                if other_id == module_id {
                    continue;
                }
                for input_port in &other_schema.inputs {
                    if input_port.signal_type == output_port.signal_type
                        && ranges_compatible(output_port, input_port)
                        && rates_compatible(output_port, input_port)
                    {
                        let edge_id = (module_id, output_port.id, other_id, input_port.id);
                        if !self.edges.contains_key(&edge_id) {
                            candidates.push(edge_id);
                        }
                    }
                }
            }
        }

        // Also explore: other modules' output ports → this module's input ports
        for input_port in &my_schema.inputs {
            for (&other_id, other_schema) in schemas {
                if other_id == module_id {
                    continue;
                }
                for output_port in &other_schema.outputs {
                    if output_port.signal_type == input_port.signal_type
                        && ranges_compatible(output_port, input_port)
                        && rates_compatible(output_port, input_port)
                    {
                        let edge_id = (other_id, output_port.id, module_id, input_port.id);
                        if !self.edges.contains_key(&edge_id) {
                            candidates.push(edge_id);
                        }
                    }
                }
            }
        }

        if candidates.is_empty() {
            return None;
        }

        let idx = self.rng.gen_range(0..candidates.len());
        let edge_id = candidates[idx];

        self.add_edge(edge_id);
        if let Some(event) = self.ledger.events().back() {
            // Re-tag the creation reason as Exploration (add_edge uses Exploration already)
            let _ = event;
        }

        Some(edge_id)
    }

    /// Softmax routing weights for edges from a given output port.
    /// Returns (EdgeId, normalized_weight) pairs that sum to ~1.0.
    pub fn routing_weights_for_port(
        &self,
        source: ModuleId,
        port: PortId,
    ) -> Vec<(EdgeId, f32)> {
        let matching: Vec<(EdgeId, f32)> = self
            .edges
            .iter()
            .filter(|(&(src, src_port, _, _), _)| src == source && src_port == port)
            .map(|(&id, e)| (id, e.weight))
            .collect();

        if matching.is_empty() {
            return Vec::new();
        }

        // Softmax normalization
        let max_w = matching.iter().map(|(_, w)| *w).fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = matching.iter().map(|(_, w)| (w - max_w).exp()).sum();

        if exp_sum < 1e-10 {
            return matching;
        }

        matching
            .into_iter()
            .map(|(id, w)| (id, (w - max_w).exp() / exp_sum))
            .collect()
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::port::{Port, PortRate};
    use crate::module::SignalType;

    fn make_test_schemas() -> HashMap<ModuleId, ModuleSchema> {
        let mut schemas = HashMap::new();

        // Module 0: outputs Float "pitch"
        let schema_a = ModuleSchema::new("producer", crate::module::ModuleCategory::Input)
            .with_output(Port::output("pitch", SignalType::Float, PortRate::Block));
        schemas.insert(0, schema_a);

        // Module 1: inputs Float "raw_pitch", outputs Float "hz"
        let schema_b = ModuleSchema::new("processor", crate::module::ModuleCategory::Processing)
            .with_input(Port::input("raw_pitch", SignalType::Float, PortRate::Block))
            .with_output(Port::output("hz", SignalType::Float, PortRate::Block));
        schemas.insert(1, schema_b);

        // Module 2: inputs Float "freq"
        let schema_c = ModuleSchema::new("consumer", crate::module::ModuleCategory::Output)
            .with_input(Port::input("freq", SignalType::Float, PortRate::Block));
        schemas.insert(2, schema_c);

        schemas
    }

    #[test]
    fn add_edge_and_ledger() {
        let mut graph = AffinityGraph::new(42);
        let edge_id = (0, PortId(0), 1, PortId(1));
        assert!(graph.add_edge(edge_id));
        assert!(!graph.add_edge(edge_id)); // duplicate
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.ledger.len(), 1);
    }

    #[test]
    fn tick_decays_all_edges() {
        let mut graph = AffinityGraph::new(42);
        let edge_id = (0, PortId(0), 1, PortId(1));
        graph.add_edge(edge_id);
        graph.edges.get_mut(&edge_id).unwrap().weight = 0.9;

        graph.tick(&[], &[]);

        let edge = &graph.edges[&edge_id];
        assert!(edge.weight < 0.9, "weight should decay toward 0.5");
        assert_eq!(edge.age_blocks, 1);
    }

    #[test]
    fn good_deliveries_plus_positive_valence_strengthen() {
        let mut graph = AffinityGraph::new(42);
        graph.register_module(1, 5.0);
        let edge_id = (0, PortId(0), 1, PortId(1));
        graph.add_edge(edge_id);

        // Make receiving module happy (moderate activity, no errors)
        for _ in 0..50 {
            let stats = vec![ModuleTickStats {
                module_id: 1,
                signals_received: 5,
                errors: 0,
            }];
            let deliveries = vec![DeliveryRecord {
                edge_id,
                type_valid: true,
                magnitude: 1.0,
            }];
            graph.tick(&deliveries, &stats);
        }

        let weight = graph.edges[&edge_id].weight;
        assert!(weight > 0.5, "weight should strengthen: {}", weight);
    }

    #[test]
    fn bad_deliveries_weaken() {
        let mut graph = AffinityGraph::new(42);
        graph.register_module(1, 5.0);
        let edge_id = (0, PortId(0), 1, PortId(1));
        graph.add_edge(edge_id);

        // Type-invalid deliveries → bad goodput → receiving module unhappy
        for _ in 0..200 {
            let stats = vec![ModuleTickStats {
                module_id: 1,
                signals_received: 5,
                errors: 5,
            }];
            let deliveries = vec![DeliveryRecord {
                edge_id,
                type_valid: false,
                magnitude: 0.0,
            }];
            graph.tick(&deliveries, &stats);
        }

        let weight = graph.edges[&edge_id].weight;
        assert!(weight < 0.5, "weight should weaken: {}", weight);
    }

    #[test]
    fn prune_removes_old_weak_edges() {
        let mut graph = AffinityGraph::new(42);
        let edge_id = (0, PortId(0), 1, PortId(1));
        graph.add_edge(edge_id);

        // Force edge to be old and weak
        let edge = graph.edges.get_mut(&edge_id).unwrap();
        edge.weight = 0.05;
        edge.age_blocks = 1500;

        graph.tick(&[], &[]);

        assert!(
            !graph.edges.contains_key(&edge_id),
            "old weak edge should be pruned"
        );
        // Ledger should record the prune
        let prune_events: Vec<_> = graph
            .ledger
            .events()
            .iter()
            .filter(|e| e.event_type == LedgerEventType::Pruned)
            .collect();
        assert!(!prune_events.is_empty(), "pruning should be logged");
    }

    #[test]
    fn explore_creates_compatible_edge() {
        let schemas = make_test_schemas();
        let mut graph = AffinityGraph::new(42);

        // Register modules
        for &id in schemas.keys() {
            graph.register_module(id, 5.0);
        }

        // Force high arousal on module 0 to trigger exploration
        graph.emotions.get_mut(&0).unwrap().arousal = 0.8;

        // Try many times (exploration is probabilistic)
        let mut explored = false;
        for _ in 0..100 {
            if graph.maybe_explore(0, &schemas).is_some() {
                explored = true;
                break;
            }
        }
        assert!(explored, "exploration should eventually create an edge");
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn softmax_weights_sum_to_one() {
        let mut graph = AffinityGraph::new(42);
        let edge_a = (0, PortId(10), 1, PortId(20));
        let edge_b = (0, PortId(10), 2, PortId(30));
        graph.add_edge(edge_a);
        graph.add_edge(edge_b);

        let weights = graph.routing_weights_for_port(0, PortId(10));
        assert_eq!(weights.len(), 2);

        let total: f32 = weights.iter().map(|(_, w)| w).sum();
        assert!(
            (total - 1.0).abs() < 1e-5,
            "softmax should sum to 1.0: {}",
            total
        );
    }

    #[test]
    fn unregister_removes_module_and_edges() {
        let mut graph = AffinityGraph::new(42);
        graph.register_module(0, 5.0);
        graph.register_module(1, 5.0);
        graph.add_edge((0, PortId(0), 1, PortId(1)));
        graph.add_edge((1, PortId(2), 0, PortId(3)));

        graph.unregister_module(0);
        assert!(!graph.emotions.contains_key(&0));
        assert!(graph.edges.is_empty(), "all edges involving module 0 should be removed");
    }

    #[test]
    fn weights_stay_bounded_over_many_ticks() {
        let mut graph = AffinityGraph::new(42);
        graph.register_module(0, 5.0);
        graph.register_module(1, 5.0);
        let edge_id = (0, PortId(0), 1, PortId(1));
        graph.add_edge(edge_id);

        for _ in 0..500 {
            let stats = vec![
                ModuleTickStats { module_id: 0, signals_received: 3, errors: 0 },
                ModuleTickStats { module_id: 1, signals_received: 3, errors: 0 },
            ];
            let deliveries = vec![DeliveryRecord {
                edge_id,
                type_valid: true,
                magnitude: 0.8,
            }];
            graph.tick(&deliveries, &stats);
        }

        let w = graph.edges[&edge_id].weight;
        assert!(w >= 0.0 && w <= 1.0, "weight must stay in [0,1]: {}", w);
    }
}
