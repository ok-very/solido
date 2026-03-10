use std::collections::HashMap;

use crate::module::{ModuleId, PortId, Signal, SignalType};

/// A pending delivery from infrastructure routing (no weights — full strength).
pub struct InfraDelivery {
    pub target_module: ModuleId,
    pub target_port: PortId,
    #[allow(dead_code)]
    pub target_type: SignalType,
    pub signal: Signal,
}

/// Fixed-routing table for infrastructure modules.
///
/// Infrastructure modules (keyboard, quantizer, voice, audio_analysis, etc.)
/// use deterministic routing with no learning. Edges are discovered once by
/// type/range/rate compatibility, then frozen. No weights, no Hebbian updates,
/// no exploration, no pruning.
pub struct InfrastructureRouter {
    /// Fixed edges: (src_module, src_port) -> [(dst_module, dst_port, expected_type)]
    routes: HashMap<(ModuleId, PortId), Vec<(ModuleId, PortId, SignalType)>>,
}

impl InfrastructureRouter {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Add a fixed route between infrastructure ports.
    pub fn add_route(
        &mut self,
        src_mod: ModuleId,
        src_port: PortId,
        dst_mod: ModuleId,
        dst_port: PortId,
        signal_type: SignalType,
    ) {
        let entry = self.routes.entry((src_mod, src_port)).or_default();
        // Avoid duplicates
        let target = (dst_mod, dst_port, signal_type);
        if !entry.iter().any(|(dm, dp, _)| *dm == target.0 && *dp == target.1) {
            entry.push(target);
        }
    }

    /// Remove all routes involving a module.
    pub fn remove_module(&mut self, id: ModuleId) {
        // Remove routes where this module is the source
        self.routes.retain(|&(src, _), _| src != id);
        // Remove route targets where this module is the destination
        for targets in self.routes.values_mut() {
            targets.retain(|(dst, _, _)| *dst != id);
        }
        // Clean up empty entries
        self.routes.retain(|_, targets| !targets.is_empty());
    }

    /// Route a signal to all connected infrastructure targets. Full strength, no weights.
    pub fn route(
        &self,
        src_mod: ModuleId,
        src_port: PortId,
        signal: &Signal,
    ) -> Vec<InfraDelivery> {
        let Some(targets) = self.routes.get(&(src_mod, src_port)) else {
            return Vec::new();
        };

        targets
            .iter()
            .map(|(dst_mod, dst_port, dst_type)| InfraDelivery {
                target_module: *dst_mod,
                target_port: *dst_port,
                target_type: dst_type.clone(),
                signal: signal.clone(),
            })
            .collect()
    }

    /// Total number of individual route targets.
    pub fn edge_count(&self) -> usize {
        self.routes.values().map(|v| v.len()).sum()
    }

    /// Check if any routes exist from a given source.
    #[allow(dead_code)]
    pub fn has_routes_from(&self, src_mod: ModuleId) -> bool {
        self.routes.keys().any(|&(src, _)| src == src_mod)
    }

    /// Iterate all routes for debugging.
    pub fn iter_routes(&self) -> impl Iterator<Item = (&(ModuleId, PortId), &Vec<(ModuleId, PortId, SignalType)>)> {
        self.routes.iter()
    }

    /// All infrastructure module IDs that appear as source or destination.
    #[allow(dead_code)]
    pub fn involved_modules(&self) -> Vec<ModuleId> {
        let mut ids: Vec<ModuleId> = Vec::new();
        for (&(src, _), targets) in &self.routes {
            if !ids.contains(&src) {
                ids.push(src);
            }
            for &(dst, _, _) in targets {
                if !ids.contains(&dst) {
                    ids.push(dst);
                }
            }
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_route() {
        let mut router = InfrastructureRouter::new();
        router.add_route(0, PortId(1), 1, PortId(2), SignalType::Float);

        let deliveries = router.route(0, PortId(1), &Signal::Float(440.0));
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].target_module, 1);
        assert_eq!(deliveries[0].target_port, PortId(2));
    }

    #[test]
    fn no_duplicates() {
        let mut router = InfrastructureRouter::new();
        router.add_route(0, PortId(1), 1, PortId(2), SignalType::Float);
        router.add_route(0, PortId(1), 1, PortId(2), SignalType::Float);
        assert_eq!(router.edge_count(), 1);
    }

    #[test]
    fn multicast() {
        let mut router = InfrastructureRouter::new();
        router.add_route(0, PortId(1), 1, PortId(2), SignalType::Float);
        router.add_route(0, PortId(1), 2, PortId(3), SignalType::Float);

        let deliveries = router.route(0, PortId(1), &Signal::Float(440.0));
        assert_eq!(deliveries.len(), 2);
    }

    #[test]
    fn remove_module_cleans_both_directions() {
        let mut router = InfrastructureRouter::new();
        router.add_route(0, PortId(1), 1, PortId(2), SignalType::Float);
        router.add_route(1, PortId(3), 2, PortId(4), SignalType::Float);
        router.add_route(2, PortId(5), 0, PortId(6), SignalType::Float);
        assert_eq!(router.edge_count(), 3);

        router.remove_module(1);
        // Route 0→1 gone (dst removed), route 1→2 gone (src removed), route 2→0 remains
        assert_eq!(router.edge_count(), 1);
    }

    #[test]
    fn route_returns_empty_for_unknown_source() {
        let router = InfrastructureRouter::new();
        let deliveries = router.route(99, PortId(0), &Signal::Float(1.0));
        assert!(deliveries.is_empty());
    }
}
