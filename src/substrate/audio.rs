use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

use crate::audio::gain_staging;
use crate::audio::master_bus::MasterBus;
use crate::audio::reverb_bus::{ReverbBus, ReverbBusHandles};
use crate::audio::tape_delay_bus::{TapeDelayBus, TapeDelayBusHandles};
use crate::audio::voice_bus::{BusMeterReport, ChannelStrip, VoiceBus, VoiceBusHandles, MAX_CHANNELS};
use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::organism_dsp::{OrganismDsp, SharedHandles};
use crate::dsp::shared::Shared;
use crate::organism::dna::OrganismDna;

use super::channel::{self, Receiver, Sender};

/// Payload sent from the control thread to the audio callback at spawn time.
///
/// All allocation happens on the control thread. The callback receives this
/// payload via SPSC ring buffer and integrates it at the next callback boundary
/// with no blocking and no new allocation (all Vecs are pre-alloc'd to 16).
pub struct SpawnPayload {
    pub dsp: OrganismDsp,
    pub cmd_rx: Receiver<DspCommand>,
    pub analysis_tx: Sender<DspAnalysis>,
    pub strip: ChannelStrip,
    pub reverb_send: Option<Shared>,
    pub tape_delay_send: Option<Shared>,
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
    /// Per-organism reverb send level (None if no reverb bus).
    pub reverb_send: Option<crate::dsp::shared::Shared>,
    /// Per-organism tape delay send level (None if no tape delay bus).
    pub tape_delay_send: Option<crate::dsp::shared::Shared>,
}

/// Audio substrate: owns the cpal output stream.
///
/// The audio callback runs on a high-priority OS thread with OrganismDsp
/// instances processing organism audio. Commands arrive via lock-free ring
/// buffers, analysis flows back via separate ring buffers.
///
/// All sources flow through VoiceBus channel strips (gain/pan/mute/solo)
/// before reaching MasterBus for final limiting and DC blocking.
pub struct AudioSubstrate {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    #[allow(dead_code)]
    pub channels: u16,
    /// SPSC sender for runtime organism spawning. Send a SpawnPayload from
    /// the control thread; the audio callback integrates it at the next boundary.
    pub spawn_tx: Sender<SpawnPayload>,
    /// SPSC sender for despawning organisms. Send the audio_idx to tombstone.
    pub despawn_tx: Sender<usize>,
}

