use fundsp::prelude32::{shared, Shared};

pub const MAX_CHANNELS: usize = 8;

/// Per-channel strip on the audio thread.
/// Gain, pan, mute, solo are lock-free `Shared` atomics written by the UI thread.
pub struct ChannelStrip {
    pub gain: Shared,
    pub pan: Shared,
    pub mute: Shared,
    pub solo: Shared,
    pub label: String,
    // Audio-thread metering accumulators
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
}

/// Control-thread handles for one channel strip.
/// Cloned from ChannelStrip's Shared params at construction time.
pub struct ChannelStripHandles {
    pub label: String,
    pub gain: Shared,
    pub pan: Shared,
    pub mute: Shared,
    pub solo: Shared,
}

/// Metering snapshot for one channel.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelMeter {
    pub rms: f32,
    pub peak: f32,
}

/// Meter report sent from audio thread to control thread via ringbuf.
#[derive(Debug, Clone, Copy)]
pub struct BusMeterReport {
    pub meters: [ChannelMeter; MAX_CHANNELS],
    pub count: u8,
}

impl Default for BusMeterReport {
    fn default() -> Self {
        Self {
            meters: [ChannelMeter::default(); MAX_CHANNELS],
            count: 0,
        }
    }
}

/// The mixing bus on the audio thread. Processes per-sample channel strips
/// with gain, pan, mute/solo, then sums through a master gain.
pub struct VoiceBus {
    strips: Vec<ChannelStrip>,
    master_gain: Shared,
    analysis_period: u32,
    sample_counter: u32,
}

/// Control-thread handles for the entire bus.
pub struct VoiceBusHandles {
    pub strips: Vec<ChannelStripHandles>,
    pub master_gain: Shared,
}

impl ChannelStrip {
    fn new(label: &str, default_gain: f32) -> (Self, ChannelStripHandles) {
        let gain = shared(default_gain);
        let pan = shared(0.0);
        let mute = shared(0.0);
        let solo = shared(0.0);
        let handles = ChannelStripHandles {
            label: label.to_string(),
            gain: gain.clone(),
            pan: pan.clone(),
            mute: mute.clone(),
            solo: solo.clone(),
        };
        let strip = Self {
            gain,
            pan,
            mute,
            solo,
            label: label.to_string(),
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
        };
        (strip, handles)
    }

    /// Process one stereo sample through this channel strip.
    /// Returns [left, right] after gain, pan, and mute/solo logic.
    fn process_sample(&mut self, left: f32, right: f32, any_solo: bool) -> [f32; 2] {
        let gain = self.gain.value();
        let pan = self.pan.value().clamp(-1.0, 1.0);
        let is_muted = self.mute.value() > 0.5;
        let is_soloed = self.solo.value() > 0.5;

        // Meter on pre-mute signal (post-gain)
        let mono = (left + right) * 0.5 * gain;
        self.rms_acc += mono * mono;
        self.peak = self.peak.max(mono.abs());
        self.sample_count += 1;

        // Mute or solo-exclusion → silence
        if is_muted || (any_solo && !is_soloed) {
            return [0.0, 0.0];
        }

        // Constant-power pan: center = -3dB both sides
        let pan_norm = (pan + 1.0) * 0.5; // [0, 1]
        let angle = pan_norm * std::f32::consts::FRAC_PI_2;
        let pan_l = angle.cos();
        let pan_r = angle.sin();

        [left * gain * pan_l, right * gain * pan_r]
    }

    /// Harvest metering data and reset accumulators.
    fn collect_meter(&mut self) -> ChannelMeter {
        let meter = if self.sample_count > 0 {
            ChannelMeter {
                rms: (self.rms_acc / self.sample_count as f32).sqrt(),
                peak: self.peak,
            }
        } else {
            ChannelMeter::default()
        };
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
        meter
    }
}

impl VoiceBus {
    /// Create a new VoiceBus with the given channel labels and default gains.
    /// Returns both the audio-thread bus and the control-thread handles.
    pub fn new(channels: &[(&str, f32)], master_gain: f32) -> (Self, VoiceBusHandles) {
        let mg = shared(master_gain);
        let mut strips = Vec::with_capacity(channels.len());
        let mut handle_strips = Vec::with_capacity(channels.len());

        for (label, default_gain) in channels {
            let (strip, handles) = ChannelStrip::new(label, *default_gain);
            strips.push(strip);
            handle_strips.push(handles);
        }

        let bus = Self {
            strips,
            master_gain: mg.clone(),
            analysis_period: 1024,
            sample_counter: 0,
        };
        let handles = VoiceBusHandles {
            strips: handle_strips,
            master_gain: mg,
        };
        (bus, handles)
    }

    /// Process one frame: mix all source stereo pairs through channel strips,
    /// sum, and apply master gain.
    pub fn process_frame(&mut self, sources: &[[f32; 2]], output: &mut [f32; 2]) {
        let any_solo = self.strips.iter().any(|s| s.solo.value() > 0.5);
        let mut sum = [0.0f32; 2];
        for (i, strip) in self.strips.iter_mut().enumerate() {
            if let Some(src) = sources.get(i) {
                let out = strip.process_sample(src[0], src[1], any_solo);
                sum[0] += out[0];
                sum[1] += out[1];
            }
        }
        let mg = self.master_gain.value();
        *output = [sum[0] * mg, sum[1] * mg];
        self.sample_counter += 1;
    }

    /// Returns true when the analysis period has elapsed and meters should be reported.
    pub fn should_report(&self) -> bool {
        self.sample_counter >= self.analysis_period
    }

