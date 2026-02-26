use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

use crate::audio::gain_staging;
use crate::audio::master_bus::MasterBus;
use crate::audio::voice_bus::{BusMeterReport, VoiceBus, VoiceBusHandles, MAX_CHANNELS};
use crate::audio::voice_pool::VoicePool;
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::organism_dsp::{OrganismDsp, SharedHandles};
use crate::organism::dna::OrganismDna;

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
    /// Kill all voices within ~1Hz of the target frequency.
    /// Used for voice dedup: kill previous voice at same pitch before spawning new one.
    KillVoicesAtFreq(f32),
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

/// Per-organism control endpoints returned to the control thread.
///
/// Contains the Sender for DspCommand (discrete events), Receiver for
/// DspAnalysis (periodic audio stats), and SharedHandles (lock-free
/// atomic floats for continuous param control).
pub struct OrganismEndpoint {
    pub cmd_tx: Sender<DspCommand>,
    pub analysis_rx: Receiver<DspAnalysis>,
    pub shared_handles: SharedHandles,
}

/// Audio substrate: owns the cpal output stream.
///
/// The audio callback runs on a high-priority OS thread with a VoicePool
/// rendering synthesis voices and OrganismDsp instances processing organism
/// audio. Commands arrive via lock-free ring buffers, analysis flows back
/// via separate ring buffers.
///
/// All sources flow through VoiceBus channel strips (gain/pan/mute/solo)
/// before reaching MasterBus for final limiting and DC blocking.
pub struct AudioSubstrate {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioSubstrate {
    /// Initialize the audio output stream with VoicePool + organism DSPs + VoiceBus.
    ///
    /// `organism_dna` provides blueprints for organisms to build at the
    /// discovered sample rate. Returns per-organism control endpoints,
    /// VoiceBus handles for the mixer UI, and a meter report receiver.
    ///
    /// Returns None if no audio device is available.
    pub fn new(
        organism_dna: &[OrganismDna],
    ) -> Option<(
        Self,
        Sender<AudioCommand>,
        Receiver<AudioAnalysis>,
        Vec<OrganismEndpoint>,
        VoiceBusHandles,
        Receiver<BusMeterReport>,
    )> {
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

        let sr = sample_rate as f32;

        // VoicePool command channel: control thread → audio callback (256 slots)
        let (cmd_tx, mut cmd_rx) = channel::channel::<AudioCommand>(256);
        // VoicePool analysis channel: audio callback → control thread (64 slots)
        let (mut analysis_tx, analysis_rx) = channel::channel::<AudioAnalysis>(64);
        // Meter report channel: audio callback → control thread (32 slots)
        let (mut meter_tx, meter_rx) = channel::channel::<BusMeterReport>(32);

        // Build organism DSPs at the discovered sample rate
        let mut organisms: Vec<OrganismDsp> = Vec::new();
        let mut org_cmd_rxs: Vec<Receiver<DspCommand>> = Vec::new();
        let mut org_analysis_txs: Vec<Sender<DspAnalysis>> = Vec::new();
        let mut endpoints: Vec<OrganismEndpoint> = Vec::new();
        let mut org_names: Vec<String> = Vec::new();

        for dna in organism_dna {
            match OrganismDsp::from_dna(dna, sr) {
                Some((org_dsp, shared_handles)) => {
                    let (org_cmd_tx, org_cmd_rx) = channel::channel::<DspCommand>(64);
                    let (org_analysis_tx, org_analysis_rx) =
                        channel::channel::<DspAnalysis>(32);

                    organisms.push(org_dsp);
                    org_cmd_rxs.push(org_cmd_rx);
                    org_analysis_txs.push(org_analysis_tx);
                    org_names.push(dna.name.clone());

                    endpoints.push(OrganismEndpoint {
                        cmd_tx: org_cmd_tx,
                        analysis_rx: org_analysis_rx,
                        shared_handles,
                    });

                    log::info!("Built organism '{}' (species: {})", dna.name, dna.species);
                }
                None => {
                    log::warn!("Failed to build organism '{}' — skipping", dna.name);
                }
            }
        }

        let org_count = organisms.len();

        // Build VoiceBus channel strip config:
        // Channel 0 = VoicePool, then one per organism
        // Gains from audio::gain_staging constants (documented headroom budget)
        let mut bus_channels: Vec<(&str, f32)> = Vec::new();
        bus_channels.push(("VoicePool", gain_staging::VOICE_POOL_GAIN));
        for name in &org_names {
            bus_channels.push((name.as_str(), gain_staging::species_gain(name)));
        }
        let (mut voice_bus, voice_bus_handles) =
            VoiceBus::new(&bus_channels, gain_staging::MASTER_GAIN);

        // Voice pool + master bus live on the audio thread
        let mut pool = VoicePool::new(sr);
        let mut master_bus = MasterBus::new(sr);

        // Pre-allocate pool_buf for separate VoicePool rendering
        let max_pool_buf = 8192 * channels as usize;
        let mut pool_buf: Vec<f32> = vec![0.0; max_pool_buf];

        // Block counter for periodic global analysis (every ~1024 samples)
        let mut sample_counter: u32 = 0;
        let mut rms_accum: f32 = 0.0;
        let mut peak: f32 = 0.0;
        let analysis_period: u32 = 1024;

        // Per-organism analysis accumulators
        let mut org_sample_counters: Vec<u32> = vec![0; org_count];
        let mut org_rms_accums: Vec<f32> = vec![0.0; org_count];
        let mut org_peaks: Vec<f32> = vec![0.0; org_count];

        // Stack-allocated source array for VoiceBus
        let bus_channel_count = 1 + org_count; // VoicePool + organisms
        let _ = bus_channel_count; // used in closure

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    let ch = channels as usize;
                    let frames = data.len() / ch;

                    // --- Render VoicePool to separate buffer ---
                    while let Some(cmd) = cmd_rx.try_recv() {
                        pool.dispatch_command(cmd);
                    }
                    // Ensure pool_buf is large enough, zero it
                    if pool_buf.len() < data.len() {
                        pool_buf.resize(data.len(), 0.0);
                    }
                    for s in &mut pool_buf[..data.len()] {
                        *s = 0.0;
                    }
                    pool.process_block(&mut pool_buf[..data.len()], channels);

                    // --- Per-frame: assemble sources, run through VoiceBus ---
                    for frame in 0..frames {
                        let base = frame * ch;

                        // Build source array for this frame
                        let mut sources = [[0.0f32; 2]; MAX_CHANNELS];

                        // Source 0: VoicePool
                        sources[0][0] = pool_buf[base];
                        if ch > 1 {
                            sources[0][1] = pool_buf[base + 1];
                        } else {
                            sources[0][1] = pool_buf[base];
                        }

                        // Sources 1..N: Organisms (per-sample tick)
                        for (org_idx, org) in organisms.iter_mut().enumerate() {
                            // Drain commands for this organism
                            while let Some(cmd) = org_cmd_rxs[org_idx].try_recv() {
                                org.handle_command(cmd);
                            }

                            // Tick one sample
                            let mut out = [0.0f32; 2];
                            org.tick(&mut out);
                            sources[1 + org_idx] = out;

                            // Per-organism analysis accumulation
                            let mono = (out[0] + out[1]) * 0.5;
                            org_rms_accums[org_idx] += mono * mono;
                            org_peaks[org_idx] =
                                org_peaks[org_idx].max(out[0].abs()).max(out[1].abs());
                            org_sample_counters[org_idx] += 1;

                            if org_sample_counters[org_idx] >= analysis_period {
                                let org_rms = (org_rms_accums[org_idx]
                                    / org_sample_counters[org_idx] as f32)
                                    .sqrt();
                                let _ =
                                    org_analysis_txs[org_idx].try_send(DspAnalysis {
                                        rms: org_rms,
                                        peak: org_peaks[org_idx],
                                    });
                                org_sample_counters[org_idx] = 0;
                                org_rms_accums[org_idx] = 0.0;
                                org_peaks[org_idx] = 0.0;
                            }
                        }

                        // Run through VoiceBus channel strips
                        let source_count = 1 + org_count;
                        let mut bus_out = [0.0f32; 2];
                        voice_bus.process_frame(
                            &sources[..source_count],
                            &mut bus_out,
                        );

                        // Write to output buffer
                        data[base] = bus_out[0];
                        if ch > 1 {
                            data[base + 1] = bus_out[1];
                        }
                    }

                    // Send bus meter report if analysis period elapsed
                    if voice_bus.should_report() {
                        let _ = meter_tx.try_send(voice_bus.collect_meters());
                    }

                    // Post-process through FunDSP master bus (crossover + limiters + DC block)
                    master_bus.process(data, channels);

                    // Accumulate global analysis on rendered audio
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

        log::info!(
            "Audio: {sample_rate}Hz, {channels}ch, f32, {} organisms, VoiceBus {} strips",
            org_count,
            voice_bus_handles.strips.len(),
        );

        Some((
            Self {
                _stream: stream,
                sample_rate,
                channels,
            },
            cmd_tx,
            analysis_rx,
            endpoints,
            voice_bus_handles,
            meter_rx,
        ))
    }
}
