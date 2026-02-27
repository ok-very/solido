use super::DspAtom;
use crate::dsp::adsr::AdsrState;

/// ADSR envelope atom wrapping extracted `AdsrState`. 0 inputs, 1 output.
///
/// Control via `set_param("gate", v)`: v > 0.5 triggers note_on, v <= 0.5 triggers note_off.
/// ADSR times controlled via `set_param("a"/"d"/"s"/"r", v)`.
pub struct AdsrAtom {
    state: AdsrState,
    gate: bool,
}

impl AdsrAtom {
    pub fn new(sr: f32) -> Self {
        Self {
            state: AdsrState::new(sr),
            gate: false,
        }
    }
}

impl DspAtom for AdsrAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        output[0] = self.state.process();
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "gate" => {
                let new_gate = value > 0.5;
                if new_gate && !self.gate {
                    self.state.note_on();
                } else if !new_gate && self.gate {
                    self.state.note_off();
                }
                self.gate = new_gate;
                true
            }
            "a" => {
                self.state.attack_ms = value;
                true
            }
            "d" => {
                self.state.decay_ms = value;
                true
            }
            "s" => {
                self.state.sustain = value;
                true
            }
            "r" => {
                self.state.release_ms = value;
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "gate" => Some(if self.gate { 1.0 } else { 0.0 }),
            "a" => Some(self.state.attack_ms),
            "d" => Some(self.state.decay_ms),
            "s" => Some(self.state.sustain),
            "r" => Some(self.state.release_ms),
            _ => None,
        }
    }

    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.state.reset();
        self.gate = false;
    }

    fn name(&self) -> &str {
        "adsr"
    }
}

/// Clock atom: emits trigger pulses (1.0 for one sample) at BPM/division rate.
/// Custom implementation, 0 inputs, 1 output.
pub struct ClockAtom {
    bpm: f32,
    division: f32,
    sample_rate: f32,
    counter: f32,
    samples_per_tick: f32,
}

impl ClockAtom {
    pub fn new(bpm: f32, division: f32, sr: f32) -> Self {
        let mut c = Self {
            bpm,
            division,
            sample_rate: sr,
            counter: 0.0,
            samples_per_tick: 0.0,
        };
        c.recalc();
        c
    }

    fn recalc(&mut self) {
        let beats_per_sec = self.bpm / 60.0;
        let ticks_per_sec = beats_per_sec * self.division;
        self.samples_per_tick = if ticks_per_sec > 0.0 {
            self.sample_rate / ticks_per_sec
        } else {
            f32::MAX
        };
    }
}

impl DspAtom for ClockAtom {
    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        self.counter += 1.0;
        if self.counter >= self.samples_per_tick {
            self.counter -= self.samples_per_tick;
            output[0] = 1.0;
        } else {
            output[0] = 0.0;
        }
    }

    fn set_param(&mut self, name: &str, value: f32) -> bool {
        match name {
            "bpm" => {
                self.bpm = value;
                self.recalc();
                true
            }
            "division" => {
                self.division = value;
                self.recalc();
                true
            }
            _ => false,
        }
    }

    fn get_param(&self, name: &str) -> Option<f32> {
        match name {
            "bpm" => Some(self.bpm),
            "division" => Some(self.division),
            _ => None,
        }
    }

    fn audio_inputs(&self) -> usize {
        0
    }
    fn audio_outputs(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.counter = 0.0;
    }

    fn name(&self) -> &str {
        "clock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::atom::render_atom;

    const SR: f32 = 44100.0;

    #[test]
    fn adsr_full_cycle() {
        let mut atom = AdsrAtom::new(SR);
        // Gate on
        atom.set_param("gate", 1.0);
        let mut out = [0.0f32];

        // Attack: 10ms = 441 samples. After 500 samples, should be at peak or in decay.
        for _ in 0..500 {
            atom.tick(&[], &mut out);
        }
        assert!(out[0] > 0.5, "During attack/decay, level should be high");

        // Run to sustain (10ms attack + 100ms decay = ~4851 samples total)
        for _ in 0..5000 {
            atom.tick(&[], &mut out);
        }
        assert!(
            (out[0] - 0.7).abs() < 0.05,
            "At sustain, level ~0.7, got {}",
            out[0]
        );

        // Gate off
        atom.set_param("gate", 0.0);
        // Release: 200ms = 8820 samples
        for _ in 0..10000 {
            atom.tick(&[], &mut out);
        }
        assert!(
            out[0] < 0.01,
            "After release, should be near 0, got {}",
            out[0]
        );
    }

    #[test]
    fn adsr_retrigger() {
        let mut atom = AdsrAtom::new(SR);
        atom.set_param("gate", 1.0);
        let mut out = [0.0f32];
        for _ in 0..5000 {
            atom.tick(&[], &mut out);
        }
        atom.set_param("gate", 0.0);
        for _ in 0..2000 {
            atom.tick(&[], &mut out);
        }
        let level_before = out[0];
        // Retrigger
        atom.set_param("gate", 1.0);
        atom.tick(&[], &mut out);
        assert!(out[0] >= level_before - 0.01, "Retrigger should not drop level");
    }

    #[test]
    fn adsr_param_readback() {
        let atom = AdsrAtom::new(SR);
        assert!((atom.get_param("a").unwrap() - 10.0).abs() < 0.01);
        assert!((atom.get_param("d").unwrap() - 100.0).abs() < 0.01);
        assert!((atom.get_param("s").unwrap() - 0.7).abs() < 0.01);
        assert!((atom.get_param("r").unwrap() - 200.0).abs() < 0.01);
    }

    #[test]
    fn adsr_io() {
        let atom = AdsrAtom::new(SR);
        assert_eq!(atom.audio_inputs(), 0);
        assert_eq!(atom.audio_outputs(), 1);
    }

    #[test]
    fn clock_fires_at_bpm() {
        // 120 BPM, division=1 → 2 beats/sec → expect 2 triggers per second
        let mut atom = ClockAtom::new(120.0, 1.0, SR);
        let buf = render_atom(&mut atom, 44100); // 1 second
        let trigger_count = buf.iter().filter(|&&s| s > 0.5).count();
        assert!(
            trigger_count >= 1 && trigger_count <= 3,
            "120 BPM div=1 should fire ~2 triggers/sec, got {trigger_count}"
        );
    }

    #[test]
    fn clock_division_doubles_rate() {
        let mut atom = ClockAtom::new(120.0, 2.0, SR);
        let buf = render_atom(&mut atom, 44100);
        let trigger_count = buf.iter().filter(|&&s| s > 0.5).count();
        // 120 BPM * div 2 = 4 ticks/sec
        assert!(
            trigger_count >= 3 && trigger_count <= 5,
            "120 BPM div=2 should fire ~4 triggers/sec, got {trigger_count}"
        );
    }

    #[test]
    fn clock_bpm_change() {
        let mut atom = ClockAtom::new(60.0, 1.0, SR);
        let buf1 = render_atom(&mut atom, 44100);
        let count1 = buf1.iter().filter(|&&s| s > 0.5).count();

        atom.reset();
        atom.set_param("bpm", 240.0);
        let buf2 = render_atom(&mut atom, 44100);
        let count2 = buf2.iter().filter(|&&s| s > 0.5).count();

        assert!(
            count2 > count1,
            "Higher BPM should produce more triggers: {count2} vs {count1}"
        );
    }

    #[test]
    fn clock_io() {
        let atom = ClockAtom::new(120.0, 1.0, SR);
        assert_eq!(atom.audio_inputs(), 0);
        assert_eq!(atom.audio_outputs(), 1);
    }
}
