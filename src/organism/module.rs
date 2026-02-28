use std::any::Any;

use crate::dsp::command::{DspAnalysis, DspCommand};
use crate::dsp::organism_dsp::SharedHandles;
use crate::dsp::shared::Shared;
use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::substrate::channel::{Receiver, Sender};

use super::dna::OrganismDna;
use super::sim::OrganismId;

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

        let pitch_hz_port = pitch_hz_in.id;
        let rms_in_port = rms_in.id;
        let gate_port = gate_in.id;
        let accent_port = accent_in.id;
        let rms_out_port = rms_out.id;
        let peak_out_port = peak_out.id;
        let is_active_port = is_active_out.id;
        let actual_pitch_port = actual_pitch_out.id;
        let rhythm_density_port = rhythm_density_out.id;

        let module_name = format!("organism:{}", dna.name);
        let schema = ModuleSchema::new(&module_name, ModuleCategory::Output)
            .with_description(&format!(
                "{} organism — species: {} (fidelity: {:.1})",
                dna.name, dna.species, dna.fidelity
            ))
            .with_tier(ModuleTier::Organism)
            .with_input(pitch_hz_in)
            .with_input(rms_in)
            .with_input(gate_in)
            .with_input(accent_in)
            .with_output(rms_out)
            .with_output(peak_out)
            .with_output(is_active_out)
            .with_output(actual_pitch_out)
            .with_output(rhythm_density_out)
            .with_side_effect("audio_output")
            .with_initial_emotion(dna.emotion.base_arousal, dna.emotion.base_valence);

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
            actual_pitch_port,
            rhythm_density_port,
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

    /// Current peak from the audio thread.
    pub fn current_peak(&self) -> f32 {
        self.current_peak
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
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
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

        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, dt: f32) {
        // Drain analysis from audio thread
        while let Some(analysis) = self.analysis_rx.try_recv() {
            self.current_rms = analysis.rms;
            self.current_peak = analysis.peak;
        }

        // Update rhythm tracking
        self.last_gate_time += dt;
        self.tick_counter += 1;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
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
            seed: 42,
            version: 1,
            cells: vec![],
            cell_wiring: vec![],
            body: super::super::dna::BodyDna::default(),
            render: super::super::dna::RenderDna::default(),
            physics: super::super::dna::PhysicsDna::default(),
            emotion: super::super::dna::EmotionDna::default(),
            sends: None,
            affinity_tags: vec![],
            affinity_biases: vec![],
            fidelity: 0.3,
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
        assert_eq!(schema.inputs.len(), 4); // pitch_hz, rms, gate, accent
        assert_eq!(schema.outputs.len(), 5); // rms, peak, is_active, actual_pitch, rhythm_density
        assert!(schema.input("pitch_hz").is_some());
        assert!(schema.input("rms").is_some());
        assert!(schema.input("gate").is_some());
        assert!(schema.input("accent").is_some());
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
            .try_send(DspAnalysis {
                rms: 0.42,
                peak: 0.85,
            })
            .unwrap();

        module.tick(1.0 / 60.0);

        assert!((module.current_rms - 0.42).abs() < 1e-6);
        assert!((module.current_peak - 0.85).abs() < 1e-6);
    }

    #[test]
    fn emit_signals_returns_5() {
        let (mut module, _, _) = make_test_module();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        // rms, peak, is_active, actual_pitch, rhythm_density
        assert_eq!(buffer.len(), 5);
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
            .try_send(DspAnalysis {
                rms: 0.1,
                peak: 0.2,
            })
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
}
