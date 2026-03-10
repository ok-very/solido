#![allow(dead_code)]
/// Rhythm gravity — tala grid, beat quantization, euclidean patterns.
///
/// Beats have gravity just like scale degrees: the beat grid exerts pull
/// on continuous time, snapping looser or tighter depending on `gravity`.

/// Definition of a rhythmic cycle (tala).
#[derive(Clone, Debug)]
pub struct TalaDefinition {
    pub name: String,
    pub beats: u32,
    pub divisions: u32,
    /// Which beat indices are stressed (1-indexed in Indian convention, 0-indexed here).
    pub stressed: Vec<u32>,
    /// Per-beat weight. Length must equal `beats`. Higher = stronger gravity pull.
    pub weights: Vec<f32>,
}

/// A beat event emitted when the grid crosses a beat boundary.
#[derive(Clone, Debug)]
pub struct BeatEvent {
    pub beat_index: u32,
    pub weight: f32,
    pub is_sam: bool,
}

/// A running tala grid that tracks phase and emits beat events.
pub struct TalaGrid {
    pub tala: TalaDefinition,
    pub tempo_bpm: f64,
    pub gravity: f32,
    pub swing: f32,
    /// Current phase in beats (0.0 .. tala.beats as f64).
    phase: f64,
    /// Pending beat events from the last advance().
    pending_events: Vec<BeatEvent>,
}

impl TalaGrid {
    pub fn new(tala: TalaDefinition, tempo_bpm: f64) -> Self {
        Self {
            tala,
            tempo_bpm,
            gravity: 0.5,
            swing: 0.0,
            phase: 0.0,
            pending_events: Vec::new(),
        }
    }

    /// Current phase as a fraction [0, 1) of the full cycle.
    pub fn phase_fraction(&self) -> f64 {
        self.phase / self.tala.beats as f64
    }

    /// Advance time by `dt` seconds. Detects beat crossings (while-loop for multi-beat skips).
    pub fn advance(&mut self, dt: f64) {
        if self.tempo_bpm <= 0.0 || self.tala.beats == 0 {
            return;
        }

        let beats_per_sec = self.tempo_bpm / 60.0;
        let delta_beats = dt * beats_per_sec;
        let cycle_len = self.tala.beats as f64;

        let old_phase = self.phase;
        let new_phase = old_phase + delta_beats;

        // Detect all beat crossings between old and new phase
        let mut next_beat = old_phase.ceil();
        if next_beat == old_phase && old_phase == old_phase.floor() {
            // We're exactly on a beat — skip to next
            next_beat += 1.0;
        }

        while next_beat <= new_phase {
            let beat_index = (next_beat as u32) % self.tala.beats;
            let weight = self.tala.weights.get(beat_index as usize).copied().unwrap_or(1.0);
            let is_sam = beat_index == 0;

            self.pending_events.push(BeatEvent {
                beat_index,
                weight,
                is_sam,
            });
            next_beat += 1.0;
        }

        self.phase = new_phase % cycle_len;
    }

    /// Drain all pending beat events.
    pub fn drain_events(&mut self) -> Vec<BeatEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Quantize a raw time (in beats) toward the nearest beat using gravity.
    ///
    /// Uses corrected midpoint-normalized formula:
    /// `pull = norm_d * |norm_d|^gravity`
    pub fn quantize_trigger(&self, raw_time_beats: f64) -> f64 {
        if self.gravity <= 0.0 || self.tala.beats == 0 {
            return raw_time_beats;
        }

        let nearest_beat = raw_time_beats.round();
        let distance = raw_time_beats - nearest_beat;
        let half_span = 0.5; // beats are evenly spaced by 1.0

        if self.gravity >= 1.0 {
            return nearest_beat;
        }

        let norm_d = (distance / half_span).clamp(-1.0, 1.0);
        let pull_norm = norm_d * norm_d.abs().powf(self.gravity as f64);
        let pull_beats = pull_norm * half_span;

        raw_time_beats - pull_beats
    }
}

