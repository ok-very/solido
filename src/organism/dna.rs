use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Complete organism blueprint: audio, visual, physics, social identity.
/// Serialized as a single JSON document. One struct = one organism.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganismDna {
    // --- Identity ---
    pub name: String,
    pub species: String,
    pub seed: u64,
    pub version: u32,

    // --- Audio / DSP (S12) ---
    pub cells: Vec<CellDna>,
    pub cell_wiring: Vec<CellWire>,

    // --- Send buses ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sends: Option<SendsDna>,

    // --- Visual Body (S09) ---
    pub body: BodyDna,

    // --- Rendering (S09) ---
    pub render: RenderDna,

    // --- Physics (S09) ---
    pub physics: PhysicsDna,

    // --- Emotion / Social ---
    pub emotion: EmotionDna,
    pub affinity_tags: Vec<String>,
    pub affinity_biases: Vec<AffinityBias>,

    // --- Inter-organism parameter interaction ---
    /// External patch points: CV-style exports and imports for cross-organism modulation.
    /// Exports expose internal signals (RMS, rhythm, cell params) as output ports.
    /// Imports accept modulation from other organisms on specific Shared handles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_params: Option<InteractionParams>,

    // --- Dialogue Personality (S19) ---
    /// How closely organism follows external prompts vs internal patterns (0.0-1.0).
    /// 0.0 = ignores external input, 1.0 = faithful follower.
    /// Species defaults: DRON=0.3, HOSO=0.9, SPGL=0.1, ACID=0.8, TBLK=0.5, KKIT=0.95
    #[serde(default = "default_fidelity")]
    pub fidelity: f32,

    /// How much this organism follows external scales (raga quantization) [0,1].
    /// Separate from fidelity — scale_affinity specifically controls raga pitch snapping.
    /// Species defaults: DRON=0.3, HOSO=0.8, SPGL=0.1, ACID=0.7, TBLK=0.2, KKIT=0.0
    #[serde(default = "default_scale_affinity")]
    pub scale_affinity: f32,

    /// How strongly this organism syncs to external rhythm (tala beat grid) [0,1].
    /// Species defaults: DRON=0.1, HOSO=0.7, SPGL=0.0, ACID=0.8, TBLK=0.6, KKIT=0.9
    #[serde(default = "default_rhythm_affinity")]
    pub rhythm_affinity: f32,

    /// Rhythm sync mode: "none" | "soft" | "hard".
    /// none = ignore tala, soft = phase nudge toward beat, hard = lock to beat grid.
    #[serde(default = "default_rhythm_sync")]
    pub rhythm_sync: String,

    /// Whether this organism loads and activates on startup.
    /// Set false to park an organism without removing its DNA.
    /// Defaults to true when absent from JSON.
    #[serde(default = "default_active")]
    pub active: bool,

    /// Root pitch class for this organism: 0=C, 1=C#, 2=D, ... 9=A, 11=B.
    /// Used by GravityField to transpose scale weights so the organism
    /// hears the active scale relative to its own tonal center.
    #[serde(default)]
    pub root_pitch_class: u8,
}

fn default_fidelity() -> f32 {
    0.5
}

fn default_active() -> bool {
    true
}

fn default_scale_affinity() -> f32 {
    0.5
}

fn default_rhythm_affinity() -> f32 {
    0.5
}

fn default_rhythm_sync() -> String {
    "none".into()
}

fn default_viscosity() -> f32 {
    0.5
}

fn default_shape_amplitude() -> f32 {
    30.0
}

fn default_shape_frequency() -> f32 {
    0.012
}

fn default_rd_reactivity() -> f32 {
    0.5
}

fn default_rd_feed() -> f32 {
    0.035
}

fn default_rd_kill() -> f32 {
    0.065
}

fn default_rd_scale() -> f32 {
    2.0
}

fn default_harmonic_count() -> f32 {
    0.0
}

fn default_harmonic_amp() -> f32 {
    0.0
}

fn default_elongation() -> f32 {
    0.0
}

fn default_chladni_m() -> f32 {
    2.0
}

fn default_chladni_n() -> f32 {
    1.0
}

/// A parameter this organism exports for other organisms to read.
/// Like a CV output jack on a synth module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamExport {
    /// Port name visible in affinity graph (e.g., "envelope", "rhythm")
    pub name: String,
    /// What internal signal this tracks. Options:
    ///   "rms"            — organism's audio RMS (already computed)
    ///   "rhythm_density" — trigger rate (already computed)
    ///   "cell{N}.{param}" — a specific cell's Shared param value
    pub source: String,
    /// Signal type for affinity routing
    pub signal_type: String, // "Float" or "Trigger"
}