impl AudioSubstrate {
    /// Initialize the audio output stream with organism DSPs + VoiceBus + ReverbBus.
    ///
    /// `organism_dna` provides blueprints for organisms to build at the
    /// discovered sample rate. Returns per-organism control endpoints,
    /// VoiceBus handles for the mixer UI, optional reverb bus handles,
    /// and a meter report receiver.
    ///
    /// Returns None if no audio device is available.
    pub fn new(
        organism_dna: &[OrganismDna],
        playing: crate::dsp::shared::Shared,
    ) -> Option<(
        Self,
        Vec<OrganismEndpoint>,
        VoiceBusHandles,
        Option<ReverbBusHandles>,
        Option<TapeDelayBusHandles>,
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

        #[cfg(debug_assertions)]
        log::warn!("Debug build active — use `cargo run --release` for audio quality. FunDSP has no SIMD/inlining in debug mode and may cause XRuns.");

        let sr = sample_rate as f32;

        // Meter report channel: audio callback → control thread (32 slots)
        let (mut meter_tx, meter_rx) = channel::channel::<BusMeterReport>(32);

        // Spawn channel: control thread → audio callback (capacity 4 concurrent spawns)
        let (spawn_tx, mut spawn_rx) = channel::channel::<SpawnPayload>(4);

        // Despawn channel: control thread → audio callback (tombstone indices)
        let (despawn_tx, mut despawn_rx) = channel::channel::<usize>(16);

        // Build organism DSPs at the discovered sample rate
        let mut organisms: Vec<OrganismDsp> = Vec::with_capacity(16);
        let mut org_cmd_rxs: Vec<Receiver<DspCommand>> = Vec::with_capacity(16);
        let mut org_analysis_txs: Vec<Sender<DspAnalysis>> = Vec::with_capacity(16);
        let mut endpoints: Vec<OrganismEndpoint> = Vec::with_capacity(16);
        let mut org_names: Vec<String> = Vec::with_capacity(16);

        // Collect per-organism reverb send levels from DNA
        let mut org_reverb_sends: Vec<f32> = Vec::new();
        // Track the first reverb send DNA we find (used to configure the bus)
        let mut reverb_send_dna: Option<&crate::organism::dna::ReverbSendDna> = None;

        // Collect per-organism tape delay send levels from DNA
        let mut org_tape_delay_sends: Vec<f32> = Vec::new();
        let mut tape_delay_send_dna: Option<&crate::organism::dna::TapeDelaySendDna> = None;

        for dna in organism_dna {
            let rev_send = dna.sends.as_ref()
                .and_then(|s| s.reverb.as_ref())
                .map(|r| {
                    if reverb_send_dna.is_none() {
                        reverb_send_dna = Some(r);
                    }
                    r.send
                })
                .unwrap_or(0.0);
            org_reverb_sends.push(rev_send);

            let td_send = dna.sends.as_ref()
                .and_then(|s| s.tape_delay.as_ref())
                .map(|td| {
                    if tape_delay_send_dna.is_none() {
                        tape_delay_send_dna = Some(td);
                    }
                    td.send
                })
                .unwrap_or(0.0);
            org_tape_delay_sends.push(td_send);
        }

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
                        reverb_send: None,      // filled in below after ReverbBus is built
                        tape_delay_send: None,  // filled in below after TapeDelayBus is built
                    });

                    log::info!("Built organism '{}' (species: {})", dna.name, dna.species);
                }
                None => {
                    log::warn!("Failed to build organism '{}' — skipping", dna.name);
                }
            }
        }

        let org_count = organisms.len();

        // Build ReverbBus if any organism requests reverb sends
        let (mut reverb_bus_opt, reverb_bus_handles) = if let Some(rdna) = reverb_send_dna {
            let send_levels: Vec<f32> = org_reverb_sends[..org_count].to_vec();
            let (bus, handles) = ReverbBus::new(rdna, &send_levels, sr);
            // Wire send level handles back to endpoints
            for (i, ep) in endpoints.iter_mut().enumerate() {
                if i < handles.send_levels.len() {
                    ep.reverb_send = Some(handles.send_levels[i].clone());
                }
            }
            log::info!("ReverbBus: type={}, {} sends", handles.reverb_type, handles.send_levels.len());
            (Some(bus), Some(handles))
        } else {
            (None, None)
        };

        let (mut tape_delay_bus_opt, tape_delay_bus_handles) = if let Some(tddna) = tape_delay_send_dna {
            let send_levels: Vec<f32> = org_tape_delay_sends[..org_count].to_vec();
            let (bus, handles) = TapeDelayBus::new(tddna, &send_levels, sr);
            // Wire send level handles back to endpoints
            for (i, ep) in endpoints.iter_mut().enumerate() {
                if i < handles.send_levels.len() {
                    ep.tape_delay_send = Some(handles.send_levels[i].clone());
                }
            }
            log::info!("TapeDelayBus: {} sends", handles.send_levels.len());
            (Some(bus), Some(handles))
        } else {
            (None, None)
        };

        // Build VoiceBus channel strip config: one per organism
        // Gains from audio::gain_staging constants (documented headroom budget)
        let mut bus_channels: Vec<(&str, f32)> = Vec::new();
        for name in &org_names {
            bus_channels.push((name.as_str(), gain_staging::species_gain(name)));
        }
        let (mut voice_bus, voice_bus_handles) =
            VoiceBus::new(&bus_channels, gain_staging::MASTER_GAIN);

        // Clone master_gain handle for tape delay return scaling in the callback.
        // VoiceBus reads this same Shared internally; tape delay return must match.
        let master_gain_for_tape = voice_bus_handles.master_gain.clone();

        // Master bus lives on the audio thread
        let mut master_bus = MasterBus::new(sr);

        // Per-organism analysis accumulators (pre-alloc'd to 16 for RT-safe push)
        let analysis_period: u32 = 1024;
        let mut org_sample_counters: Vec<u32> = Vec::with_capacity(16);
        let mut org_rms_accums: Vec<f32> = Vec::with_capacity(16);
        let mut org_peaks: Vec<f32> = Vec::with_capacity(16);
        for _ in 0..org_count {
            org_sample_counters.push(0);
            org_rms_accums.push(0.0);
            org_peaks.push(0.0);
        }

        // Per-organism alive flags (preallocated, stack-sized, no heap on RT thread)
        let mut alive = [true; MAX_CHANNELS];


        // Clone playing handle for the callback (one atomic read per frame)
        let playing_handle = playing;

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // Flush denormals to zero — prevents massive CPU spikes
                    // when filter state variables decay toward zero.
                    #[cfg(target_arch = "x86_64")]
                    #[allow(deprecated)]
                    unsafe {
                        let csr = std::arch::x86_64::_mm_getcsr();
                        // FTZ (bit 15) + DAZ (bit 6)
                        std::arch::x86_64::_mm_setcsr(csr | 0x8040);
                    }

                    let ch = channels as usize;
                    let frames = data.len() / ch;

                    // Integrate any spawned organisms (no alloc if within pre-alloc'd capacity of 16)
                    while let Some(p) = spawn_rx.try_recv() {
                        if organisms.len() >= MAX_CHANNELS {
                            break; // drop payload rather than heap-allocate on RT thread
                        }
                        organisms.push(p.dsp);
                        org_cmd_rxs.push(p.cmd_rx);
                        org_analysis_txs.push(p.analysis_tx);
                        org_sample_counters.push(0);
                        org_rms_accums.push(0.0);
                        org_peaks.push(0.0);
                        voice_bus.add_strip(p.strip);
                        if let (Some(ref mut rb), Some(s)) = (reverb_bus_opt.as_mut(), p.reverb_send) {
                            rb.add_organism_send(s);
                        }
                        if let (Some(ref mut td), Some(s)) = (tape_delay_bus_opt.as_mut(), p.tape_delay_send) {
                            td.add_organism_send(s);
                        }
                    }

                    // Drain despawn commands (RT-safe: try_recv is lock-free)
                    while let Some(idx) = despawn_rx.try_recv() {
                        if idx < alive.len() {
                            alive[idx] = false;
                            voice_bus.mark_dead(idx);
                        }
                    }

                    // Drain commands once per callback (not per frame)
                    for (org_idx, org) in organisms.iter_mut().enumerate() {
                        if !alive[org_idx] { continue; }
                        while let Some(cmd) = org_cmd_rxs[org_idx].try_recv() {
                            org.handle_command(cmd);
                        }
                    }

                    // Check playing state once per callback (RT-safe: AtomicU32 Relaxed)
                    let is_playing = playing_handle.value() > 0.5;

                    // --- Per-frame: assemble sources, run through VoiceBus ---
                    for frame in 0..frames {
                        let base = frame * ch;

                        // Build source array for this frame
                        let mut sources = [[0.0f32; 2]; MAX_CHANNELS];

                        // Sources 0..N: Organisms (per-sample tick, skip dead and stopped)
                        for (org_idx, org) in organisms.iter_mut().enumerate() {
                            if !alive[org_idx] || !is_playing {
                                sources[org_idx] = [0.0, 0.0];
                                continue;
                            }
                            // Tick one sample
                            let mut out = [0.0f32; 2];
                            org.tick(&mut out);
                            sources[org_idx] = out;

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
                                // Populate bridge data from RT-safe accessor
                                // (uses precomputed cell indices, no allocation)
                                let bridge = org.bridge_data();
                                let _ =
                                    org_analysis_txs[org_idx].try_send(DspAnalysis {
                                        rms: org_rms,
                                        peak: org_peaks[org_idx],
                                        seq_pitch_hz: bridge.seq_pitch_hz,
                                        seq_gate: bridge.seq_gate,
                                        env_level: bridge.env_level,
                                        spectral_centroid: bridge.spectral_centroid,
                                        seq_chaos: bridge.seq_chaos,
                                        logic_density: bridge.logic_density,
                                    });
                                org_sample_counters[org_idx] = 0;
                                org_rms_accums[org_idx] = 0.0;
                                org_peaks[org_idx] = 0.0;
                            }
                        }

                        // Run through VoiceBus channel strips (dry path)
                        // Use organisms.len() to include any organisms spawned at runtime
                        let source_count = organisms.len().min(MAX_CHANNELS);
                        let mut bus_out = [0.0f32; 2];
                        voice_bus.process_frame(
                            &sources[..source_count],
                            &mut bus_out,
                        );

                        // ReverbBus: process sends and add wet return
                        // Scale by master_gain (same as tape delay) so reverb sits
                        // at the same level as dry signals from VoiceBus.
                        if let Some(ref mut rb) = reverb_bus_opt {
                            let mut reverb_out = [0.0f32; 2];
                            rb.tick(&sources[..source_count], &mut reverb_out);
                            let mg = master_gain_for_tape.value();
                            bus_out[0] += reverb_out[0] * mg;
                            bus_out[1] += reverb_out[1] * mg;
                        }

                        // TapeDelayBus: process sends and add wet echo return.
                        // Scale by master_gain so the echo sits at the same level as dry
                        // signals from VoiceBus (which already applies master_gain).
                        // Uses dynamic master_gain (tracks active organism count).
                        if let Some(ref mut td) = tape_delay_bus_opt {
                            let mut tape_out = [0.0f32; 2];
                            td.tick(&sources[..source_count], &mut tape_out);
                            let mg = master_gain_for_tape.value();
                            bus_out[0] += tape_out[0] * mg;
                            bus_out[1] += tape_out[1] * mg;
                        }

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

                    // Post-process through MasterBus (crossover + limiters + DC block)
                    master_bus.process(data, channels);
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
            "Audio: {sample_rate}Hz, {channels}ch, f32, {} organisms, VoiceBus {} strips, reverb={}, tape_delay={}",
            org_count,
            voice_bus_handles.strips.len(),
            reverb_bus_handles.is_some(),
            tape_delay_bus_handles.is_some(),
        );

        Some((
            Self {
                _stream: stream,
                sample_rate,
                channels,
                spawn_tx,
                despawn_tx,
            },
            endpoints,
            voice_bus_handles,
            reverb_bus_handles,
            tape_delay_bus_handles,
            meter_rx,
        ))
    }
}

