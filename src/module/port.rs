use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::signal::SignalType;

/// Global counter for unique port IDs.
static NEXT_PORT_ID: AtomicU32 = AtomicU32::new(0);

/// Cheap, Copy port identifier — just a u32.
///
/// Assigned automatically when a `Port` is created. Modules store their
/// output PortIds as fields and copy them (4 bytes, no atomic ops) into
/// the emit buffer each tick.
///
/// Names live in the `PortRegistry` for UI/debug/ledger display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

impl PortId {
    fn next() -> Self {
        Self(NEXT_PORT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for PortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "port:{}", self.0)
    }
}

/// Maps PortId → human-readable name for UI, debug, and ledger display.
///
/// Built from module schemas when modules register with the SeedReactor.
/// Also supports dynamic names from ISF shaders and YAML definitions.
pub struct PortRegistry {
    names: HashMap<PortId, Arc<str>>,
}

impl PortRegistry {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: PortId, name: &str) {
        self.names.insert(id, Arc::from(name));
    }

    /// Register all ports from a module schema.
    pub fn register_schema(&mut self, schema: &super::schema::ModuleSchema) {
        for port in &schema.inputs {
            self.names.insert(port.id, port.name.clone());
        }
        for port in &schema.outputs {
            self.names.insert(port.id, port.name.clone());
        }
    }

    pub fn name(&self, id: PortId) -> Option<&str> {
        self.names.get(&id).map(|s| s.as_ref())
    }

    /// Name or fallback to "port:N" format.
    pub fn display(&self, id: PortId) -> String {
        match self.names.get(&id) {
            Some(name) => name.to_string(),
            None => format!("port:{}", id.0),
        }
    }
}

/// How often a port expects to send or receive signals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortRate {
    /// 44.1 kHz — audio thread.
    Audio,
    /// ~60 Hz — frame rate / control rate.
    Block,
    /// ~2–10 Hz — LLM inference rate.
    Llm,
    /// Sporadic — triggers, key presses, state changes.
    Event,
}

/// Direction distinguishes input ports from output ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}

/// A typed port on a Module — the connection point for signals.
///
/// Each port gets a unique `PortId` (auto-assigned u32) and keeps
/// its human-readable name for schema introspection and registry population.
#[derive(Clone, Debug)]
pub struct Port {
    pub id: PortId,
    pub name: Arc<str>,
    pub direction: PortDirection,
    pub signal_type: SignalType,
    pub rate: PortRate,
    /// Optional min/max range for Float signals.
    pub range: Option<(f32, f32)>,
    pub description: String,
}

impl Port {
    pub fn input(name: &str, signal_type: SignalType, rate: PortRate) -> Self {
        Self {
            id: PortId::next(),
            name: Arc::from(name),
            direction: PortDirection::Input,
            signal_type,
            rate,
            range: None,
            description: String::new(),
        }
    }

    pub fn output(name: &str, signal_type: SignalType, rate: PortRate) -> Self {
        Self {
            id: PortId::next(),
            name: Arc::from(name),
            direction: PortDirection::Output,
            signal_type,
            rate,
            range: None,
            description: String::new(),
        }
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.range = Some((min, max));
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Check if a signal is compatible with this port's declared type.
    pub fn accepts(&self, signal: &super::signal::Signal) -> bool {
        signal.matches_type(&self.signal_type)
    }
}