/// A parameter this organism accepts modulation on.
/// Like a CV input jack on a synth module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamImport {
    /// Port name visible in affinity graph (e.g., "filter_mod", "decay_mod")
    pub name: String,
    /// Which Shared handle to modulate (e.g., "cell5.cutoff", "cell2.decay")
    pub target: String,
    /// Valid range for the target parameter
    pub range: (f32, f32),
    /// Scale factor: incoming signal [0,1] × gain → modulation amount
    pub gain: f32,
    /// Add (base + mod) or Multiply (base × mod)
    pub mode: String, // "Add" or "Multiply"
    /// Which species can modulate this. ["*"] = any, ["spgl","dron"] = specific
    pub accepts_from: Vec<String>,
}

/// External patch points for inter-organism modulation.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct InteractionParams {
    #[serde(default)]
    pub exports: Vec<ParamExport>,
    #[serde(default)]
    pub imports: Vec<ParamImport>,
}

/// Audio cell blueprint: type string + named parameter map.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellDna {
    pub cell_type: String,
    pub params: BTreeMap<String, f32>,
    #[serde(default)]
    pub string_params: BTreeMap<String, String>,
}

/// Send bus configuration embedded in organism DNA.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SendsDna {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverb: Option<ReverbSendDna>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tape_delay: Option<TapeDelaySendDna>,
}

/// Per-organism tape delay send configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TapeDelaySendDna {
    /// Send level from this organism to the tape delay bus [0.0, 1.0]
    pub send: f32,
    /// Delay algorithm params: time, feedback, hf_damp
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
}

/// Per-organism reverb send configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReverbSendDna {
    /// Reverb algorithm type: "plate", "hall", "room"
    #[serde(rename = "type")]
    pub reverb_type: String,
    /// Send level from this organism to the reverb bus [0.0, 1.0]
    pub send: f32,
    /// Reverb algorithm params (size, dcy, damp, etc.)
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
}

/// Wiring between cells in an organism.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellWire {
    pub src_cell: usize,
    pub dst_cell: usize,
    pub wire_type: WireType,
    pub gain: f32,
    pub mode: WireMode,
}

/// How a wire combines its signal with the destination input.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum WireMode {
    #[default]
    Add,
    Multiply,
    /// Modulation signal × gain replaces the base value entirely.
    /// Falls back to base when source is bypassed.
    Replace,
}

/// Type of connection between cells.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WireType {
    Audio,
    Trigger,
    Modulation { target_param: String },
}

/// Visual body shape parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyDna {
    pub lobe_count: u8,
    pub core_radius: f32,
    pub pseudopod_gain: f32,
    pub extension_speed: f32,
    pub retraction_speed: f32,
    #[serde(default = "default_shape_amplitude")]
    pub shape_amplitude: f32,
    #[serde(default = "default_shape_frequency")]
    pub shape_frequency: f32,
    /// How reactive this organism's trail slime is to reaction-diffusion [0,1].
    #[serde(default = "default_rd_reactivity")]
    pub rd_reactivity: f32,
    /// Gray-Scott feed rate — controls RD pattern topology.
    #[serde(default = "default_rd_feed")]
    pub rd_feed: f32,
    /// Gray-Scott kill rate — controls RD pattern topology.
    #[serde(default = "default_rd_kill")]
    pub rd_kill: f32,
    /// RD pattern scale — higher = larger patterns. Energy modulates dynamically.
    #[serde(default = "default_rd_scale")]
    pub rd_scale: f32,
    /// Angular harmonic lobe count: 2=ellipse, 3=trefoil, 5=starfish. 0=circle.
    #[serde(default = "default_harmonic_count")]
    pub harmonic_count: f32,
    /// Angular harmonic amplitude [0, 0.4]. 0 = circle, higher = more pronounced lobes.
    #[serde(default = "default_harmonic_amp")]
    pub harmonic_amp: f32,
    /// Velocity elongation sensitivity [0, 1]. 0 = no stretch, 1 = very stretchy.
    #[serde(default = "default_elongation")]
    pub elongation: f32,
    /// Chladni modal number m (2–5): controls radial vibration pattern.
    #[serde(default = "default_chladni_m")]
    pub chladni_m: f32,
    /// Chladni modal number n (1–3): controls angular pattern complexity.
    #[serde(default = "default_chladni_n")]
    pub chladni_n: f32,
}

/// Rendering appearance parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderDna {
    pub smin_k: f32,
    pub edge_softness: f32,
    pub glow: f32,
    pub hue: f32,
    pub thermal_enabled: bool,
    pub palette_variant: u8,
    pub pulse_response: f32,
}

