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

/// Check if two ports have compatible value ranges for edge discovery.
///
/// If both ports declare a range and the ranges don't overlap, the edge
/// is rejected. This prevents spurious Float→Float connections like
/// raw_pitch [0,1] → pitch_hz [20,20000].
///
/// If either port lacks a range, the connection is allowed (backward compat).
pub fn ranges_compatible(out: &Port, inp: &Port) -> bool {
    match (out.range, inp.range) {
        (Some((o_min, o_max)), Some((i_min, i_max))) => {
            o_max >= i_min && i_max >= o_min
        }
        _ => true,
    }
}

/// Check if two ports have compatible rates for edge discovery.
///
/// Block-rate outputs (continuous streams) should not flood Event-rate
/// inputs (discrete triggers/changes). All other combinations are fine:
/// - Event → Event: discrete signals match
/// - Event → Block: events can modulate continuous params
/// - Block → Block: continuous streams match
/// - Block → Event: REJECTED — continuous data overwhelms event inputs
pub fn rates_compatible(out: &Port, inp: &Port) -> bool {
    if inp.rate == PortRate::Event && out.rate != PortRate::Event {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float_port(name: &str, dir: PortDirection, range: Option<(f32, f32)>) -> Port {
        let mut p = match dir {
            PortDirection::Output => Port::output(name, SignalType::Float, PortRate::Block),
            PortDirection::Input => Port::input(name, SignalType::Float, PortRate::Block),
        };
        p.range = range;
        p
    }

    #[test]
    fn ranges_no_overlap_rejected() {
        let out = float_port("raw_pitch", PortDirection::Output, Some((0.0, 1.0)));
        let inp = float_port("pitch_hz", PortDirection::Input, Some((20.0, 20000.0)));
        assert!(!ranges_compatible(&out, &inp));
    }

    #[test]
    fn ranges_overlap_accepted() {
        let out = float_port("pitch_hz", PortDirection::Output, Some((20.0, 20000.0)));
        let inp = float_port("pitch_hz", PortDirection::Input, Some((20.0, 20000.0)));
        assert!(ranges_compatible(&out, &inp));
    }

    #[test]
    fn ranges_partial_overlap_accepted() {
        let out = float_port("gravity_delta", PortDirection::Output, Some((-1.0, 1.0)));
        let inp = float_port("amplitude", PortDirection::Input, Some((0.0, 1.0)));
        assert!(ranges_compatible(&out, &inp));
    }

    #[test]
    fn ranges_none_always_allowed() {
        let out = float_port("unranged", PortDirection::Output, None);
        let inp = float_port("pitch_hz", PortDirection::Input, Some((20.0, 20000.0)));
        assert!(ranges_compatible(&out, &inp));

        let out2 = float_port("raw_pitch", PortDirection::Output, Some((0.0, 1.0)));
        let inp2 = float_port("unranged", PortDirection::Input, None);
        assert!(ranges_compatible(&out2, &inp2));

        let out3 = float_port("a", PortDirection::Output, None);
        let inp3 = float_port("b", PortDirection::Input, None);
        assert!(ranges_compatible(&out3, &inp3));
    }

    #[test]
    fn ranges_adjacent_accepted() {
        // Edge case: ranges touch at boundary
        let out = float_port("a", PortDirection::Output, Some((0.0, 20.0)));
        let inp = float_port("b", PortDirection::Input, Some((20.0, 20000.0)));
        assert!(ranges_compatible(&out, &inp));
    }

    // --- Rate compatibility tests ---

    #[test]
    fn rate_event_to_event_ok() {
        let out = Port::output("trigger", SignalType::Trigger, PortRate::Event);
        let inp = Port::input("trigger", SignalType::Trigger, PortRate::Event);
        assert!(rates_compatible(&out, &inp));
    }

    #[test]
    fn rate_event_to_block_ok() {
        let out = Port::output("gravity_delta", SignalType::Float, PortRate::Event);
        let inp = Port::input("gravity_override", SignalType::Float, PortRate::Block);
        assert!(rates_compatible(&out, &inp));
    }

    #[test]
    fn rate_block_to_block_ok() {
        let out = Port::output("pitch_hz", SignalType::Float, PortRate::Block);
        let inp = Port::input("pitch_hz", SignalType::Float, PortRate::Block);
        assert!(rates_compatible(&out, &inp));
    }

    #[test]
    fn rate_block_to_event_rejected() {
        let out = Port::output("cursor_x", SignalType::Float, PortRate::Block);
        let inp = Port::input("raw_pitch", SignalType::Float, PortRate::Event);
        assert!(!rates_compatible(&out, &inp));
    }
}
