use super::port::{Port, PortId};

/// What kind of thing this module does in the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleCategory {
    /// Feeds external data into the system (keyboard, camera, OSC).
    Input,
    /// Transforms signals (quantizer, raga, pitch gravity).
    Processing,
    /// Produces audible/visible output (voice, blob renderer).
    Output,
    /// Visual-only output (data diagram, ASCII texture, ISF shader).
    Visual,
}

/// Whether a module is infrastructure (fixed routing) or organism (learned routing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleTier {
    /// Fixed routing via InfrastructureRouter. No emotions, no Hebbian learning,
    /// no exploration, no pruning. Studio hardware: pickups, strings, frets, mixer.
    Infrastructure,
    /// AffinityGraph routing with full Hebbian learning. Creative entities that
    /// learn which connections are productive. Visualized as blobs.
    Organism,
}

/// Complete metadata for a Module — its ports, category, and side effects.
///
/// The schema is the "contract" that the affinity graph reads to determine
/// which edges are valid (type-compatible ports between modules).
#[derive(Clone, Debug)]
pub struct ModuleSchema {
    pub name: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// Side effects like "audio_output", "gpu_render", "file_write".
    pub side_effects: Vec<String>,
    pub category: ModuleCategory,
    /// Infrastructure modules get fixed routing; organism modules get AffinityGraph.
    pub tier: ModuleTier,
}

impl ModuleSchema {
    pub fn new(name: &str, category: ModuleCategory) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            side_effects: Vec::new(),
            category,
            tier: ModuleTier::Organism,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_input(mut self, port: Port) -> Self {
        self.inputs.push(port);
        self
    }

    pub fn with_output(mut self, port: Port) -> Self {
        self.outputs.push(port);
        self
    }

    pub fn with_side_effect(mut self, effect: &str) -> Self {
        self.side_effects.push(effect.to_string());
        self
    }

    pub fn with_tier(mut self, tier: ModuleTier) -> Self {
        self.tier = tier;
        self
    }

    /// Find an input port by name.
    pub fn input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| &*p.name == name)
    }

    /// Find an output port by name.
    pub fn output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| &*p.name == name)
    }

    /// Get the PortId for a named input port.
    pub fn input_id(&self, name: &str) -> Option<PortId> {
        self.input(name).map(|p| p.id)
    }

    /// Get the PortId for a named output port.
    pub fn output_id(&self, name: &str) -> Option<PortId> {
        self.output(name).map(|p| p.id)
    }
}