/// Physics / movement parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhysicsDna {
    pub mass: f32,
    pub drag: f32,
    pub max_speed: f32,
    #[serde(default = "default_viscosity")]
    pub viscosity: f32,
    pub interaction_rules: Vec<InteractionRule>,
}

/// Rule governing inter-organism interaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionRule {
    pub with_species: String,
    pub mode: InteractionMode,
    pub range: f32,
    pub strength: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dwell_secs: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest_length: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_force: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_distance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affinity_threshold: Option<f32>,
}

/// Interaction mode enum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionMode {
    Repel,
    Bounce,
    Slow,
    Attach,
    Glob,
    Orbit,
    IntegratePropose,
}

/// Base emotional disposition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmotionDna {
    pub base_valence: f32,
    pub base_arousal: f32,
    /// Initial desire to connect [0, 1]. Adapts at runtime based on valence.
    #[serde(default = "default_desire_to_connect")]
    pub desire_to_connect: f32,
}

fn default_desire_to_connect() -> f32 {
    0.3
}

/// Initial affinity graph bias for a named port.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AffinityBias {
    pub port_name: String,
    pub bias: f32,
}

impl Default for BodyDna {
    fn default() -> Self {
        Self {
            lobe_count: 6,
            core_radius: 18.0,
            pseudopod_gain: 0.9,
            extension_speed: 6.0,
            retraction_speed: 8.0,
            shape_amplitude: 30.0,
            shape_frequency: 0.012,
            rd_reactivity: 0.5,
            rd_feed: 0.035,
            rd_kill: 0.065,
            rd_scale: 2.0,
            harmonic_count: 0.0,
            harmonic_amp: 0.0,
            elongation: 0.0,
            chladni_m: 2.0,
            chladni_n: 1.0,
        }
    }
}

impl Default for RenderDna {
    fn default() -> Self {
        Self {
            smin_k: 0.25,
            edge_softness: 2.0,
            glow: 0.4,
            hue: 0.5,
            thermal_enabled: true,
            palette_variant: 0,
            pulse_response: 0.5,
        }
    }
}

impl Default for PhysicsDna {
    fn default() -> Self {
        Self {
            mass: 1.0,
            drag: 0.9,
            max_speed: 150.0,
            viscosity: 0.5,
            interaction_rules: vec![InteractionRule {
                with_species: "*".into(),
                mode: InteractionMode::Repel,
                range: 25.0,
                strength: 1.0,
                dwell_secs: None,
                rest_length: None,
                break_force: None,
                break_distance: None,
                affinity_threshold: None,
            }],
        }
    }
}

