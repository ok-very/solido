/// A single automation keyframe: (time, value).
#[derive(Debug, Clone, Copy)]
pub struct Keyframe {
    pub time: f64,
    pub value: f32,
}

/// Which channel strip parameter this lane controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationTarget {
    Gain,
    Pan,
}

/// A single automation lane: a sorted list of keyframes for one parameter.
pub struct AutomationLane {
    pub keyframes: Vec<Keyframe>,
    pub target: AutomationTarget,
    pub channel: usize,
}

impl AutomationLane {
    pub fn new(target: AutomationTarget, channel: usize) -> Self {
        Self {
            keyframes: Vec::new(),
            target,
            channel,
        }
    }

    /// Read interpolated value at the given time.
    /// Returns None if no keyframes exist.
    pub fn read_at(&self, time: f64) -> Option<f32> {
        if self.keyframes.is_empty() {
            return None;
        }
        if self.keyframes.len() == 1 {
            return Some(self.keyframes[0].value);
        }

        // Before first keyframe: hold first value
        if time <= self.keyframes[0].time {
            return Some(self.keyframes[0].value);
        }

        // After last keyframe: hold last value
        let last = &self.keyframes[self.keyframes.len() - 1];
        if time >= last.time {
            return Some(last.value);
        }

        // Binary search for the interval containing `time`
        let idx = self
            .keyframes
            .partition_point(|kf| kf.time <= time);

        // idx is the first keyframe with time > target
        let b = &self.keyframes[idx];
        let a = &self.keyframes[idx - 1];

        // Linear interpolation
        let t = ((time - a.time) / (b.time - a.time)) as f32;
        Some(a.value + (b.value - a.value) * t)
    }

    /// Record a keyframe, maintaining sort order.
    /// Deduplicates: if an existing keyframe is within 1/60s, replace it.
    pub fn record(&mut self, time: f64, value: f32) {
        let dedup_threshold = 1.0 / 60.0;

        // Check for existing keyframe close in time
        if let Some(kf) = self
            .keyframes
            .iter_mut()
            .find(|kf| (kf.time - time).abs() < dedup_threshold)
        {
            kf.value = value;
            return;
        }

        // Insert maintaining sort
        let idx = self.keyframes.partition_point(|kf| kf.time < time);
        self.keyframes.insert(idx, Keyframe { time, value });
    }

    /// Clear all keyframes.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.keyframes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lane_returns_none() {
        let lane = AutomationLane::new(AutomationTarget::Gain, 0);
        assert!(lane.read_at(1.0).is_none());
    }

    #[test]
    fn single_keyframe_returns_constant() {
        let mut lane = AutomationLane::new(AutomationTarget::Gain, 0);
        lane.record(1.0, 0.5);
        assert_eq!(lane.read_at(0.0), Some(0.5));
        assert_eq!(lane.read_at(1.0), Some(0.5));
        assert_eq!(lane.read_at(99.0), Some(0.5));
    }

    #[test]
    fn linear_interpolation() {
        let mut lane = AutomationLane::new(AutomationTarget::Gain, 0);
        lane.record(0.0, 0.0);
        lane.record(1.0, 1.0);
        let val = lane.read_at(0.5).unwrap();
        assert!((val - 0.5).abs() < 0.001, "expected 0.5, got {val}");
        let val2 = lane.read_at(0.25).unwrap();
        assert!((val2 - 0.25).abs() < 0.001);
    }

    #[test]
    fn hold_before_and_after() {
        let mut lane = AutomationLane::new(AutomationTarget::Pan, 0);
        lane.record(1.0, -0.5);
        lane.record(2.0, 0.5);
        assert_eq!(lane.read_at(0.0), Some(-0.5));
        assert_eq!(lane.read_at(3.0), Some(0.5));
    }

    #[test]
    fn dedup_within_threshold() {
        let mut lane = AutomationLane::new(AutomationTarget::Gain, 0);
        lane.record(1.0, 0.5);
        lane.record(1.01, 0.8); // within 1/60s ≈ 0.0167
        assert_eq!(lane.keyframes.len(), 1);
        assert!((lane.keyframes[0].value - 0.8).abs() < 0.001);
    }

    #[test]
    fn record_maintains_sort_order() {
        let mut lane = AutomationLane::new(AutomationTarget::Gain, 0);
        lane.record(3.0, 0.3);
        lane.record(1.0, 0.1);
        lane.record(2.0, 0.2);
        let times: Vec<f64> = lane.keyframes.iter().map(|kf| kf.time).collect();
        assert_eq!(times, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn multi_segment_interpolation() {
        let mut lane = AutomationLane::new(AutomationTarget::Gain, 0);
        lane.record(0.0, 0.0);
        lane.record(1.0, 1.0);
        lane.record(2.0, 0.0);
        let val = lane.read_at(1.5).unwrap();
        assert!((val - 0.5).abs() < 0.001);
    }
}
