#![allow(dead_code)]
/// Scale morphing — smooth transitions between raga gravity weight vectors.
///
/// When switching ragas, `ScaleMorph` interpolates between the old and new
/// `gravity_weights` over a configurable number of tick blocks.

/// Morphs between two weight vectors over time.
pub struct ScaleMorph {
    from_weights: Vec<f32>,
    to_weights: Vec<f32>,
    /// Progress [0.0, 1.0].
    t: f64,
    /// Total duration in blocks (ticks).
    morph_blocks: u32,
    /// Elapsed blocks since morph started.
    elapsed_blocks: u32,
}

impl ScaleMorph {
    /// Create a new morph. Returns `None` if weight vectors have different lengths.
    pub fn new(from: Vec<f32>, to: Vec<f32>, blocks: u32) -> Option<Self> {
        if from.len() != to.len() {
            return None;
        }
        Some(Self {
            from_weights: from,
            to_weights: to,
            t: 0.0,
            morph_blocks: blocks.max(1),
            elapsed_blocks: 0,
        })
    }

    /// Advance one block. Returns `true` when the morph is complete.
    pub fn tick(&mut self) -> bool {
        self.elapsed_blocks += 1;
        self.t = (self.elapsed_blocks as f64 / self.morph_blocks as f64).min(1.0);
        self.t >= 1.0
    }

    /// Current interpolated weights (lerp between from and to).
    pub fn current_weights(&self) -> Vec<f32> {
        let t = self.t as f32;
        self.from_weights
            .iter()
            .zip(self.to_weights.iter())
            .map(|(&a, &b)| a + (b - a) * t)
            .collect()
    }

    /// Final weights (the target).
    pub fn target_weights(&self) -> &[f32] {
        &self.to_weights
    }

    /// Progress as fraction [0, 1].
    pub fn progress(&self) -> f64 {
        self.t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morph_completes() {
        let from = vec![1.0, 0.5, 0.5];
        let to = vec![0.5, 1.0, 0.5];
        let mut morph = ScaleMorph::new(from, to, 10).unwrap();

        let mut completed = false;
        for _ in 0..10 {
            completed = morph.tick();
        }
        assert!(completed, "morph should complete after 10 blocks");
    }

    #[test]
    fn lerp_correct() {
        let from = vec![0.0, 1.0];
        let to = vec![1.0, 0.0];
        let mut morph = ScaleMorph::new(from, to, 2).unwrap();

        // After 1 of 2 blocks: t=0.5
        morph.tick();
        let weights = morph.current_weights();
        assert!(
            (weights[0] - 0.5).abs() < 1e-5,
            "midpoint should be 0.5: {}",
            weights[0]
        );
        assert!(
            (weights[1] - 0.5).abs() < 1e-5,
            "midpoint should be 0.5: {}",
            weights[1]
        );

        // After 2 of 2 blocks: t=1.0
        morph.tick();
        let weights = morph.current_weights();
        assert!(
            (weights[0] - 1.0).abs() < 1e-5,
            "end should be 1.0: {}",
            weights[0]
        );
        assert!(
            (weights[1] - 0.0).abs() < 1e-5,
            "end should be 0.0: {}",
            weights[1]
        );
    }

    #[test]
    fn mismatched_lengths_returns_none() {
        let result = ScaleMorph::new(vec![1.0, 2.0], vec![1.0], 10);
        assert!(result.is_none(), "different lengths should return None");
    }

    #[test]
    fn initial_weights_match_from() {
        let from = vec![1.0, 2.0, 3.0];
        let to = vec![4.0, 5.0, 6.0];
        let morph = ScaleMorph::new(from.clone(), to, 10).unwrap();

        let weights = morph.current_weights();
        assert_eq!(weights, from, "initial weights should match from");
    }

    #[test]
    fn progress_tracks() {
        let mut morph = ScaleMorph::new(vec![1.0], vec![2.0], 4).unwrap();
        assert!((morph.progress() - 0.0).abs() < 1e-10);

        morph.tick();
        assert!((morph.progress() - 0.25).abs() < 1e-10);

        morph.tick();
        assert!((morph.progress() - 0.5).abs() < 1e-10);
    }
}
