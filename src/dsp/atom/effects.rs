use super::DspAtom;

/// Gate/threshold comparator. Custom implementation.
/// Output is 1.0 when |input| >= threshold, 0.0 otherwise. 1→1.
pub struct GateAtom {
    threshold: f32,
}

impl GateAtom {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl DspAtom for GateAtom {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        output[0] = if input[0].abs() >= self.threshold {
            1.0
        } else {
            0.0
        };
    }
    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "threshold" => {
                self.threshold = value;
                true
            }
            _ => false,
        }
    }
    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "threshold" => Some(self.threshold),
            _ => None,
        }
    }
    fn audio_inputs(&self) -> usize {
        1
    }
    fn audio_outputs(&self) -> usize {
        1
    }
    fn reset(&mut self) {
        // Stateless — nothing to reset.
    }
    fn name(&self) -> &str {
        "gate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_opens_above_threshold() {
        let mut atom = GateAtom::new(0.5);
        let mut out = [0.0f32];
        atom.tick(&[0.7], &mut out);
        assert_eq!(out[0], 1.0);
        atom.tick(&[0.3], &mut out);
        assert_eq!(out[0], 0.0);
        atom.tick(&[-0.6], &mut out);
        assert_eq!(out[0], 1.0); // abs(-0.6) >= 0.5
    }

    #[test]
    fn gate_threshold_change() {
        let mut atom = GateAtom::new(0.5);
        let mut out = [0.0f32];
        atom.tick(&[0.4], &mut out);
        assert_eq!(out[0], 0.0);
        atom.set_param("threshold", 0.3);
        atom.tick(&[0.4], &mut out);
        assert_eq!(out[0], 1.0);
    }
}
