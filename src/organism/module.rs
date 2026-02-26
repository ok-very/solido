use std::any::Any;

use fundsp::prelude32::Shared;

use crate::dsp::command::DspAnalysis;
use crate::dsp::organism_dsp::SharedHandles;
use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::substrate::channel::Receiver;

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
    // Cached analysis from audio thread
    current_rms: f32,
    current_peak: f32,
    // OrganismState identity (for visual updates in app.rs)
    organism_id: OrganismId,
    // Port IDs
    pitch_hz_port: PortId,
    rms_in_port: PortId,
    rms_out_port: PortId,
    peak_out_port: PortId,
    is_active_port: PortId,
}

impl OrganismModule {
    /// Create an OrganismModule from DNA, shared handles, and channel endpoints.
    ///
    /// `shared_handles` and `analysis_rx` come from AudioSubstrate after building
    /// the organism's DSP graph. `organism_id` is the OrganismState ID in the registry.
    pub fn new(
        dna: OrganismDna,
        shared_handles: SharedHandles,
        analysis_rx: Receiver<DspAnalysis>,
        organism_id: OrganismId,
    ) -> Self {
        let pitch_hz_in = Port::input("pitch_hz", SignalType::Float, PortRate::Block)
            .with_range(20.0, 20000.0)
            .with_description("Pitch frequency in Hz — drives voice cell frequencies");
        let rms_in = Port::input("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("External RMS level — modulates organism arousal");

        let rms_out = Port::output("rms", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Organism audio RMS level");
        let peak_out = Port::output("peak", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Organism audio peak level");
        let is_active_out = Port::output("is_active", SignalType::Bool, PortRate::Block)
            .with_description("True when organism is producing sound");

        let pitch_hz_port = pitch_hz_in.id;
        let rms_in_port = rms_in.id;
        let rms_out_port = rms_out.id;
        let peak_out_port = peak_out.id;
        let is_active_port = is_active_out.id;

        let module_name = format!("organism:{}", dna.name);
        let schema = ModuleSchema::new(&module_name, ModuleCategory::Output)
            .with_description(&format!(
                "{} organism — species: {}",
                dna.name, dna.species
            ))
            .with_tier(ModuleTier::Organism)
            .with_input(pitch_hz_in)
            .with_input(rms_in)
            .with_output(rms_out)
            .with_output(peak_out)
            .with_output(is_active_out)
            .with_side_effect("audio_output")
            .with_initial_emotion(dna.emotion.base_arousal, dna.emotion.base_valence);

        Self {
            schema,
            dna,
            shared_handles,
            analysis_rx,
            current_rms: 0.0,
            current_peak: 0.0,
            organism_id,
            pitch_hz_port,
            rms_in_port,
            rms_out_port,
            peak_out_port,
            is_active_port,
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
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.pitch_hz_port {
            if let Signal::Float(hz) = signal {
                self.apply_pitch_hz(hz);
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

    fn tick(&mut self, _dt: f32) {
        // Drain analysis from audio thread
        while let Some(analysis) = self.analysis_rx.try_recv() {
            self.current_rms = analysis.rms;
            self.current_peak = analysis.peak;
        }
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

    fn make_test_module() -> (OrganismModule, crate::substrate::channel::Sender<DspAnalysis>) {
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
            affinity_tags: vec![],
            affinity_biases: vec![],
        };

        let mut handles = HashMap::new();
        handles.insert("cell0.root_hz".into(), Shared::new(110.0));
        handles.insert("cell0.cutoff".into(), Shared::new(800.0));

        let (analysis_tx, analysis_rx) = channel::channel::<DspAnalysis>(32);

        let module = OrganismModule::new(dna, handles, analysis_rx, 0);
        (module, analysis_tx)
    }

    #[test]
    fn schema_has_correct_ports() {
        let (module, _) = make_test_module();
        let schema = module.schema();

        assert_eq!(schema.tier, ModuleTier::Organism);
        assert_eq!(schema.inputs.len(), 2);
        assert_eq!(schema.outputs.len(), 3);
        assert!(schema.input("pitch_hz").is_some());
        assert!(schema.input("rms").is_some());
        assert!(schema.output("rms").is_some());
        assert!(schema.output("peak").is_some());
        assert!(schema.output("is_active").is_some());
        assert!(schema.side_effects.contains(&"audio_output".to_string()));
    }

    #[test]
    fn receive_pitch_hz_sets_shared_handle() {
        let (mut module, _) = make_test_module();

        module
            .receive_signal(module.pitch_hz_port, Signal::Float(220.0))
            .unwrap();

        let root_hz = module.shared_handles.get("cell0.root_hz").unwrap();
        assert!(
            (root_hz.value() - 220.0).abs() < 0.01,
            "pitch_hz should set root_hz shared handle, got {}",
            root_hz.value()
        );
    }

    #[test]
    fn receive_rms_accepts_float() {
        let (mut module, _) = make_test_module();
        let result = module.receive_signal(module.rms_in_port, Signal::Float(0.5));
        assert!(result.is_ok());
    }

    #[test]
    fn reject_wrong_type() {
        let (mut module, _) = make_test_module();
        let result = module.receive_signal(module.pitch_hz_port, Signal::Trigger);
        assert!(matches!(result, Err(SignalError::WrongType { .. })));
    }

    #[test]
    fn reject_unknown_port() {
        let (mut module, _) = make_test_module();
        let result = module.receive_signal(PortId(99999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn tick_drains_analysis() {
        let (mut module, mut analysis_tx) = make_test_module();

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
    fn emit_signals_returns_3() {
        let (mut module, _) = make_test_module();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn is_active_reflects_rms() {
        let (mut module, mut analysis_tx) = make_test_module();

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
    fn pitch_hz_clamps_to_range() {
        let (mut module, _) = make_test_module();

        // Below minimum
        module
            .receive_signal(module.pitch_hz_port, Signal::Float(5.0))
            .unwrap();
        let root_hz = module.shared_handles.get("cell0.root_hz").unwrap();
        assert!(
            (root_hz.value() - 20.0).abs() < 0.01,
            "should clamp to 20 Hz min, got {}",
            root_hz.value()
        );
    }
}