impl Default for EmotionDna {
    fn default() -> Self {
        Self {
            base_valence: 0.0,
            base_arousal: 0.5,
            desire_to_connect: 0.3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_dna() -> OrganismDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), 440.0);
        OrganismDna {
            name: "test".into(),
            species: "tblk".into(),
            active: true,
            seed: 42,
            version: 1,
            cells: vec![CellDna {
                cell_type: "hexo_strike".into(),
                params,
                string_params: BTreeMap::new(),
            }],
            cell_wiring: vec![],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec!["test".into()],
            affinity_biases: vec![AffinityBias {
                port_name: "note_on".into(),
                bias: 0.5,
            }],
            fidelity: 0.5,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
            root_pitch_class: 0,
        }
    }

    #[test]
    fn dna_clone_preserves_values() {
        let dna = make_test_dna();
        let cloned = dna.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.cells.len(), 1);
        assert_eq!(cloned.cells[0].cell_type, "hexo_strike");
    }

    #[test]
    fn dna_serialize_deserialize_roundtrip() {
        let dna = make_test_dna();
        let json = serde_json::to_string_pretty(&dna).unwrap();
        let loaded: OrganismDna = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.name, dna.name);
        assert_eq!(loaded.species, dna.species);
        assert_eq!(loaded.seed, dna.seed);
        assert_eq!(loaded.cells.len(), dna.cells.len());
        assert_eq!(loaded.cells[0].cell_type, dna.cells[0].cell_type);
        let freq = loaded.cells[0].params.get("freq").unwrap();
        assert!((freq - 440.0).abs() < 0.01);
    }

    #[test]
    fn cell_dna_string_params_serde_default() {
        // Old JSON without string_params should deserialize with empty map
        let json = r#"{"cell_type":"hexo_strike","params":{"freq":440.0}}"#;
        let dna: CellDna = serde_json::from_str(json).unwrap();
        assert_eq!(dna.cell_type, "hexo_strike");
        assert!(dna.string_params.is_empty());
    }

    #[test]
    fn cell_dna_string_params_roundtrip() {
        let mut string_params = BTreeMap::new();
        string_params.insert("osc.mode".into(), "sine_cluster".into());
        string_params.insert("filter.type".into(), "ladder".into());
        let dna = CellDna {
            cell_type: "hexo_timbre".into(),
            params: BTreeMap::new(),
            string_params,
        };
        let json = serde_json::to_string(&dna).unwrap();
        let loaded: CellDna = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.string_params.get("osc.mode").unwrap(), "sine_cluster");
        assert_eq!(loaded.string_params.get("filter.type").unwrap(), "ladder");
    }

    #[test]
    fn wire_type_serialization() {
        let wire = CellWire {
            src_cell: 0,
            dst_cell: 1,
            wire_type: WireType::Modulation {
                target_param: "cutoff".into(),
            },
            gain: 1.0,
            mode: WireMode::default(),
        };
        let json = serde_json::to_string(&wire).unwrap();
        let loaded: CellWire = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.src_cell, 0);
        match loaded.wire_type {
            WireType::Modulation { ref target_param } => {
                assert_eq!(target_param, "cutoff");
            }
            _ => panic!("Expected Modulation variant"),
        }
    }

    #[test]
    fn fidelity_serialization_roundtrip() {
        let dna = make_test_dna();
        let json = serde_json::to_string(&dna).unwrap();
        let loaded: OrganismDna = serde_json::from_str(&json).unwrap();
        assert!(
            (loaded.fidelity - 0.5).abs() < 0.01,
            "fidelity should roundtrip correctly"
        );
    }

    #[test]
    fn interaction_params_roundtrip() {
        let mut dna = make_test_dna();
        dna.interaction_params = Some(InteractionParams {
            exports: vec![ParamExport {
                name: "env_out".into(),
                source: "rms".into(),
                signal_type: "Float".into(),
            }],
            imports: vec![ParamImport {
                name: "filter_mod".into(),
                target: "cell1.cutoff".into(),
                range: (20.0, 5000.0),
                gain: 1500.0,
                mode: "Add".into(),
                accepts_from: vec!["*".into()],
            }],
        });
        let json = serde_json::to_string_pretty(&dna).unwrap();
        let loaded: OrganismDna = serde_json::from_str(&json).unwrap();
        let ip = loaded.interaction_params.expect("interaction_params should roundtrip");
        assert_eq!(ip.exports.len(), 1);
        assert_eq!(ip.exports[0].name, "env_out");
        assert_eq!(ip.imports.len(), 1);
        assert_eq!(ip.imports[0].target, "cell1.cutoff");
        assert_eq!(ip.imports[0].range, (20.0, 5000.0));
        assert_eq!(ip.imports[0].accepts_from, vec!["*"]);
    }

    #[test]
    fn interaction_params_defaults_when_missing() {
        // Old JSON without interaction_params should deserialize with None
        let json = r#"{
            "name":"old-org","species":"dron","seed":1,"version":1,
            "cells":[],"cell_wiring":[],
            "body":{"lobe_count":6,"core_radius":18.0,"pseudopod_gain":0.9,"extension_speed":6.0,"retraction_speed":8.0},
            "render":{"smin_k":0.25,"edge_softness":2.0,"glow":0.4,"hue":0.5,"thermal_enabled":true,"palette_variant":0,"pulse_response":0.5},
            "physics":{"mass":1.0,"drag":0.9,"max_speed":150.0,"interaction_rules":[]},
            "emotion":{"base_valence":0.0,"base_arousal":0.5},
            "affinity_tags":[],"affinity_biases":[]
        }"#;
        let dna: OrganismDna = serde_json::from_str(json).unwrap();
        assert!(dna.interaction_params.is_none(), "missing interaction_params should default to None");
    }

    #[test]
    fn fidelity_defaults_when_missing() {
        // Old JSON without fidelity should deserialize with default value
        let json = r#"{
            "name":"old-org","species":"dron","seed":1,"version":1,
            "cells":[],"cell_wiring":[],
            "body":{"lobe_count":6,"core_radius":18.0,"pseudopod_gain":0.9,"extension_speed":6.0,"retraction_speed":8.0},
            "render":{"smin_k":0.25,"edge_softness":2.0,"glow":0.4,"hue":0.5,"thermal_enabled":true,"palette_variant":0,"pulse_response":0.5},
            "physics":{"mass":1.0,"drag":0.9,"max_speed":150.0,"interaction_rules":[]},
            "emotion":{"base_valence":0.0,"base_arousal":0.5},
            "affinity_tags":[],"affinity_biases":[]
        }"#;
        let dna: OrganismDna = serde_json::from_str(json).unwrap();
        assert!(
            (dna.fidelity - 0.5).abs() < 0.01,
            "missing fidelity should default to 0.5"
        );
    }
}
