use std::collections::{HashMap, VecDeque};

use super::edge::{EdgeAffinity, EdgeId};
use crate::module::ModuleId;

/// One sample of an edge's continuous state at a point in time.
#[derive(Clone, Copy, Debug)]
pub struct TrajectorySample {
    pub tick: u64,
    pub weight: f32,
    pub satisfaction: f32,
    pub impact: f32,
}

impl Default for TrajectorySample {
    fn default() -> Self {
        Self {
            tick: 0,
            weight: 0.5,
            satisfaction: 1.0,
            impact: 0.0,
        }
    }
}

const TRAJECTORY_CAPACITY: usize = 256;

/// Per-edge ring buffer of weight/satisfaction/impact samples.
/// Samples every `interval` ticks (default 4). Capacity 256 = ~17s at 60fps.
pub struct EdgeTrajectory {
    samples: Box<[TrajectorySample; TRAJECTORY_CAPACITY]>,
    write_pos: u8,
    len: u16,
    interval: u32,
    ticks_since: u32,
}

impl EdgeTrajectory {
    pub fn new(interval: u32) -> Self {
        Self {
            samples: Box::new([TrajectorySample::default(); TRAJECTORY_CAPACITY]),
            write_pos: 0,
            len: 0,
            interval: interval.max(1),
            ticks_since: 0,
        }
    }

    /// Record a sample if enough ticks have passed. Returns true if sampled.
    pub fn maybe_sample(&mut self, tick: u64, edge: &EdgeAffinity) -> bool {
        self.ticks_since += 1;
        if self.ticks_since < self.interval {
            return false;
        }
        self.ticks_since = 0;
        self.samples[self.write_pos as usize] = TrajectorySample {
            tick,
            weight: edge.weight,
            satisfaction: edge.satisfaction,
            impact: edge.impact,
        };
        self.write_pos = self.write_pos.wrapping_add(1);
        if self.len < TRAJECTORY_CAPACITY as u16 {
            self.len += 1;
        }
        true
    }

    /// Number of valid samples stored.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterate all samples in chronological order.
    pub fn iter(&self) -> TrajectoryIter<'_> {
        let total = self.len as usize;
        let start = if total < TRAJECTORY_CAPACITY {
            0
        } else {
            self.write_pos as usize
        };
        TrajectoryIter {
            samples: &self.samples,
            start,
            total,
            pos: 0,
        }
    }

    /// Get the sample closest to a given tick number.
    pub fn at_tick(&self, tick: u64) -> Option<&TrajectorySample> {
        if self.len == 0 {
            return None;
        }
        let mut best_idx = 0usize;
        let mut best_dist = u64::MAX;
        for (i, s) in self.iter().enumerate() {
            let dist = if s.tick > tick {
                s.tick - tick
            } else {
                tick - s.tick
            };
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        // Convert iteration index to ring buffer index
        let total = self.len as usize;
        let start = if total < TRAJECTORY_CAPACITY {
            0
        } else {
            self.write_pos as usize
        };
        let actual = (start + best_idx) % TRAJECTORY_CAPACITY;
        Some(&self.samples[actual])
    }

    /// Most recent sample.
    pub fn latest(&self) -> Option<&TrajectorySample> {
        if self.len == 0 {
            return None;
        }
        let idx = self.write_pos.wrapping_sub(1) as usize;
        Some(&self.samples[idx])
    }

    /// Most recent N samples in chronological order.
    pub fn recent(&self, n: usize) -> impl Iterator<Item = &TrajectorySample> {
        let total = self.len as usize;
        let skip = total.saturating_sub(n);
        self.iter().skip(skip)
    }
}

/// Iterator over trajectory samples in chronological order.
pub struct TrajectoryIter<'a> {
    samples: &'a [TrajectorySample; TRAJECTORY_CAPACITY],
    start: usize,
    total: usize,
    pos: usize,
}

impl<'a> Iterator for TrajectoryIter<'a> {
    type Item = &'a TrajectorySample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.total {
            return None;
        }
        let idx = (self.start + self.pos) % TRAJECTORY_CAPACITY;
        self.pos += 1;
        Some(&self.samples[idx])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.total - self.pos;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for TrajectoryIter<'a> {}

/// Record of an exploration attempt (successful or not).
#[derive(Clone, Debug)]
pub struct ExplorationEvent {
    pub tick: u64,
    pub module_id: ModuleId,
    pub arousal: f32,
    pub candidate_count: usize,
    pub chosen: Option<EdgeId>,
    pub reason: &'static str,
}

const EXPLORATION_LOG_CAPACITY: usize = 100;

/// Stores trajectories for all edges and an exploration event log.
pub struct TrajectoryStore {
    pub trajectories: HashMap<EdgeId, EdgeTrajectory>,
    pub exploration_log: VecDeque<ExplorationEvent>,
    sample_interval: u32,
}

impl TrajectoryStore {
    pub fn new(sample_interval: u32) -> Self {
        Self {
            trajectories: HashMap::new(),
            exploration_log: VecDeque::new(),
            sample_interval,
        }
    }

    /// Create a trajectory for a new edge.
    pub fn track(&mut self, edge_id: EdgeId) {
        self.trajectories
            .entry(edge_id)
            .or_insert_with(|| EdgeTrajectory::new(self.sample_interval));
    }

