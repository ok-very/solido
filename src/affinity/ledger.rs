use std::collections::VecDeque;

use super::edge::EdgeId;

const DEFAULT_CAPACITY: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerEventType {
    Created,
    Strengthened,
    Weakened,
    Pruned,
    Explored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerReason {
    Delivery,
    Hebbian,
    Decay,
    Exploration,
    Manual,
}

/// A single recorded weight-change event.
#[derive(Debug, Clone)]
pub struct LedgerEvent {
    pub tick: u64,
    pub edge_id: EdgeId,
    pub event_type: LedgerEventType,
    pub weight_before: f32,
    pub weight_after: f32,
    pub reason: LedgerReason,
}

/// Fixed-capacity ring buffer of ledger events.
///
/// Every edge weight change writes a LedgerEvent. The ledger is the
/// explainability spine — it answers "why is this connection strong?"
pub struct LedgerRingBuffer {
    events: VecDeque<LedgerEvent>,
    capacity: usize,
}

impl LedgerRingBuffer {
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(DEFAULT_CAPACITY),
            capacity: DEFAULT_CAPACITY,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Record a weight change. Evicts oldest event if at capacity.
    pub fn record(
        &mut self,
        tick: u64,
        edge_id: EdgeId,
        event_type: LedgerEventType,
        weight_before: f32,
        weight_after: f32,
        reason: LedgerReason,
    ) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(LedgerEvent {
            tick,
            edge_id,
            event_type,
            weight_before,
            weight_after,
            reason,
        });
    }

    /// All events, oldest first.
    pub fn events(&self) -> &VecDeque<LedgerEvent> {
        &self.events
    }

    /// Events for a specific edge, oldest first.
    pub fn events_for_edge(&self, edge_id: &EdgeId) -> Vec<&LedgerEvent> {
        self.events
            .iter()
            .filter(|e| &e.edge_id == edge_id)
            .collect()
    }

    /// Most recent N events.
    pub fn recent(&self, n: usize) -> Vec<&LedgerEvent> {
        self.events.iter().rev().take(n).collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::PortId;

    fn test_edge_id() -> EdgeId {
        (0, PortId(0), 1, PortId(1))
    }

    #[test]
    fn record_and_retrieve() {
        let mut ledger = LedgerRingBuffer::new();
        ledger.record(
            1,
            test_edge_id(),
            LedgerEventType::Created,
            0.0,
            0.5,
            LedgerReason::Exploration,
        );
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger.events()[0].tick, 1);
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut ledger = LedgerRingBuffer::with_capacity(3);
        for i in 0..5 {
            ledger.record(
                i as u64,
                test_edge_id(),
                LedgerEventType::Strengthened,
                0.5,
                0.6,
                LedgerReason::Hebbian,
            );
        }
        assert_eq!(ledger.len(), 3);
        // Oldest should be tick 2 (0 and 1 evicted)
        assert_eq!(ledger.events()[0].tick, 2);
    }

    #[test]
    fn filter_by_edge() {
        let mut ledger = LedgerRingBuffer::new();
        let edge_a = (0, PortId(0), 1, PortId(1));
        let edge_b = (0, PortId(0), 2, PortId(2));

        ledger.record(1, edge_a, LedgerEventType::Created, 0.0, 0.5, LedgerReason::Exploration);
        ledger.record(2, edge_b, LedgerEventType::Created, 0.0, 0.5, LedgerReason::Exploration);
        ledger.record(3, edge_a, LedgerEventType::Strengthened, 0.5, 0.6, LedgerReason::Hebbian);

        let a_events = ledger.events_for_edge(&edge_a);
        assert_eq!(a_events.len(), 2);
    }

    #[test]
    fn recent_returns_newest_first() {
        let mut ledger = LedgerRingBuffer::new();
        for i in 0..10 {
            ledger.record(
                i,
                test_edge_id(),
                LedgerEventType::Strengthened,
                0.5,
                0.6,
                LedgerReason::Delivery,
            );
        }
        let recent = ledger.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].tick, 9);
        assert_eq!(recent[1].tick, 8);
        assert_eq!(recent[2].tick, 7);
    }
}