    /// Collect meter data from all strips and reset the sample counter.
    pub fn collect_meters(&mut self) -> BusMeterReport {
        let mut report = BusMeterReport::default();
        report.count = self.strips.len() as u8;
        for (i, strip) in self.strips.iter_mut().enumerate() {
            if i < MAX_CHANNELS {
                report.meters[i] = strip.collect_meter();
            }
        }
        self.sample_counter = 0;
        report
    }

    /// Number of channel strips.
    pub fn strip_count(&self) -> usize {
        self.strips.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_passthrough() {
        let channels = [("test", 1.0)];
        let (mut bus, _handles) = VoiceBus::new(&channels, 1.0);
        let sources = [[0.5, 0.5]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        // At center pan, constant-power gives cos(pi/4) = sin(pi/4) ≈ 0.707
        let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((out[0] - expected).abs() < 0.001, "left: {}", out[0]);
        assert!((out[1] - expected).abs() < 0.001, "right: {}", out[1]);
    }

    #[test]
    fn mute_silences_channel() {
        let channels = [("test", 1.0)];
        let (mut bus, handles) = VoiceBus::new(&channels, 1.0);
        handles.strips[0].mute.set(1.0);
        let sources = [[0.5, 0.5]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn solo_isolates_channel() {
        let channels = [("a", 1.0), ("b", 1.0)];
        let (mut bus, handles) = VoiceBus::new(&channels, 1.0);
        // Solo channel 0 — channel 1 should be silenced
        handles.strips[0].solo.set(1.0);
        let sources = [[0.4, 0.4], [0.6, 0.6]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        // Only channel 0 contributes
        let expected = 0.4 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((out[0] - expected).abs() < 0.001);
        assert!((out[1] - expected).abs() < 0.001);
    }

    #[test]
    fn master_gain_scales_output() {
        let channels = [("test", 1.0)];
        let (mut bus, handles) = VoiceBus::new(&channels, 0.5);
        let sources = [[1.0, 1.0]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        let expected = 1.0 * std::f32::consts::FRAC_1_SQRT_2 * 0.5;
        assert!((out[0] - expected).abs() < 0.001);
        // Change master gain
        handles.master_gain.set(0.25);
        bus.process_frame(&sources, &mut out);
        let expected2 = 1.0 * std::f32::consts::FRAC_1_SQRT_2 * 0.25;
        assert!((out[0] - expected2).abs() < 0.001);
    }

    #[test]
    fn pan_hard_left() {
        let channels = [("test", 1.0)];
        let (mut bus, handles) = VoiceBus::new(&channels, 1.0);
        handles.strips[0].pan.set(-1.0);
        let sources = [[1.0, 1.0]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        // Hard left: pan_norm=0, angle=0, cos(0)=1, sin(0)=0
        assert!((out[0] - 1.0).abs() < 0.001, "left should be ~1.0: {}", out[0]);
        assert!(out[1].abs() < 0.001, "right should be ~0.0: {}", out[1]);
    }

    #[test]
    fn pan_hard_right() {
        let channels = [("test", 1.0)];
        let (mut bus, handles) = VoiceBus::new(&channels, 1.0);
        handles.strips[0].pan.set(1.0);
        let sources = [[1.0, 1.0]];
        let mut out = [0.0; 2];
        bus.process_frame(&sources, &mut out);
        // Hard right: pan_norm=1, angle=pi/2, cos(pi/2)≈0, sin(pi/2)=1
        assert!(out[0].abs() < 0.001, "left should be ~0.0: {}", out[0]);
        assert!((out[1] - 1.0).abs() < 0.001, "right should be ~1.0: {}", out[1]);
    }

    #[test]
    fn meter_accumulates_correctly() {
        let channels = [("test", 1.0)];
        let (mut bus, _handles) = VoiceBus::new(&channels, 1.0);
        // Process several frames
        for _ in 0..100 {
            let mut out = [0.0; 2];
            bus.process_frame(&[[0.5, 0.5]], &mut out);
        }
        let report = bus.collect_meters();
        assert_eq!(report.count, 1);
        assert!(report.meters[0].rms > 0.0);
        assert!(report.meters[0].peak > 0.0);
    }

    #[test]
    fn should_report_after_analysis_period() {
        let channels = [("test", 1.0)];
        let (mut bus, _handles) = VoiceBus::new(&channels, 1.0);
        for _ in 0..1023 {
            let mut out = [0.0; 2];
            bus.process_frame(&[[0.1, 0.1]], &mut out);
        }
        assert!(!bus.should_report());
        let mut out = [0.0; 2];
        bus.process_frame(&[[0.1, 0.1]], &mut out);
        assert!(bus.should_report());
    }

    #[test]
    fn empty_sources_produces_silence() {
        let channels = [("a", 1.0), ("b", 1.0)];
        let (mut bus, _handles) = VoiceBus::new(&channels, 1.0);
        let sources: &[[f32; 2]] = &[];
        let mut out = [0.0; 2];
        bus.process_frame(sources, &mut out);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn default_gains_match_spec() {
        let channels = [
            ("VoicePool", 0.8),
            ("TBLK", 0.7),
            ("DRON", 0.4),
            ("MELO", 0.6),
        ];
        let (_bus, handles) = VoiceBus::new(&channels, 0.85);
        assert!((handles.strips[0].gain.value() - 0.8).abs() < 0.001);
        assert!((handles.strips[1].gain.value() - 0.7).abs() < 0.001);
        assert!((handles.strips[2].gain.value() - 0.4).abs() < 0.001);
        assert!((handles.strips[3].gain.value() - 0.6).abs() < 0.001);
        assert!((handles.master_gain.value() - 0.85).abs() < 0.001);
    }
}