    /// Remove a trajectory when an edge is pruned.
    pub fn untrack(&mut self, edge_id: &EdgeId) {
        self.trajectories.remove(edge_id);
    }

    /// Sample all tracked edges.
    pub fn sample_all(&mut self, tick: u64, edges: &HashMap<EdgeId, EdgeAffinity>) {
        for (id, traj) in &mut self.trajectories {
            if let Some(edge) = edges.get(id) {
                traj.maybe_sample(tick, edge);
            }
        }
    }

    /// Record an exploration event.
    pub fn log_exploration(&mut self, event: ExplorationEvent) {
        self.exploration_log.push_back(event);
        if self.exploration_log.len() > EXPLORATION_LOG_CAPACITY {
            self.exploration_log.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::PortId;

    fn make_edge() -> EdgeAffinity {
        EdgeAffinity::new()
    }

    #[test]
    fn trajectory_samples_at_interval() {
        let mut traj = EdgeTrajectory::new(4);
        let mut edge = make_edge();

        // Ticks 1-3: no sample
        for t in 1..=3 {
            assert!(!traj.maybe_sample(t, &edge));
        }
        // Tick 4: sample
        assert!(traj.maybe_sample(4, &edge));
        assert_eq!(traj.len(), 1);

        // Ticks 5-7: no sample
        for t in 5..=7 {
            assert!(!traj.maybe_sample(t, &edge));
        }
        // Tick 8: sample
        edge.weight = 0.7;
        assert!(traj.maybe_sample(8, &edge));
        assert_eq!(traj.len(), 2);

        let samples: Vec<_> = traj.iter().collect();
        assert_eq!(samples.len(), 2);
        assert!((samples[0].weight - 0.5).abs() < f32::EPSILON);
        assert!((samples[1].weight - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn trajectory_ring_wraps() {
        let mut traj = EdgeTrajectory::new(1); // sample every tick
        let edge = make_edge();

        // Fill beyond capacity
        for t in 1..=300 {
            traj.maybe_sample(t, &edge);
        }

        assert_eq!(traj.len(), 256);
        // Oldest should be tick 45 (300 - 256 + 1)
        let first = traj.iter().next().unwrap();
        assert_eq!(first.tick, 45);
        let last = traj.latest().unwrap();
        assert_eq!(last.tick, 300);
    }

    #[test]
    fn trajectory_at_tick() {
        let mut traj = EdgeTrajectory::new(4);
        let mut edge = make_edge();

        // Sample at ticks 4, 8, 12
        for t in 1..=12 {
            if t == 8 {
                edge.weight = 0.8;
            }
            traj.maybe_sample(t, &edge);
        }

        // at_tick(7) should return tick 8 (nearest)
        let sample = traj.at_tick(7).unwrap();
        assert_eq!(sample.tick, 8);

        // at_tick(4) should return tick 4 (exact)
        let sample = traj.at_tick(4).unwrap();
        assert_eq!(sample.tick, 4);
    }

    #[test]
    fn trajectory_lifecycle() {
        let edge_id: EdgeId = (0, PortId(1), 2, PortId(3));
        let mut store = TrajectoryStore::new(4);

        store.track(edge_id);
        assert!(store.trajectories.contains_key(&edge_id));

        store.untrack(&edge_id);
        assert!(!store.trajectories.contains_key(&edge_id));
    }

    #[test]
    fn trajectory_recent() {
        let mut traj = EdgeTrajectory::new(1);
        let edge = make_edge();

        for t in 1..=10 {
            traj.maybe_sample(t, &edge);
        }

        let recent: Vec<_> = traj.recent(3).collect();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].tick, 8);
        assert_eq!(recent[1].tick, 9);
        assert_eq!(recent[2].tick, 10);
    }

    #[test]
    fn exploration_log_capacity() {
        let mut store = TrajectoryStore::new(4);

        for i in 0..150 {
            store.log_exploration(ExplorationEvent {
                tick: i,
                module_id: 0,
                arousal: 0.5,
                candidate_count: 3,
                chosen: None,
                reason: "test",
            });
        }

        assert_eq!(store.exploration_log.len(), 100);
        assert_eq!(store.exploration_log.front().unwrap().tick, 50);
    }

    #[test]
    fn store_sample_all() {
        let edge_id: EdgeId = (0, PortId(1), 2, PortId(3));
        let mut store = TrajectoryStore::new(1);
        store.track(edge_id);

        let mut edges = HashMap::new();
        let mut edge = EdgeAffinity::new();
        edge.weight = 0.9;
        edges.insert(edge_id, edge);

        store.sample_all(1, &edges);

        let traj = store.trajectories.get(&edge_id).unwrap();
        assert_eq!(traj.len(), 1);
        let s = traj.latest().unwrap();
        assert!((s.weight - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_trajectory_queries() {
        let traj = EdgeTrajectory::new(4);
        assert!(traj.is_empty());
        assert!(traj.at_tick(100).is_none());
        assert!(traj.latest().is_none());
        assert_eq!(traj.iter().count(), 0);
        assert_eq!(traj.recent(5).count(), 0);
    }
}
