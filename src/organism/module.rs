use std::any::Any;
use std::sync::Arc;

use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::organism_dsp::SharedHandles;
use crate::dsp::shared::Shared;
use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, ModuleId, PortId, SignalError};
use crate::substrate::channel::{Receiver, Sender};

use super::dna::OrganismDna;
use super::sim::OrganismId;

/// How an import modulation combines with the base value.
#[derive(Debug, Clone, Copy)]
enum ModulationMode {
    Add,
    Multiply,
}

/// Runtime state for a DNA-defined parameter export (CV output jack).
struct ParamExportState {
    port_id: PortId,
    /// What to read: "rms", "rhythm_density", or "cell{N}.{param}"
    source: String,
    /// If source is a cell param, the cached Shared handle
    source_handle: Option<Shared>,
}

/// Runtime state for a DNA-defined parameter import (CV input jack).
struct ParamImportState {
    port_id: PortId,
    target_handle: Shared,
    base_value: f32,
    gain: f32,
    mode: ModulationMode,
    range: (f32, f32),
}

/// Organism module — bridges the SeedReactor (60Hz control thread) to the
/// OrganismDsp (44.1kHz audio thread) via SharedHandles and ring buffers.
///
/// Each OrganismModule wraps one organism: it receives infrastructure signals
/// (pitch, rms) and maps them to Shared param handles that the audio-thread
/// OrganismDsp reads lock-free. It emits analysis signals (rms, peak) back
/// into the affinity graph for Hebbian learning.
///
/// Tier = Organism: gets AffinityGraph routing with learned edge weights.
pub struct OrganismModule {
    schema: ModuleSchema,
    dna: OrganismDna,
    shared_handles: SharedHandles,
    analysis_rx: Receiver<DspAnalysis>,
    cmd_tx: Sender<DspCommand>,

    // Cached analysis from audio thread
    current_rms: f32,
    current_peak: f32,

    // Dialogue state (S19)
    last_actual_pitch: f32,
    accent_level: f32,
    last_gate_time: f32,
    tick_counter: u64,

    // OrganismState identity (for visual updates in app.rs)
    organism_id: OrganismId,

    // Port IDs (existing)
    pitch_hz_port: PortId,
    rms_in_port: PortId,
    rms_out_port: PortId,
    peak_out_port: PortId,
    is_active_port: PortId,

    // Port IDs (S19 dialogue)
    gate_port: PortId,
    accent_port: PortId,
    actual_pitch_port: PortId,
    rhythm_density_port: PortId,

    // Port IDs (S33 scale/rhythm bridge)
    gravity_weights_port: PortId,
    beat_phase_port: PortId,
    beat_trigger_port: PortId,
    gamaka_config_port: PortId,

    // Port IDs (cell-level signal bridge)
    seq_pitch_port: PortId,
    seq_gate_port: PortId,
    env_level_port: PortId,
    spectral_centroid_port: PortId,

    // S33 cached bridge state
    current_gravity_weights: Option<Arc<Vec<f32>>>,
    current_beat_phase: f32,

    // Cached cell-level bridge data from audio thread
    current_seq_pitch_hz: f32,
    current_seq_gate: bool,
    current_env_level: f32,
    current_spectral_centroid: f32,

    // Inter-organism interaction (DNA-defined patch points)
    param_exports: Vec<ParamExportState>,
    param_imports: Vec<ParamImportState>,

    // Lifecycle
    reactor_id: Option<ModuleId>,
    shutdown_initiated: bool,
}

