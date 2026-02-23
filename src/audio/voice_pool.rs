use crate::substrate::audio::{AudioCommand, VoiceParam};

use super::voice::{AdsrStage, Voice};

/// Maximum simultaneous voices. Fixed-size array — no heap allocation after init.
pub const MAX_VOICES: usize = 8;

/// Fixed-size voice pool for the audio thread.
///
/// Manages voice lifecycle: spawn, kill, steal, render. All methods are
/// allocation-free and safe for the audio callback hot path.
pub struct VoicePool {
    voices: [Voice; MAX_VOICES],
    next_id: u64,
    sample_rate: f32,
}

impl VoicePool {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            voices: std::array::from_fn(|_| Voice::empty(sample_rate)),
            next_id: 1,
            sample_rate,
        }
    }

    /// Spawn a new voice. Returns the assigned voice ID.
    ///
    /// Voice stealing order: free slot → oldest releasing → oldest active.
    pub fn spawn(&mut self, freq: f32, cutoff: f32, amp: f32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        // 1. Find a free slot (inactive)
        if let Some(slot) = self.voices.iter_mut().find(|v| !v.active) {
            *slot = Voice::new(id, freq, cutoff, amp, self.sample_rate);
            return id;
        }

        // 2. Steal oldest voice in Release stage (lowest ID = oldest)
        if let Some(slot) = self
            .voices
            .iter_mut()
            .filter(|v| v.envelope.stage() == AdsrStage::Release)
            .min_by_key(|v| v.id)
        {
            *slot = Voice::new(id, freq, cutoff, amp, self.sample_rate);
            return id;
        }

        // 3. Steal oldest active voice (lowest ID)
        if let Some(slot) = self.voices.iter_mut().min_by_key(|v| v.id) {
            *slot = Voice::new(id, freq, cutoff, amp, self.sample_rate);
            return id;
        }

        id
    }

    /// Trigger release on a voice. The voice continues playing through its
    /// ADSR release stage and is reclaimed when the envelope reaches zero.
    pub fn kill(&mut self, id: u64) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id && v.active) {
            voice.note_off();
        }
    }

    /// Update a parameter on a specific voice.
    pub fn set_param(&mut self, id: u64, param: VoiceParam, value: f32) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id && v.active) {
            match param {
                VoiceParam::Frequency => voice.frequency = value,
                VoiceParam::Cutoff => voice.filter_cutoff = value,
                VoiceParam::Amplitude => voice.amplitude = value,
                VoiceParam::FilterQ => voice.filter_resonance = value,
            }
        }
    }

    /// Kill all voices immediately. No release, just silence.
    pub fn panic(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
        }
    }

    /// Render all active voices into the output buffer, mixing additively.
    ///
    /// Output is zeroed first, then each active voice is summed in.
    /// Mono signal is duplicated to all channels. Output uses tanh soft-clipping
    /// to avoid harsh digital distortion when voices stack.
    pub fn process_block(&mut self, output: &mut [f32], channels: u16) {
        // Zero the buffer
        for s in output.iter_mut() {
            *s = 0.0;
        }

        let ch = channels as usize;
        if ch == 0 {
            return;
        }
        let num_frames = output.len() / ch;

        // Count active voices for output scaling
        let active = self.voices.iter().filter(|v| v.active).count().max(1) as f32;
        // Scale factor: attenuate proportional to voice count to prevent clipping
        let scale = 1.0 / active.sqrt();

        for voice in self.voices.iter_mut().filter(|v| v.active) {
            for frame in 0..num_frames {
                let sample = voice.render_sample(self.sample_rate);
                for c in 0..ch {
                    output[frame * ch + c] += sample;
                }
            }
        }

        // Apply scaling and soft-clip via tanh to avoid harsh digital clicks
        for s in output.iter_mut() {
            *s = (*s * scale).tanh();
        }
    }

    /// Number of currently active voices.
    pub fn active_count(&self) -> u32 {
        self.voices.iter().filter(|v| v.active).count() as u32
    }

    /// Dispatch an AudioCommand to the appropriate method.
    pub fn dispatch_command(&mut self, cmd: AudioCommand) {
        match cmd {
            AudioCommand::SpawnVoice {
                id: _,
                freq,
                cutoff,
                amp,
            } => {
                self.spawn(freq, cutoff, amp);
            }
            AudioCommand::KillVoice(id) => self.kill(id),
            AudioCommand::SetParam { id, param, value } => self.set_param(id, param, value),
            AudioCommand::Panic => self.panic(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    #[test]
    fn spawn_returns_unique_ids() {
        let mut pool = VoicePool::new(SR);
        let id1 = pool.spawn(440.0, 2000.0, 0.5);
        let id2 = pool.spawn(550.0, 2000.0, 0.5);
        let id3 = pool.spawn(660.0, 2000.0, 0.5);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn spawn_activates_voice() {
        let mut pool = VoicePool::new(SR);
        assert_eq!(pool.active_count(), 0);
        pool.spawn(440.0, 2000.0, 0.5);
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn kill_triggers_release() {
        let mut pool = VoicePool::new(SR);
        let id = pool.spawn(440.0, 2000.0, 0.5);
        // Advance a bit so voice is past attack
        let mut buf = vec![0.0; 512];
        pool.process_block(&mut buf, 1);

        pool.kill(id);
        // Voice should still be active (in release), not immediately dead
        assert_eq!(pool.active_count(), 1);
        let voice = pool.voices.iter().find(|v| v.id == id).unwrap();
        assert_eq!(voice.envelope.stage(), AdsrStage::Release);
    }

    #[test]
    fn process_block_produces_audio() {
        let mut pool = VoicePool::new(SR);
        pool.spawn(440.0, 5000.0, 0.5);

        let mut buf = vec![0.0; 1024];
        pool.process_block(&mut buf, 1);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.01, "Active voice should produce audio, energy={energy}");
    }

    #[test]
    fn process_block_silence_when_empty() {
        let mut pool = VoicePool::new(SR);
        let mut buf = vec![0.0; 512];
        pool.process_block(&mut buf, 1);
        assert!(buf.iter().all(|&s| s == 0.0), "Empty pool should produce silence");
    }

    #[test]
    fn max_voices_steals() {
        let mut pool = VoicePool::new(SR);
        // Fill all 8 slots
        for i in 0..8 {
            pool.spawn(440.0 + i as f32 * 50.0, 2000.0, 0.5);
        }
        assert_eq!(pool.active_count(), 8);

        // 9th spawn should steal (still 8 active)
        pool.spawn(880.0, 2000.0, 0.5);
        assert_eq!(pool.active_count(), 8);
    }

    #[test]
    fn panic_kills_all() {
        let mut pool = VoicePool::new(SR);
        for _ in 0..5 {
            pool.spawn(440.0, 2000.0, 0.5);
        }
        assert_eq!(pool.active_count(), 5);
        pool.panic();
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn set_param_changes_frequency() {
        let mut pool = VoicePool::new(SR);
        let id = pool.spawn(440.0, 2000.0, 0.5);
        pool.set_param(id, VoiceParam::Frequency, 880.0);
        let voice = pool.voices.iter().find(|v| v.id == id).unwrap();
        assert!((voice.frequency - 880.0).abs() < 1e-6);
    }

    #[test]
    fn voice_reclaimed_after_release() {
        let mut pool = VoicePool::new(SR);
        let id = pool.spawn(440.0, 2000.0, 0.5);

        // Run a bit, then kill
        let mut buf = vec![0.0; 2048];
        pool.process_block(&mut buf, 1);
        pool.kill(id);

        // Run long enough for release to complete (200ms = ~8820 samples)
        for _ in 0..20 {
            pool.process_block(&mut buf, 1);
        }

        // Voice should be reclaimed
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn dispatch_command_spawn() {
        let mut pool = VoicePool::new(SR);
        pool.dispatch_command(AudioCommand::SpawnVoice {
            id: 42,
            freq: 440.0,
            cutoff: 2000.0,
            amp: 0.5,
        });
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn dispatch_command_panic() {
        let mut pool = VoicePool::new(SR);
        pool.spawn(440.0, 2000.0, 0.5);
        pool.dispatch_command(AudioCommand::Panic);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn stereo_output() {
        let mut pool = VoicePool::new(SR);
        pool.spawn(440.0, 5000.0, 0.5);

        // 2 channels, 256 frames = 512 samples
        let mut buf = vec![0.0; 512];
        pool.process_block(&mut buf, 2);

        // Left and right channels should be identical (mono duplication)
        for frame in 0..256 {
            assert!(
                (buf[frame * 2] - buf[frame * 2 + 1]).abs() < 1e-6,
                "L/R should match at frame {frame}"
            );
        }
    }
}
