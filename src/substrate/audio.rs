use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

use crate::audio::voice_pool::VoicePool;

use super::channel::{self, Receiver, Sender};

/// Commands sent from the control thread to the audio callback.
/// Travels through a lock-free ring buffer — no allocations in the audio path.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    SpawnVoice {
        id: u64,
        freq: f32,
        cutoff: f32,
        amp: f32,
    },
    KillVoice(u64),
    SetParam {
        id: u64,
        param: VoiceParam,
        value: f32,
    },
    /// Kill all voices immediately (panic button).
    Panic,
}

#[derive(Debug, Clone)]
pub enum VoiceParam {
    Frequency,
    Cutoff,
    Amplitude,
    FilterQ,
}

/// Analysis data sent from the audio callback back to the control thread.
#[derive(Debug, Clone)]
pub struct AudioAnalysis {
    pub rms: f32,
    pub peak: f32,
    pub active_count: u32,
}

/// Audio substrate: owns the cpal output stream.
///
/// The audio callback runs on a high-priority OS thread with a VoicePool
/// rendering synthesis voices. Commands arrive via lock-free ring buffer,
/// analysis flows back via a second ring buffer.
///
/// Channel endpoints (Sender/Receiver) are returned from `new()` and
/// owned by VoiceModule on the control thread.
pub struct AudioSubstrate {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioSubstrate {
    /// Initialize the audio output stream with a VoicePool in the callback.
    ///
    /// Returns `(substrate, cmd_sender, analysis_receiver)` so the caller
    /// can pass the channel endpoints to VoiceModule. Returns None if no
    /// audio device is available.
    pub fn new() -> Option<(Self, Sender<AudioCommand>, Receiver<AudioAnalysis>)> {
        let host = cpal::default_host();

        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                log::warn!("No audio output device found — audio disabled");
                return None;
            }
        };

        let supported_config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to get audio config: {e} — audio disabled");
                return None;
            }
        };

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();

        // Command channel: control thread → audio callback (256 slots)
        let (cmd_tx, mut cmd_rx) = channel::channel::<AudioCommand>(256);
        // Analysis channel: audio callback → control thread (64 slots)
        let (mut analysis_tx, analysis_rx) = channel::channel::<AudioAnalysis>(64);

        // Voice pool lives on the audio thread — moved into the callback closure
        let mut pool = VoicePool::new(sample_rate as f32);

        // Block counter for periodic analysis (every ~1024 samples)
        let mut sample_counter: u32 = 0;
        let mut rms_accum: f32 = 0.0;
        let mut peak: f32 = 0.0;
        let analysis_period: u32 = 1024;

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // Drain commands and dispatch to voice pool
                    while let Some(cmd) = cmd_rx.try_recv() {
                        pool.dispatch_command(cmd);
                    }

                    // Render voices into the output buffer
                    pool.process_block(data, channels);

                    // Accumulate analysis on rendered audio
                    let ch = channels as usize;
                    let mono_samples = if ch > 0 { data.len() / ch } else { 0 };
                    for i in 0..mono_samples {
                        let s = data[i * ch];
                        rms_accum += s * s;
                        peak = peak.max(s.abs());
                        sample_counter += 1;

                        if sample_counter >= analysis_period {
                            let rms = (rms_accum / sample_counter as f32).sqrt();
                            let _ = analysis_tx.try_send(AudioAnalysis {
                                rms,
                                peak,
                                active_count: pool.active_count(),
                            });
                            sample_counter = 0;
                            rms_accum = 0.0;
                            peak = 0.0;
                        }
                    }
                },
                move |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            ),
            _ => {
                log::warn!(
                    "Unsupported sample format {sample_format:?} — audio disabled"
                );
                return None;
            }
        };

        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to build audio stream: {e} — audio disabled");
                return None;
            }
        };

        if let Err(e) = stream.play() {
            log::warn!("Failed to start audio stream: {e} — audio disabled");
            return None;
        }

        log::info!("Audio: {sample_rate}Hz, {channels}ch, f32");

        Some((
            Self {
                _stream: stream,
                sample_rate,
                channels,
            },
            cmd_tx,
            analysis_rx,
        ))
    }
}