impl OrganismModule {
    /// Create an OrganismModule from DNA, shared handles, and channel endpoints.
    ///
    /// `shared_handles`, `analysis_rx`, and `cmd_tx` come from AudioSubstrate after
    /// building the organism's DSP graph. `organism_id` is the OrganismState ID in
    /// the registry.
    pub fn new(
        dna: OrganismDna,
        shared_handles: SharedHandles,
        analysis_rx: Receiver<DspAnalysis>,
        cmd_tx: Sender<DspCommand>,
        organism_id: OrganismId,
    ) -> Self {
        // Input ports
        let pitch_hz_in = Port::input("pitch_hz", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Pitch frequency in Hz — drives voice cell frequencies");
        let rms_in = Port::input("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("External RMS level — modulates organism arousal");
        let gate_in = Port::input("gate", SignalType::Trigger, PortRate::Block)
            .with_description("Trigger to fire NoteOn");
        let accent_in = Port::input("accent", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Accent level (0.0=normal, 1.0=accent)");

        // S33 bridge input ports
        let gravity_weights_in =
            Port::input("gravity_weights", SignalType::Pattern, PortRate::Block)
                .with_description("Per-degree pitch gravity from RagaModule");
        let beat_phase_in = Port::input("beat_phase", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Tala cycle phase [0,1]");
        let beat_trigger_in =
            Port::input("beat_trigger", SignalType::Trigger, PortRate::Event)
                .with_description("Beat trigger from TalaModule");
        let gamaka_config_in =
            Port::input("gamaka_config", SignalType::Pattern, PortRate::Block)
                .with_description("Gamaka params [slide_ms, vib_depth_cents, vib_rate_hz]");

        // Output ports
        let rms_out = Port::output("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Organism audio RMS level");
        let peak_out = Port::output("peak", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Organism audio peak level");
        let is_active_out = Port::output("is_active", SignalType::Bool, PortRate::Block)
            .with_description("True when organism is producing sound");
        let actual_pitch_out = Port::output("actual_pitch", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("What the organism actually played (Hz)");
        let rhythm_density_out =
            Port::output("rhythm_density", SignalType::Float, PortRate::Block)
                .with_range(0.0, 10.0)
                .with_description("Activity level (triggers per second)");

        // Cell-level signal bridge outputs
        let seq_pitch_out = Port::output("seq_pitch", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Internal sequencer pitch (Hz)");
        let seq_gate_out = Port::output("seq_gate", SignalType::Bool, PortRate::Block)
            .with_description("Internal sequencer gate state");
        let env_level_out = Port::output("env_level", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Internal envelope level");
        let spectral_centroid_out = Port::output("spectral_centroid", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Energy-weighted oscillator frequency center");

        let pitch_hz_port = pitch_hz_in.id;
        let rms_in_port = rms_in.id;
        let gate_port = gate_in.id;
        let accent_port = accent_in.id;
        let gravity_weights_port = gravity_weights_in.id;
        let beat_phase_port = beat_phase_in.id;
        let beat_trigger_port = beat_trigger_in.id;
        let gamaka_config_port = gamaka_config_in.id;
        let rms_out_port = rms_out.id;
        let peak_out_port = peak_out.id;
        let is_active_port = is_active_out.id;
        let actual_pitch_port = actual_pitch_out.id;
        let rhythm_density_port = rhythm_density_out.id;
        let seq_pitch_port = seq_pitch_out.id;
        let seq_gate_port = seq_gate_out.id;
        let env_level_port = env_level_out.id;
        let spectral_centroid_port = spectral_centroid_out.id;

        let module_name = format!("organism:{}", dna.name);
        let mut schema = ModuleSchema::new(&module_name, ModuleCategory::Output)
            .with_description(&format!(
                "{} organism — species: {} (fidelity: {:.1})",
                dna.name, dna.species, dna.fidelity
            ))
            .with_tier(ModuleTier::Organism)
            .with_input(pitch_hz_in)
            .with_input(rms_in)
            .with_input(gate_in)
            .with_input(accent_in)
            .with_input(gravity_weights_in)
            .with_input(beat_phase_in)
            .with_input(beat_trigger_in)
            .with_input(gamaka_config_in)
            .with_output(rms_out)
            .with_output(peak_out)
            .with_output(is_active_out)
            .with_output(actual_pitch_out)
            .with_output(rhythm_density_out)
            .with_output(seq_pitch_out)
            .with_output(seq_gate_out)
            .with_output(env_level_out)
            .with_output(spectral_centroid_out)
            .with_side_effect("audio_output")
            .with_initial_emotion(dna.emotion.base_arousal, dna.emotion.base_valence);

        // Build interaction param ports from DNA
        let mut param_exports = Vec::new();
        let mut param_imports = Vec::new();

        if let Some(ref ip) = dna.interaction_params {
            for export in &ip.exports {
                let port_name = format!("x:{}", export.name);
                let port = Port::output(&port_name, SignalType::Float, PortRate::Block)
                    .with_range(0.0, 1.0)
                    .with_description(&format!("Param export: {} (src: {})", export.name, export.source));
                let port_id = port.id;
                schema = schema.with_output(port);

                let source_handle = shared_handles.get(&export.source).cloned();
                param_exports.push(ParamExportState {
                    port_id,
                    source: export.source.clone(),
                    source_handle,
                });
            }

            for import in &ip.imports {
                let port_name = format!("x:{}", import.name);
                let port = Port::input(&port_name, SignalType::Float, PortRate::Block)
                    .with_range(0.0, 1.0)
                    .with_description(&format!("Param import: {} (tgt: {})", import.name, import.target));
                let port_id = port.id;
                schema = schema.with_input(port);

                let mode = if import.mode == "Multiply" {
                    ModulationMode::Multiply
                } else {
                    ModulationMode::Add
                };

                if let Some(handle) = shared_handles.get(&import.target) {
                    let base_value = handle.value();
                    param_imports.push(ParamImportState {
                        port_id,
                        target_handle: handle.clone(),
                        base_value,
                        gain: import.gain,
                        mode,
                        range: import.range,
                    });
                } else {
                    log::warn!(
                        "[organism:{}] interaction import '{}' target '{}' not found in shared handles",
                        dna.name, import.name, import.target
                    );
                }
            }
        }

        Self {
            schema,
            dna,
            shared_handles,
            analysis_rx,
            cmd_tx,
            current_rms: 0.0,
            current_peak: 0.0,
            last_actual_pitch: 261.63, // C4 default
            accent_level: 0.0,
            last_gate_time: 0.0,
            tick_counter: 0,
            organism_id,
            pitch_hz_port,
            rms_in_port,
            rms_out_port,
            peak_out_port,
            is_active_port,
            gate_port,
            accent_port,
            gravity_weights_port,
            beat_phase_port,
            beat_trigger_port,
            gamaka_config_port,
            current_gravity_weights: None,
            current_beat_phase: 0.0,
            actual_pitch_port,
            rhythm_density_port,
            seq_pitch_port,
            seq_gate_port,
            env_level_port,
            spectral_centroid_port,
            current_seq_pitch_hz: 0.0,
            current_seq_gate: false,
            current_env_level: 0.0,
            current_spectral_centroid: 0.0,
            param_exports,
            param_imports,
            reactor_id: None,
            shutdown_initiated: false,
        }
    }

    /// Get the organism's DNA.
    pub fn dna(&self) -> &OrganismDna {
        &self.dna
    }

    /// Get the OrganismState ID in the registry.
    pub fn organism_id(&self) -> OrganismId {
        self.organism_id
    }

    /// Current RMS from the audio thread.
    pub fn current_rms(&self) -> f32 {
        self.current_rms
    }

    /// Organism species identifier (e.g., "dron", "acid", "kkit").
    pub fn species(&self) -> &str {
        &self.dna.species
    }

    /// Audio energy (0-1 RMS) — used by BioField renderer for live signal band.
    pub fn audio_rms(&self) -> f32 {
        self.current_rms
    }

    /// Current peak from the audio thread.
    pub fn current_peak(&self) -> f32 {
        self.current_peak
    }

    /// Send a DspCommand to the audio thread (lock-free, fire-and-forget).
    pub fn send_command(&mut self, cmd: DspCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }

    /// Map pitch_hz to the appropriate shared handles for this species.
    fn apply_pitch_hz(&self, hz: f32) {
        let hz = hz.clamp(20.0, 20000.0);
        match self.dna.species.as_str() {
            "tblk" => {
                // StrikeVoice membrane frequency
                if let Some(h) = self.shared_handles.get("cell1.membrane_freq") {
                    h.set(hz);
                }
            }
            "dron" => {
                // HarmonicBed root frequency
                if let Some(h) = self.shared_handles.get("cell0.root_hz") {
                    h.set(hz);
                }
            }
            "melo" => {
                // TimbreVoice frequency
                if let Some(h) = self.shared_handles.get("cell1.freq") {
                    h.set(hz);
                }
            }
            _ => {
                // Generic: set any "freq" handle on any cell
                for (key, handle) in &self.shared_handles {
                    if key.ends_with(".freq") || key.ends_with(".root_hz") {
                        handle.set(hz);
                    }
                }
            }
        }
    }

    /// Species-specific pitch personality transform (S19).
    ///
    /// Blends external prompted pitch with organism's internal intent
    /// based on DNA fidelity and affinity weight. Each species responds
    /// differently to prompts.
    fn personality_transform_pitch(&mut self, prompted_hz: f32) -> f32 {
        let fidelity = self.dna.fidelity.clamp(0.0, 1.0);
        // TODO: Get actual affinity_weight from graph edge (placeholder: 1.0)
        let affinity_weight = 1.0;
        let blend = fidelity * affinity_weight;

        // For now, internal pitch intent is just the last pitch
        // (will be replaced by seq_cell/func_gen_cell outputs in S20+)
        let internal_hz = self.last_actual_pitch;

        match self.dna.species.as_str() {
            "dron" => {
                // Slowly slew toward prompted pitch (simplification: direct lerp)
                // TODO S20: replace with actual slew_cell output
                internal_hz * (1.0 - blend * 0.1) + prompted_hz * blend * 0.1
            }
            "hoso" => {
                // Rigid follower — direct blend
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "spgl" => {
                // Barely acknowledges — very slow blend
                internal_hz * (1.0 - blend * 0.01) + prompted_hz * blend * 0.01
            }
            "acid" => {
                // Follows tightly
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "tblk" => {
                // Follows but quantizes to nearest membrane mode (simplified: direct blend)
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "kkit" => {
                // Ignores pitch entirely
                internal_hz
            }
            _ => {
                // Default: moderate following
                internal_hz * (1.0 - blend * 0.5) + prompted_hz * blend * 0.5
            }
        }
    }
}

/// Quantize a Hz value to the nearest weighted scale degree.
/// (Now used only in tests — audio-thread quantization uses quantize_to_scale_fast in organism_dsp.rs)
///
/// `gravity` is a 12-element array of per-semitone weights [0,1].
/// Degrees with weight < 0.1 are skipped. Higher weight = stronger pull.
/// `blend` controls how much the output follows the quantized value vs raw.
#[cfg(test)]
fn quantize_to_scale(raw_hz: f32, gravity: &[f32], blend: f32) -> f32 {
    if gravity.is_empty() || blend < 0.01 {
        return raw_hz;
    }
    let midi = 12.0 * (raw_hz / 440.0).log2() + 69.0;
    let octave = (midi / 12.0).floor();
    let degree = midi - octave * 12.0;

    let mut best_degree = degree;
    let mut best_dist = f32::MAX;
    for (i, &weight) in gravity.iter().enumerate().take(12) {
        if weight < 0.1 {
            continue;
        }
        let d = i as f32;
        let dist = (degree - d).abs().min(12.0 - (degree - d).abs());
        let weighted_dist = dist / weight;
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_degree = d;
        }
    }

    let quantized_midi = octave * 12.0 + best_degree;
    let quantized_hz = 440.0 * 2.0f32.powf((quantized_midi - 69.0) / 12.0);
    // Blend between raw and quantized
    raw_hz * (1.0 - blend) + quantized_hz * blend
}

impl ModuleCore for OrganismModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.rms_out_port, Signal::Float(self.current_rms)));
        buffer.push((self.peak_out_port, Signal::Float(self.current_peak)));
        buffer.push((
            self.is_active_port,
            Signal::Bool(self.current_rms > 0.001),
        ));
        buffer.push((
            self.actual_pitch_port,
            Signal::Float(self.last_actual_pitch),
        ));

        // Calculate rhythm density (gates per second) from recent gate activity
        let decay: f32 = 0.95;
        let rhythm_density = if self.last_gate_time < 5.0 {
            (5.0 - self.last_gate_time).min(1.0)
        } else {
            0.0
        } * decay.powf(self.last_gate_time);
        buffer.push((
            self.rhythm_density_port,
            Signal::Float(rhythm_density),
        ));

        // Cell-level signal bridge outputs — only emit when bridge data is populated.
        // The audio callback currently uses DspAnalysis::new(rms, peak) which zeroes
        // bridge fields. Emitting 0.0 on pitch-range ports (seq_pitch, spectral_centroid)
        // creates edges to pitch_hz inputs and drags organism pitch to subsonic range.
        if self.current_seq_pitch_hz > 0.0 {
            buffer.push((self.seq_pitch_port, Signal::Float(self.current_seq_pitch_hz)));
        }
        if self.current_seq_gate {
            buffer.push((self.seq_gate_port, Signal::Bool(self.current_seq_gate)));
        }
        if self.current_env_level > 0.0 {
            buffer.push((self.env_level_port, Signal::Float(self.current_env_level)));
        }
        if self.current_spectral_centroid > 0.0 {
            buffer.push((self.spectral_centroid_port, Signal::Float(self.current_spectral_centroid)));
        }

        // Emit interaction param exports
        for export in &self.param_exports {
            let value = match export.source.as_str() {
                "rms" => self.current_rms,
                "rhythm_density" => rhythm_density,
                _ => {
                    // cell{N}.{param} — read from cached Shared handle
                    export.source_handle.as_ref().map_or(0.0, |h| h.value())
                }
            };
            buffer.push((export.port_id, Signal::Float(value)));
        }
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        log::debug!(
            "[organism:{}] receive_signal port={:?} type={:?}",
            self.dna.name, port, signal.signal_type()
        );

        if port == self.pitch_hz_port {
            if let Signal::Float(hz) = signal {
                // Apply personality transform
                let actual_hz = self.personality_transform_pitch(hz);
                self.last_actual_pitch = actual_hz;
                self.apply_pitch_hz(actual_hz);
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.gate_port {
            if let Signal::Trigger = signal {
                // Send NoteOn command to audio thread
                let velocity = 0.5 + self.accent_level * 0.5; // 0.5–1.0 range
                let _ = self.cmd_tx.try_send(DspCommand::NoteOn {
                    freq: self.last_actual_pitch,
                    velocity,
                });
                self.last_gate_time = 0.0; // reset gate timer
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        if port == self.accent_port {
            if let Signal::Float(accent) = signal {
                self.accent_level = accent.clamp(0.0, 1.0);
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.rms_in_port {
            if let Signal::Float(_rms) = signal {
                // Store for emotion/arousal modulation (future use)
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        // S33: Scale/rhythm bridge handlers
        if port == self.gravity_weights_port {
            if let Signal::Pattern(weights) = signal {
                self.current_gravity_weights = Some(weights.clone());
                // Send weights to audio thread for per-sample quantization
                let mut w = [0.0f32; 12];
                for (i, &v) in weights.iter().enumerate().take(12) {
                    w[i] = v;
                }
                let blend = self.dna.scale_affinity * self.dna.fidelity;
                let _ = self.cmd_tx.try_send(DspCommand::SetScaleWeights(w, blend));
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Pattern,
                got: signal.signal_type(),
            });
        }

        if port == self.beat_phase_port {
            if let Signal::Float(phase) = signal {
                self.current_beat_phase = phase;
                // Soft sync nudge applied in tick()
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Float,
                got: signal.signal_type(),
            });
        }

        if port == self.beat_trigger_port {
            if let Signal::Trigger = signal {
                if self.dna.rhythm_sync == "hard" && self.dna.rhythm_affinity > 0.01 {
                    // Hard sync: reset seq_cell phase to downbeat.
                    // NudgePhase is additive, -1.0 wraps to 0.0 via rem_euclid(1.0).
                    let _ = self.cmd_tx.try_send(DspCommand::NudgePhase(-1.0));
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Trigger,
                got: signal.signal_type(),
            });
        }

        if port == self.gamaka_config_port {
            if let Signal::Pattern(config) = signal {
                if config.len() >= 3 {
                    let blend = self.dna.scale_affinity * self.dna.fidelity;
                    // slide_ms → seconds, scaled by affinity
                    let slide_s = (config[0] / 1000.0 * blend).max(0.001);
                    let vib_depth = config[1] * blend;
                    let vib_rate = config[2];
                    let _ = self.cmd_tx.try_send(DspCommand::SetSlewRate(slide_s, slide_s));
                    let _ = self.cmd_tx.try_send(DspCommand::SetLfoParams(vib_rate, vib_depth));
                }
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Pattern,
                got: signal.signal_type(),
            });
        }

        // Check interaction param imports
        for imp in &self.param_imports {
            if port == imp.port_id {
                if let Signal::Float(v) = signal {
                    let modulated = match imp.mode {
                        ModulationMode::Add => imp.base_value + v * imp.gain,
                        ModulationMode::Multiply => imp.base_value * (1.0 + v * imp.gain),
                    };
                    let clamped = modulated.clamp(imp.range.0, imp.range.1);
                    imp.target_handle.set(clamped);
                    return Ok(());
                }
                return Err(SignalError::WrongType {
                    expected: SignalType::Float,
                    got: signal.signal_type(),
                });
            }
        }

        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, dt: f32) {
        // Drain analysis from audio thread (including cell-level bridge data)
        while let Some(analysis) = self.analysis_rx.try_recv() {
            self.current_rms = analysis.rms;
            self.current_peak = analysis.peak;
            self.current_seq_pitch_hz = analysis.seq_pitch_hz;
            self.current_seq_gate = analysis.seq_gate;
            self.current_env_level = analysis.env_level;
            self.current_spectral_centroid = analysis.spectral_centroid;
        }

        // S33: Soft sync — nudge seq_cell phase toward tala beat phase
        if self.dna.rhythm_sync == "soft" && self.dna.rhythm_affinity > 0.01 {
            // Nudge near beat boundaries (phase near 0 or 1)
            if self.current_beat_phase < 0.1 || self.current_beat_phase > 0.9 {
                let nudge = (self.current_beat_phase - 0.5).signum()
                    * 0.02
                    * self.dna.rhythm_affinity;
                let _ = self.cmd_tx.try_send(DspCommand::NudgePhase(nudge));
            }
        }

        // Update rhythm tracking
        self.last_gate_time += dt;
        self.tick_counter += 1;
    }

    #[allow(deprecated)]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[allow(deprecated)]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_register(&mut self, id: ModuleId) {
        self.reactor_id = Some(id);
    }

    fn on_unregister(&mut self) -> bool {
        if !self.shutdown_initiated {
            // First call: send Panic to release all notes
            let _ = self.cmd_tx.try_send(DspCommand::Panic);
            self.shutdown_initiated = true;
            return false; // defer removal — let audio decay
        }
        // Subsequent calls: ready when audio has decayed
        self.current_rms < 0.001
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::command::DspAnalysis;
    use crate::substrate::channel;
    use std::collections::HashMap;

    fn make_test_module() -> (
        OrganismModule,
        crate::substrate::channel::Sender<DspAnalysis>,
        crate::substrate::channel::Receiver<DspCommand>,
    ) {
        let dna = OrganismDna {
            name: "test-org".into(),
            species: "dron".into(),
            active: true,
            seed: 42,
            version: 1,
            cells: vec![],
            cell_wiring: vec![],
            body: super::super::dna::BodyDna::default(),
            render: super::super::dna::RenderDna::default(),
            physics: super::super::dna::PhysicsDna::default(),
            emotion: super::super::dna::EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
        };

        let mut handles = HashMap::new();
        handles.insert("cell0.root_hz".into(), Shared::new(110.0));
        handles.insert("cell0.cutoff".into(), Shared::new(800.0));

        let (analysis_tx, analysis_rx) = channel::channel::<DspAnalysis>(32);
        let (cmd_tx, cmd_rx) = channel::channel::<DspCommand>(32);

        let module = OrganismModule::new(dna, handles, analysis_rx, cmd_tx, 0);
        (module, analysis_tx, cmd_rx)
    }

    #[test]
    fn schema_has_correct_ports() {
        let (module, _, _) = make_test_module();
        let schema = module.schema();

        assert_eq!(schema.tier, ModuleTier::Organism);
        assert_eq!(schema.inputs.len(), 8); // pitch_hz, rms, gate, accent + gravity_weights, beat_phase, beat_trigger, gamaka_config
        assert_eq!(schema.outputs.len(), 9); // rms, peak, is_active, actual_pitch, rhythm_density, seq_pitch, seq_gate, env_level, spectral_centroid
        assert!(schema.input("pitch_hz").is_some());
        assert!(schema.input("rms").is_some());
        assert!(schema.input("gate").is_some());
        assert!(schema.input("accent").is_some());
        assert!(schema.input("gravity_weights").is_some());
        assert!(schema.input("beat_phase").is_some());
        assert!(schema.input("beat_trigger").is_some());
        assert!(schema.input("gamaka_config").is_some());
        assert!(schema.output("rms").is_some());
        assert!(schema.output("peak").is_some());
        assert!(schema.output("is_active").is_some());
        assert!(schema.output("actual_pitch").is_some());
        assert!(schema.output("rhythm_density").is_some());
        assert!(schema.side_effects.contains(&"audio_output".to_string()));
    }

    #[test]
    fn receive_pitch_hz_applies_personality_transform() {
        let (mut module, _, _) = make_test_module();

        // DRON species with fidelity=0.3 will slew slowly toward target
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(220.0))
            .unwrap();

        let root_hz = module.shared_handles.get("cell0.root_hz").unwrap();
        // DRON slews slowly, so it won't jump directly to 220, but will move toward it
        let val = root_hz.value();
        assert!(
            val > 200.0 && val < 300.0,
            "pitch should be in reasonable range after transform, got {}",
            val
        );
    }

    #[test]
    fn receive_rms_accepts_float() {
        let (mut module, _, _) = make_test_module();
        let result = module.receive_signal(module.rms_in_port, Signal::Float(0.5));
        assert!(result.is_ok());
    }

    #[test]
    fn reject_wrong_type() {
        let (mut module, _, _) = make_test_module();
        let result = module.receive_signal(module.pitch_hz_port, Signal::Trigger);
        assert!(matches!(result, Err(SignalError::WrongType { .. })));
    }

    #[test]
    fn reject_unknown_port() {
        let (mut module, _, _) = make_test_module();
        let result = module.receive_signal(PortId(99999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn tick_drains_analysis() {
        let (mut module, mut analysis_tx, _) = make_test_module();

        analysis_tx
            .try_send(DspAnalysis::new(0.42, 0.85))
            .unwrap();

        module.tick(1.0 / 60.0);

        assert!((module.current_rms - 0.42).abs() < 1e-6);
        assert!((module.current_peak - 0.85).abs() < 1e-6);
    }

    #[test]
    fn emit_signals_returns_5_when_bridge_unpopulated() {
        let (mut module, _, _) = make_test_module();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        // rms, peak, is_active, actual_pitch, rhythm_density
        // (bridge ports suppressed when zeroed — seq_pitch, seq_gate, env_level, spectral_centroid)
        assert_eq!(buffer.len(), 5);
    }

    #[test]
    fn emit_signals_returns_9_when_bridge_populated() {
        let (mut module, mut analysis_tx, _) = make_test_module();
        // Send analysis with populated bridge data
        let analysis = DspAnalysis {
            rms: 0.5,
            peak: 0.8,
            seq_pitch_hz: 440.0,
            seq_gate: true,
            env_level: 0.7,
            spectral_centroid: 800.0,
        };
        analysis_tx.try_send(analysis).unwrap();
        module.tick(1.0 / 60.0);
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        // All 9 ports emit when bridge data is non-zero
        assert_eq!(buffer.len(), 9);
    }

    #[test]
    fn is_active_reflects_rms() {
        let (mut module, mut analysis_tx, _) = make_test_module();

        // Silent
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        let is_active = buffer.iter().find(|(port, _)| *port == module.is_active_port);
        assert!(matches!(is_active, Some((_, Signal::Bool(false)))));

        // Active
        analysis_tx
            .try_send(DspAnalysis::new(0.1, 0.2))
            .unwrap();
        module.tick(1.0 / 60.0);
        buffer.clear();
        module.emit_signals(&mut buffer);
        let is_active = buffer.iter().find(|(port, _)| *port == module.is_active_port);
        assert!(matches!(is_active, Some((_, Signal::Bool(true)))));
    }

    #[test]
    fn pitch_hz_applies_transform_before_clamp() {
        let (mut module, _, _) = make_test_module();

        // Send very low pitch - personality transform applies first, then clamping
        // DRON slews slowly, so it won't jump to 5.0, but blend toward it
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(5.0))
            .unwrap();
        let root_hz = module.shared_handles.get("cell0.root_hz").unwrap();
        // After transform, should still be in valid range
        assert!(
            root_hz.value() >= 20.0,
            "after transform and clamp, should be >= 20 Hz, got {}",
            root_hz.value()
        );
    }

    // S19 Dialogue Tests

    #[test]
    fn gate_triggers_noteon() {
        let (mut module, _, mut cmd_rx) = make_test_module();

        // Set a test pitch and accent
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(440.0))
            .unwrap();
        module
            .receive_signal(module.accent_port, Signal::Float(1.0))
            .unwrap();

        // Trigger gate
        module
            .receive_signal(module.gate_port, Signal::Trigger)
            .unwrap();

        // Check that NoteOn was sent
        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_some(), "should send NoteOn command");
        if let Some(DspCommand::NoteOn { freq, velocity }) = cmd {
            assert!(freq > 0.0, "NoteOn freq should be positive");
            assert!(velocity > 0.5, "velocity should be elevated with accent=1.0");
        } else {
            panic!("expected NoteOn command");
        }
    }

    #[test]
    fn accent_modulates_velocity() {
        let (mut module, _, mut cmd_rx) = make_test_module();

        // Low accent
        module
            .receive_signal(module.accent_port, Signal::Float(0.0))
            .unwrap();
        module
            .receive_signal(module.gate_port, Signal::Trigger)
            .unwrap();

        if let Some(DspCommand::NoteOn { velocity, .. }) = cmd_rx.try_recv() {
            assert!(
                velocity <= 0.6,
                "low accent should give low velocity, got {}",
                velocity
            );
        }

        // High accent
        module
            .receive_signal(module.accent_port, Signal::Float(1.0))
            .unwrap();
        module
            .receive_signal(module.gate_port, Signal::Trigger)
            .unwrap();

        if let Some(DspCommand::NoteOn { velocity, .. }) = cmd_rx.try_recv() {
            assert!(
                velocity >= 0.9,
                "high accent should give high velocity, got {}",
                velocity
            );
        }
    }

    #[test]
    fn emits_actual_pitch() {
        let (mut module, _, _) = make_test_module();

        module
            .receive_signal(module.pitch_hz_port, Signal::Float(440.0))
            .unwrap();

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let actual_pitch = buffer
            .iter()
            .find(|(port, _)| *port == module.actual_pitch_port)
            .map(|(_, sig)| {
                if let Signal::Float(v) = sig {
                    *v
                } else {
                    0.0
                }
            });

        assert!(actual_pitch.is_some(), "should emit actual_pitch");
        assert!(
            actual_pitch.unwrap() > 0.0,
            "actual_pitch should be positive"
        );
    }

    #[test]
    fn emits_rhythm_density() {
        let (mut module, _, _) = make_test_module();

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let rhythm_density = buffer
            .iter()
            .find(|(port, _)| *port == module.rhythm_density_port)
            .map(|(_, sig)| {
                if let Signal::Float(v) = sig {
                    *v
                } else {
                    -1.0
                }
            });

        assert!(rhythm_density.is_some(), "should emit rhythm_density");
        assert!(
            rhythm_density.unwrap() >= 0.0,
            "rhythm_density should be non-negative"
        );
    }

    #[test]
    fn dron_slews_pitch() {
        let (mut module, _, _) = make_test_module();
        // module species is "dron" with fidelity=0.3

        // Start at a known pitch
        module.last_actual_pitch = 220.0;

        // Send a new pitch (should slew slowly)
        let new_pitch = module.personality_transform_pitch(440.0);

        // DRON should not jump directly to 440, should be somewhere between 220 and 440
        assert!(
            new_pitch > 220.0 && new_pitch < 440.0,
            "DRON should slew, not jump: got {}",
            new_pitch
        );
    }

    #[test]
    fn kkit_ignores_pitch() {
        let (mut module, _, _) = make_test_module();
        module.dna.species = "kkit".into();
        module.last_actual_pitch = 100.0;

        let new_pitch = module.personality_transform_pitch(1000.0);

        // KKIT should ignore external pitch
        assert!(
            (new_pitch - 100.0).abs() < 0.01,
            "KKIT should ignore pitch, got {}",
            new_pitch
        );
    }

    // Interaction param tests

    fn make_interaction_module() -> (
        OrganismModule,
        crate::substrate::channel::Sender<DspAnalysis>,
        crate::substrate::channel::Receiver<DspCommand>,
    ) {
        use super::super::dna::*;

        let dna = OrganismDna {
            name: "interact-test".into(),
            species: "acid".into(),
            active: true,
            seed: 42,
            version: 1,
            cells: vec![],
            cell_wiring: vec![],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: Some(InteractionParams {
                exports: vec![ParamExport {
                    name: "env_out".into(),
                    source: "rms".into(),
                    signal_type: "Float".into(),
                }],
                imports: vec![ParamImport {
                    name: "filter_mod".into(),
                    target: "cell0.cutoff".into(),
                    range: (20.0, 5000.0),
                    gain: 1500.0,
                    mode: "Add".into(),
                    accepts_from: vec!["*".into()],
                }],
            }),
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.8,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
        };

        let mut handles = HashMap::new();
        handles.insert("cell0.cutoff".into(), Shared::new(400.0));

        let (analysis_tx, analysis_rx) = channel::channel::<DspAnalysis>(32);
        let (cmd_tx, cmd_rx) = channel::channel::<DspCommand>(32);

        let module = OrganismModule::new(dna, handles, analysis_rx, cmd_tx, 0);
        (module, analysis_tx, cmd_rx)
    }

    #[test]
    fn interaction_exports_add_output_ports() {
        let (module, _, _) = make_interaction_module();
        let schema = module.schema();
        // 9 base outputs + 1 export = 10
        assert_eq!(schema.outputs.len(), 10);
        assert!(schema.output("x:env_out").is_some());
    }

    #[test]
    fn interaction_imports_add_input_ports() {
        let (module, _, _) = make_interaction_module();
        let schema = module.schema();
        // 8 base inputs + 1 import = 9
        assert_eq!(schema.inputs.len(), 9);
        assert!(schema.input("x:filter_mod").is_some());
    }

    #[test]
    fn interaction_export_emits_rms() {
        let (mut module, mut analysis_tx, _) = make_interaction_module();

        analysis_tx
            .try_send(DspAnalysis::new(0.75, 0.9))
            .unwrap();
        module.tick(1.0 / 60.0);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        let export_port = module.param_exports[0].port_id;
        let export_signal = buffer.iter().find(|(port, _)| *port == export_port);
        assert!(export_signal.is_some(), "should emit on export port");
        if let Some((_, Signal::Float(v))) = export_signal {
            assert!(
                (*v - 0.75).abs() < 0.01,
                "export should emit RMS value, got {}",
                v
            );
        }
    }

    #[test]
    fn interaction_import_modulates_param() {
        let (mut module, _, _) = make_interaction_module();

        let import_port = module.param_imports[0].port_id;
        // Send 0.5 signal → base(400) + 0.5 * gain(1500) = 400 + 750 = 1150
        module
            .receive_signal(import_port, Signal::Float(0.5))
            .unwrap();

        let cutoff = module.shared_handles.get("cell0.cutoff").unwrap().value();
        assert!(
            (cutoff - 1150.0).abs() < 1.0,
            "cutoff should be modulated to ~1150, got {}",
            cutoff
        );
    }

    #[test]
    fn interaction_import_clamps_to_range() {
        let (mut module, _, _) = make_interaction_module();

        let import_port = module.param_imports[0].port_id;
        // Send huge signal → base(400) + 10.0 * 1500 = 15400 → clamped to 5000
        module
            .receive_signal(import_port, Signal::Float(10.0))
            .unwrap();

        let cutoff = module.shared_handles.get("cell0.cutoff").unwrap().value();
        assert!(
            (cutoff - 5000.0).abs() < 1.0,
            "cutoff should be clamped to max range 5000, got {}",
            cutoff
        );
    }

    // S33 Scale/Rhythm Bridge Tests

    #[test]
    fn quantize_to_scale_basic() {
        // C major gravity weights: C D E F G A B
        let mut gravity = vec![0.0f32; 12];
        gravity[0] = 1.0; // C
        gravity[2] = 0.8; // D
        gravity[4] = 0.9; // E
        gravity[5] = 0.7; // F
        gravity[7] = 1.0; // G
        gravity[9] = 0.8; // A
        gravity[11] = 0.7; // B

        // E4 = 329.63 Hz (MIDI 64, degree 4) — should stay on E
        let result = quantize_to_scale(329.63, &gravity, 1.0);
        assert!(
            (result - 329.63).abs() < 1.0,
            "E4 should stay on E in C major, got {}",
            result
        );

        // Eb4 = 311.13 Hz (MIDI 63, degree 3) — should snap toward E (degree 4, weight 0.9)
        let result = quantize_to_scale(311.13, &gravity, 1.0);
        // With full blend, should be very close to E4
        assert!(
            (result - 329.63).abs() < 5.0,
            "Eb4 should snap to E4 in C major, got {}",
            result
        );
    }

    #[test]
    fn quantize_to_scale_blend_zero() {
        let gravity = vec![1.0; 12];
        let raw = 440.0;
        let result = quantize_to_scale(raw, &gravity, 0.0);
        assert!(
            (result - raw).abs() < 0.01,
            "Blend=0 should return raw Hz, got {}",
            result
        );
    }

    #[test]
    fn receive_gravity_weights_sends_command() {
        let (mut module, _, mut cmd_rx) = make_test_module();

        module.dna.scale_affinity = 1.0;
        module.dna.fidelity = 1.0;

        // C major gravity weights
        let mut gravity = vec![0.0f32; 12];
        gravity[0] = 1.0; // C
        gravity[4] = 1.0; // E
        gravity[7] = 1.0; // G

        let signal = Signal::Pattern(Arc::new(gravity));
        module
            .receive_signal(module.gravity_weights_port, signal)
            .unwrap();

        // Should send SetScaleWeights to audio thread
        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_some(), "Should send SetScaleWeights command");
        if let Some(DspCommand::SetScaleWeights(w, blend)) = cmd {
            assert_eq!(w[0], 1.0, "C weight");
            assert_eq!(w[4], 1.0, "E weight");
            assert_eq!(w[7], 1.0, "G weight");
            assert!((blend - 1.0).abs() < 0.01, "blend should be scale_affinity * fidelity = 1.0");
        } else {
            panic!("Expected SetScaleWeights command, got {:?}", cmd);
        }
    }

    #[test]
    fn receive_beat_trigger_hard_sync() {
        let (mut module, _, mut cmd_rx) = make_test_module();
        module.dna.rhythm_sync = "hard".into();
        module.dna.rhythm_affinity = 1.0;

        module
            .receive_signal(module.beat_trigger_port, Signal::Trigger)
            .unwrap();

        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_some(), "Should send NudgePhase on hard sync beat trigger");
        if let Some(DspCommand::NudgePhase(nudge)) = cmd {
            assert!(
                nudge.abs() >= 1.0,
                "Hard sync should send large nudge for phase reset, got {}",
                nudge
            );
        } else {
            panic!("Expected NudgePhase command");
        }
    }

    #[test]
    fn receive_beat_trigger_ignored_when_none() {
        let (mut module, _, mut cmd_rx) = make_test_module();
        module.dna.rhythm_sync = "none".into();

        module
            .receive_signal(module.beat_trigger_port, Signal::Trigger)
            .unwrap();

        let cmd = cmd_rx.try_recv();
        assert!(cmd.is_none(), "Should NOT send NudgePhase when rhythm_sync=none");
    }

    #[test]
    fn receive_gamaka_sends_slew_and_lfo() {
        let (mut module, _, mut cmd_rx) = make_test_module();
        module.dna.scale_affinity = 1.0;
        module.dna.fidelity = 1.0;

        // gamaka: [slide_ms=50, vib_depth_cents=20, vib_rate_hz=5]
        let config = vec![50.0, 20.0, 5.0];
        let signal = Signal::Pattern(Arc::new(config));
        module
            .receive_signal(module.gamaka_config_port, signal)
            .unwrap();

        // Should produce SetSlewRate + SetLfoParams commands
        let cmd1 = cmd_rx.try_recv();
        assert!(cmd1.is_some(), "Should send SetSlewRate");
        assert!(
            matches!(cmd1, Some(DspCommand::SetSlewRate(_, _))),
            "First command should be SetSlewRate"
        );

        let cmd2 = cmd_rx.try_recv();
        assert!(cmd2.is_some(), "Should send SetLfoParams");
        assert!(
            matches!(cmd2, Some(DspCommand::SetLfoParams(_, _))),
            "Second command should be SetLfoParams"
        );
    }
}