/// Bjorklund's algorithm for euclidean rhythms.
///
/// Distributes `n_hits` onsets as evenly as possible across `n_slots`.
/// E(3,8) = [1,0,0,1,0,0,1,0] (tresillo), E(5,8) = [1,0,1,1,0,1,1,0].
pub fn euclidean_rhythm(n_hits: u32, n_slots: u32) -> Vec<bool> {
    if n_slots == 0 {
        return Vec::new();
    }
    if n_hits == 0 {
        return vec![false; n_slots as usize];
    }
    if n_hits >= n_slots {
        return vec![true; n_slots as usize];
    }

    // Bjorklund algorithm
    let mut pattern: Vec<Vec<bool>> = Vec::new();
    let mut remainder: Vec<Vec<bool>> = Vec::new();

    for _ in 0..n_hits {
        pattern.push(vec![true]);
    }
    for _ in 0..(n_slots - n_hits) {
        remainder.push(vec![false]);
    }

    loop {
        let min_len = pattern.len().min(remainder.len());
        if min_len == 0 || remainder.is_empty() {
            break;
        }

        let mut new_pattern = Vec::new();
        let mut new_remainder = Vec::new();

        for i in 0..min_len {
            let mut combined = pattern[i].clone();
            combined.extend_from_slice(&remainder[i]);
            new_pattern.push(combined);
        }

        // Leftovers from whichever was longer
        if pattern.len() > min_len {
            for i in min_len..pattern.len() {
                new_remainder.push(pattern[i].clone());
            }
        } else if remainder.len() > min_len {
            for i in min_len..remainder.len() {
                new_remainder.push(remainder[i].clone());
            }
        }

        pattern = new_pattern;
        remainder = new_remainder;

        if remainder.len() <= 1 {
            break;
        }
    }

    let mut result = Vec::new();
    for p in &pattern {
        result.extend_from_slice(p);
    }
    for r in &remainder {
        result.extend_from_slice(r);
    }
    result
}

/// Registry of built-in tala definitions.
pub struct TalaRegistry {
    talas: Vec<TalaDefinition>,
}

impl TalaRegistry {
    pub fn new() -> Self {
        let talas = vec![
            TalaDefinition {
                name: "teentaal".to_string(),
                beats: 16,
                divisions: 4,
                stressed: vec![0, 4, 8, 12],
                weights: vec![
                    2.0, 1.0, 1.0, 1.0, // dhaa
                    1.5, 1.0, 1.0, 1.0, // dhaa
                    1.5, 1.0, 1.0, 1.0, // dhaa
                    1.5, 1.0, 1.0, 1.0, // dhaa
                ],
            },
            TalaDefinition {
                name: "rupak".to_string(),
                beats: 7,
                divisions: 3,
                stressed: vec![0, 3, 5],
                weights: vec![2.0, 1.0, 1.0, 1.5, 1.0, 1.5, 1.0],
            },
            TalaDefinition {
                name: "jhaptal".to_string(),
                beats: 10,
                divisions: 4,
                stressed: vec![0, 2, 5, 8],
                weights: vec![2.0, 1.0, 1.5, 1.0, 1.0, 1.5, 1.0, 1.0, 1.5, 1.0],
            },
            TalaDefinition {
                name: "dadra".to_string(),
                beats: 6,
                divisions: 2,
                stressed: vec![0, 3],
                weights: vec![2.0, 1.0, 1.0, 1.5, 1.0, 1.0],
            },
            TalaDefinition {
                name: "freeform".to_string(),
                beats: 1,
                divisions: 1,
                stressed: vec![0],
                weights: vec![1.0],
            },
        ];
        Self { talas }
    }

    pub fn get(&self, name: &str) -> Option<&TalaDefinition> {
        self.talas.iter().find(|t| t.name == name)
    }

