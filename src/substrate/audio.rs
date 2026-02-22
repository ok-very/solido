use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

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
}

/// Audio substrate: cpal stream + command channels.
///
/// The audio callback runs on a high-priority OS thread. It reads AudioCommands
/// from a ring buffer (non-blocking) and writes silence (for now — voice DSP
/// comes in S05). No allocations, no mutexes, no panics in the callback.
pub struct AudioSubstrate {
    _stream: cpal::Stream,
    cmd_tx: Sender<AudioCommand>,
    analysis_rx: Receiver<AudioAnalysis>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioSubstrate {
    /// Initialize the audio output stream. Returns None if no audio device available.
    pub fn new() -> Option<Self> {
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

        // Block counter for periodic analysis (every ~1024 samples)
        let mut sample_counter: u32 = 0;
        let mut rms_accum: f32 = 0.0;
        let mut peak: f32 = 0.0;
        let analysis_period: u32 = 1024;

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // Drain commands (non-blocking)
                    while let Some(_cmd) = cmd_rx.try_recv() {
                        // TODO (S05): dispatch to VoicePool
                    }

                    // Write silence for now — voice DSP comes in S05
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }

                    // Accumulate analysis
                    let mono_samples = data.len() / channels as usize;
                    for i in 0..mono_samples {
                        let s = data[i * channels as usize];
                        rms_accum += s * s;
                        peak = peak.max(s.abs());
                        sample_counter += 1;

                        if sample_counter >= analysis_period {
                            let rms = (rms_accum / sample_counter as f32).sqrt();
                            let _ = analysis_tx.try_send(AudioAnalysis { rms, peak });
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

        Some(Self {
            _stream: stream,
            cmd_tx,
            analysis_rx,
            sample_rate,
            channels,
        })
    }

    /// Send a command to the audio thread. Non-blocking.
    pub fn send_command(&mut self, cmd: AudioCommand) -> Result<(), AudioCommand> {
        self.cmd_tx.try_send(cmd)
    }

    /// Read latest analysis from the audio thread. Returns the most recent if multiple queued.
    pub fn latest_analysis(&mut self) -> Option<AudioAnalysis> {
        let mut latest = None;
        while let Some(a) = self.analysis_rx.try_recv() {
            latest = Some(a);
        }
        latest
    }
}