#[cfg(test)]
mod full_chain_tests {
    use crate::audio::gain_staging;
    use crate::audio::master_bus::MasterBus;
    use crate::audio::reverb_bus::ReverbBus;
    use crate::audio::tape_delay_bus::TapeDelayBus;
    use crate::audio::voice_bus::{VoiceBus, MAX_CHANNELS};
    use crate::dsp::organism_dsp::OrganismDsp;
    use crate::organism::dna::OrganismDna;

    const SR: f32 = 44100.0;

    fn load_all_dna() -> Vec<OrganismDna> {
        let files = [
            "assets/dna/dron-alpha.json",
            "assets/dna/hoso-malabar.json",
            "assets/dna/spgl-kepler.json",
            "assets/dna/acid-kinoko.json",
            "assets/dna/kkit-909.json",
            "assets/dna/tblk-dha.json",
        ];
        files
            .iter()
            .filter_map(|path| {
                let json = std::fs::read_to_string(path).ok()?;
                serde_json::from_str(&json).ok()
            })
            .collect()
    }

    #[test]
    fn full_chain_diagnostic() {
        let dnas = load_all_dna();
        assert!(dnas.len() >= 4, "Need at least 4 DNA files, got {}", dnas.len());

        // Build organisms
        let mut organisms: Vec<(String, OrganismDsp)> = Vec::new();
        for dna in &dnas {
            if let Some((org, _handles)) = OrganismDsp::from_dna(dna, SR) {
                organisms.push((dna.name.clone(), org));
            }
        }
        let org_count = organisms.len();
        println!("\n=== FULL CHAIN DIAGNOSTIC ===");
        println!("Organisms: {}", org_count);

        // Build VoiceBus
        let bus_channels: Vec<(&str, f32)> = organisms
            .iter()
            .map(|(name, _)| (name.as_str(), gain_staging::species_gain(name)))
            .collect();
        let (mut voice_bus, vb_handles) = VoiceBus::new(&bus_channels, gain_staging::MASTER_GAIN);

        // Build ReverbBus from first organism with reverb sends
        let reverb_send_dna = dnas.iter().find_map(|d| {
            d.sends.as_ref()?.reverb.as_ref()
        });
        let mut reverb_bus = reverb_send_dna.map(|rdna| {
            let send_levels: Vec<f32> = dnas.iter().map(|d| {
                d.sends.as_ref()
                    .and_then(|s| s.reverb.as_ref())
                    .map(|r| r.send)
                    .unwrap_or(0.0)
            }).collect();
            let (bus, _handles) = ReverbBus::new(rdna, &send_levels, SR);
            bus
        });

        // Build TapeDelayBus
        let tape_send_dna = dnas.iter().find_map(|d| {
            d.sends.as_ref()?.tape_delay.as_ref()
        });
        let mut tape_bus = tape_send_dna.map(|tdna| {
            let send_levels: Vec<f32> = dnas.iter().map(|d| {
                d.sends.as_ref()
                    .and_then(|s| s.tape_delay.as_ref())
                    .map(|td| td.send)
                    .unwrap_or(0.0)
            }).collect();
            let (bus, _handles) = TapeDelayBus::new(tdna, &send_levels, SR);
            bus
        });

        // Master bus
        let mut master_bus = MasterBus::new(SR);
        let master_gain = vb_handles.master_gain.clone();

        // Simulate what app.rs does: set dynamic master gain based on organism count
        let dynamic = gain_staging::dynamic_master_gain(org_count);
        master_gain.set(dynamic);

        // Run for 3 seconds
        let total_frames = (SR * 3.0) as usize;
        let channels = 2u16;

        // Per-organism tracking
        let mut org_peaks = vec![0.0f32; org_count];
        let mut org_rms_acc = vec![0.0f32; org_count];

        // Bus stage tracking
        let mut dry_peak = 0.0f32;
        let mut dry_rms_acc = 0.0f32;
        let mut reverb_peak = 0.0f32;
        let mut tape_peak = 0.0f32;
        let mut pre_master_peak = 0.0f32;
        let mut post_master_peak = 0.0f32;
        let mut post_master_rms_acc = 0.0f32;

        // Click detection
        let mut prev_l = 0.0f32;
        let mut click_count = 0u32;
        let mut max_jump = 0.0f32;
        let mut nan_count = 0u32;
        let mut inf_count = 0u32;
        let mut clamp_count = 0u32;

        let chunk_size = 512;
        let mut data = vec![0.0f32; chunk_size * channels as usize];

        for chunk_start in (0..total_frames).step_by(chunk_size) {
            let frames_this_chunk = (total_frames - chunk_start).min(chunk_size);

            for frame in 0..frames_this_chunk {
                let mut sources = [[0.0f32; 2]; MAX_CHANNELS];

                for (org_idx, (_name, org)) in organisms.iter_mut().enumerate() {
                    let mut out = [0.0f32; 2];
                    org.tick(&mut out);
                    sources[org_idx] = out;

                    org_peaks[org_idx] = org_peaks[org_idx].max(out[0].abs()).max(out[1].abs());
                    let mono = (out[0] + out[1]) * 0.5;
                    org_rms_acc[org_idx] += mono * mono;
                }

                let mut bus_out = [0.0f32; 2];
                voice_bus.process_frame(&sources[..org_count], &mut bus_out);

                let dry_mono = (bus_out[0] + bus_out[1]) * 0.5;
                dry_peak = dry_peak.max(bus_out[0].abs()).max(bus_out[1].abs());
                dry_rms_acc += dry_mono * dry_mono;

                if let Some(ref mut rb) = reverb_bus {
                    let mut reverb_out = [0.0f32; 2];
                    rb.tick(&sources[..org_count], &mut reverb_out);
                    let mg = master_gain.value();
                    reverb_peak = reverb_peak.max((reverb_out[0] * mg).abs()).max((reverb_out[1] * mg).abs());
                    bus_out[0] += reverb_out[0] * mg;
                    bus_out[1] += reverb_out[1] * mg;
                }

                if let Some(ref mut td) = tape_bus {
                    let mut tape_out = [0.0f32; 2];
                    td.tick(&sources[..org_count], &mut tape_out);
                    let mg = master_gain.value();
                    tape_peak = tape_peak.max((tape_out[0] * mg).abs()).max((tape_out[1] * mg).abs());
                    bus_out[0] += tape_out[0] * mg;
                    bus_out[1] += tape_out[1] * mg;
                }

                pre_master_peak = pre_master_peak.max(bus_out[0].abs()).max(bus_out[1].abs());

                let base = frame * channels as usize;
                data[base] = bus_out[0];
                data[base + 1] = bus_out[1];
            }

            master_bus.process(&mut data[..frames_this_chunk * channels as usize], channels);

            for frame in 0..frames_this_chunk {
                let base = frame * channels as usize;
                let l = data[base];
                let r = data[base + 1];

                if l.is_nan() || r.is_nan() { nan_count += 1; }
                if l.is_infinite() || r.is_infinite() { inf_count += 1; }

                post_master_peak = post_master_peak.max(l.abs()).max(r.abs());
                let mono = (l + r) * 0.5;
                post_master_rms_acc += mono * mono;

                if l.abs() >= 0.999 || r.abs() >= 0.999 { clamp_count += 1; }

                let jump = (l - prev_l).abs();
                max_jump = max_jump.max(jump);
                if jump > 0.3 && chunk_start + frame > 500 {
                    click_count += 1;
                }
                prev_l = l;
            }
        }

        let total_samples = total_frames as f32;

        println!("\n--- Per-Organism Output (raw, pre-VoiceBus) ---");
        for (i, (name, _)) in organisms.iter().enumerate() {
            let rms = (org_rms_acc[i] / total_samples).sqrt();
            let gain = gain_staging::species_gain(name);
            println!(
                "  {:<16} peak={:.4}  rms={:.4}  species_gain={:.2}",
                name, org_peaks[i], rms, gain,
            );
        }

        let dry_rms = (dry_rms_acc / total_samples).sqrt();
        let post_rms = (post_master_rms_acc / total_samples).sqrt();

        println!("\n--- Bus Stages ---");
        println!("  VoiceBus dry:    peak={:.4}  rms={:.4}", dry_peak, dry_rms);
        println!("  Reverb return:   peak={:.4}", reverb_peak);
        println!("  Tape return:     peak={:.4}", tape_peak);
        println!("  Pre-MasterBus:   peak={:.4}", pre_master_peak);
        println!("  Post-MasterBus:  peak={:.4}  rms={:.4}", post_master_peak, post_rms);

        println!("\n--- Signal Quality ---");
        println!("  Clicks (>0.3 jump):  {}", click_count);
        println!("  Max jump:            {:.4}", max_jump);
        println!("  Clamp hits (±1.0):   {}", clamp_count);
        println!("  NaN samples:         {}", nan_count);
        println!("  Inf samples:         {}", inf_count);
        println!("  Master gain:         {:.4}", master_gain.value());
        println!();

        // Hard assertions
        assert_eq!(nan_count, 0, "NaN detected in output!");
        assert_eq!(inf_count, 0, "Inf detected in output!");
        assert!(
            post_master_peak <= 1.001,
            "Post-master peak exceeds ±1.0: {:.4}",
            post_master_peak
        );
        assert!(
            post_rms > 0.001,
            "Output RMS too low — signal is inaudible: {:.6}",
            post_rms
        );
    }
}
