use super::automation::{AutomationLane, AutomationTarget};
use super::voice_bus::{ChannelMeter, VoiceBusHandles, MAX_CHANNELS};

/// Automation read/write mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationMode {
    Off,
    Read,
    Write,
    Latch,
}

impl AutomationMode {
    pub const ALL: [AutomationMode; 4] = [
        AutomationMode::Off,
        AutomationMode::Read,
        AutomationMode::Write,
        AutomationMode::Latch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Read => "Read",
            Self::Write => "Write",
            Self::Latch => "Latch",
        }
    }
}

/// Control-thread mixer state. Holds handles to the audio-thread VoiceBus,
/// latest meter snapshots, automation lanes, and transport state.
pub struct MixerState {
    pub handles: VoiceBusHandles,
    pub meters: [ChannelMeter; MAX_CHANNELS],
    pub meter_count: usize,
    pub automation: Vec<AutomationLane>,
    pub automation_mode: AutomationMode,
    pub transport_time: f64,
    pub transport_running: bool,
}

impl MixerState {
    pub fn new(handles: VoiceBusHandles) -> Self {
        let channel_count = handles.strips.len();

        // Create default automation lanes: one gain + one pan per channel
        let mut automation = Vec::new();
        for ch in 0..channel_count {
            automation.push(AutomationLane::new(AutomationTarget::Gain, ch));
            automation.push(AutomationLane::new(AutomationTarget::Pan, ch));
        }

        Self {
            handles,
            meters: [ChannelMeter::default(); MAX_CHANNELS],
            meter_count: channel_count,
            automation,
            automation_mode: AutomationMode::Off,
            transport_time: 0.0,
            transport_running: false,
        }
    }

    /// Apply automation lanes in Read or Latch mode: read interpolated values
    /// and push them to the Shared handles.
    pub fn apply_automation(&self) {
        if self.automation_mode != AutomationMode::Read
            && self.automation_mode != AutomationMode::Latch
        {
            return;
        }

        let t = self.transport_time;
        for lane in &self.automation {
            if lane.channel >= self.handles.strips.len() {
                continue;
            }
            if let Some(value) = lane.read_at(t) {
                let strip = &self.handles.strips[lane.channel];
                match lane.target {
                    AutomationTarget::Gain => strip.gain.set(value),
                    AutomationTarget::Pan => strip.pan.set(value),
                }
            }
        }
    }

    /// Record a value into the matching automation lane (Write/Latch mode).
    pub fn record_automation(
        &mut self,
        channel: usize,
        target: AutomationTarget,
        value: f32,
    ) {
        if self.automation_mode != AutomationMode::Write
            && self.automation_mode != AutomationMode::Latch
        {
            return;
        }
        if let Some(lane) = self
            .automation
            .iter_mut()
            .find(|l| l.channel == channel && l.target == target)
        {
            lane.record(self.transport_time, value);
        }
    }

    /// Advance transport time by delta seconds.
    pub fn advance_transport(&mut self, delta: f64) {
        if self.transport_running {
            self.transport_time += delta;
        }
    }

    /// Reset transport to beginning.
    pub fn rewind(&mut self) {
        self.transport_time = 0.0;
    }

    /// Toggle play/pause.
    pub fn toggle_transport(&mut self) {
        self.transport_running = !self.transport_running;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::shared::shared;

    use crate::audio::voice_bus::ChannelStripHandles;

    fn make_handles(n: usize) -> VoiceBusHandles {
        let strips: Vec<ChannelStripHandles> = (0..n)
            .map(|i| ChannelStripHandles {
                label: format!("ch{i}"),
                gain: shared(1.0),
                pan: shared(0.0),
                mute: shared(0.0),
                solo: shared(0.0),
            })
            .collect();
        VoiceBusHandles {
            strips,
            master_gain: shared(1.0),
        }
    }

    #[test]
    fn automation_read_applies_gain() {
        let handles = make_handles(2);
        let mut state = MixerState::new(handles);
        state.automation_mode = AutomationMode::Write;
        state.transport_running = true;

        // Record some keyframes
        state.transport_time = 0.0;
        state.record_automation(0, AutomationTarget::Gain, 0.2);
        state.transport_time = 1.0;
        state.record_automation(0, AutomationTarget::Gain, 0.8);

        // Switch to read mode and seek to midpoint
        state.automation_mode = AutomationMode::Read;
        state.transport_time = 0.5;
        state.apply_automation();

        let gain = state.handles.strips[0].gain.value();
        assert!((gain - 0.5).abs() < 0.05, "gain should be ~0.5, got {gain}");
    }

    #[test]
    fn transport_advance() {
        let handles = make_handles(1);
        let mut state = MixerState::new(handles);
        state.transport_running = true;
        state.advance_transport(0.5);
        assert!((state.transport_time - 0.5).abs() < 0.001);
        state.transport_running = false;
        state.advance_transport(0.5);
        assert!((state.transport_time - 0.5).abs() < 0.001); // didn't advance
    }

    #[test]
    fn rewind_resets_time() {
        let handles = make_handles(1);
        let mut state = MixerState::new(handles);
        state.transport_time = 5.0;
        state.rewind();
        assert_eq!(state.transport_time, 0.0);
    }
}