    pub fn list(&self) -> Vec<&str> {
        self.talas.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn get_by_index(&self, index: usize) -> Option<&TalaDefinition> {
        self.talas.get(index)
    }

    pub fn len(&self) -> usize {
        self.talas.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_zero_passthrough() {
        let tala = TalaDefinition {
            name: "test".to_string(),
            beats: 4,
            divisions: 1,
            stressed: vec![0],
            weights: vec![1.0; 4],
        };
        let grid = TalaGrid::new(tala, 120.0);
        // gravity is 0.5 by default, set it to 0
        let mut grid = grid;
        grid.gravity = 0.0;

        let raw = 1.3;
        let quantized = grid.quantize_trigger(raw);
        assert!(
            (quantized - raw).abs() < 1e-10,
            "gravity=0 should pass through: {} vs {}",
            raw,
            quantized
        );
    }

    #[test]
    fn gravity_one_snap() {
        let tala = TalaDefinition {
            name: "test".to_string(),
            beats: 4,
            divisions: 1,
            stressed: vec![0],
            weights: vec![1.0; 4],
        };
        let mut grid = TalaGrid::new(tala, 120.0);
        grid.gravity = 1.0;

        let quantized = grid.quantize_trigger(1.3);
        assert!(
            (quantized - 1.0).abs() < 1e-10,
            "gravity=1 should snap to nearest beat: {}",
            quantized
        );

        let quantized2 = grid.quantize_trigger(2.7);
        assert!(
            (quantized2 - 3.0).abs() < 1e-10,
            "gravity=1 should snap to nearest beat: {}",
            quantized2
        );
    }

    #[test]
    fn euclidean_3_8() {
        let pattern = euclidean_rhythm(3, 8);
        assert_eq!(pattern.len(), 8);
        let hits: u32 = pattern.iter().map(|&b| if b { 1 } else { 0 }).sum();
        assert_eq!(hits, 3, "E(3,8) should have 3 hits");
        // Tresillo: [1,0,0,1,0,0,1,0]
        assert!(pattern[0]);
        assert!(!pattern[1]);
    }

    #[test]
    fn euclidean_5_8() {
        let pattern = euclidean_rhythm(5, 8);
        assert_eq!(pattern.len(), 8);
        let hits: u32 = pattern.iter().map(|&b| if b { 1 } else { 0 }).sum();
        assert_eq!(hits, 5, "E(5,8) should have 5 hits");
    }

    #[test]
    fn euclidean_edge_cases() {
        assert!(euclidean_rhythm(0, 8).iter().all(|&b| !b));
        assert!(euclidean_rhythm(8, 8).iter().all(|&b| b));
        assert!(euclidean_rhythm(0, 0).is_empty());
    }

    #[test]
    fn multi_beat_skip() {
        let tala = TalaDefinition {
            name: "test".to_string(),
            beats: 4,
            divisions: 1,
            stressed: vec![0],
            weights: vec![1.0; 4],
        };
        let mut grid = TalaGrid::new(tala, 120.0);
        // At 120 BPM, 2 beats/sec. dt=1.5s = 3 beats crossed
        grid.advance(1.5);
        let events = grid.drain_events();
        assert_eq!(
            events.len(),
            3,
            "1.5s at 120bpm should cross 3 beats, got {}",
            events.len()
        );
    }

    #[test]
    fn sam_detected() {
        let tala = TalaDefinition {
            name: "test".to_string(),
            beats: 4,
            divisions: 1,
            stressed: vec![0],
            weights: vec![2.0, 1.0, 1.0, 1.0],
        };
        let mut grid = TalaGrid::new(tala, 120.0);
        // Advance enough to cross all 4 beats + sam again
        grid.advance(2.5); // 5 beats at 120bpm
        let events = grid.drain_events();

        let sam_events: Vec<_> = events.iter().filter(|e| e.is_sam).collect();
        assert!(
            !sam_events.is_empty(),
            "should detect sam (beat 0)"
        );
        assert!(
            sam_events[0].weight > 1.0,
            "sam should have higher weight"
        );
    }

    #[test]
    fn registry_has_5_talas() {
        let reg = TalaRegistry::new();
        assert_eq!(reg.len(), 5);
    }

    #[test]
    fn registry_lookup() {
        let reg = TalaRegistry::new();
        let teen = reg.get("teentaal").unwrap();
        assert_eq!(teen.beats, 16);
        assert_eq!(teen.weights.len(), 16);

        let rupak = reg.get("rupak").unwrap();
        assert_eq!(rupak.beats, 7);
        assert_eq!(rupak.weights.len(), 7);

        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn phase_wraps() {
        let tala = TalaDefinition {
            name: "test".to_string(),
            beats: 4,
            divisions: 1,
            stressed: vec![0],
            weights: vec![1.0; 4],
        };
        let mut grid = TalaGrid::new(tala, 120.0);
        // 4 beats at 120bpm = 2 seconds for one cycle
        grid.advance(3.0); // 6 beats = 1.5 cycles
        assert!(
            grid.phase < 4.0,
            "phase should wrap: {}",
            grid.phase
        );
    }
}
