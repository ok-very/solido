//! ProcessChain — ordered list of signal transforms applied outside
//! the AffinityGraph. No Hebbian learning, no weights, deterministic order.
//!
//! Use cases: master EQ, velocity curves, sidechain compression, parameter
//! automation — transforms that should always be applied.

use crate::module::signal::{Signal, SignalType};
use crate::module::PortId;

/// A single processing step in a chain.
pub trait ProcessStep: Send {
    /// Transform a signal. Return None to suppress delivery entirely.
    fn process(&mut self, signal: Signal) -> Option<Signal>;

    /// What signal type this step accepts.
    fn accepts(&self) -> SignalType;

    /// Human-readable name for debug/UI.
    fn name(&self) -> &str;
}

/// When in the tick cycle the chain runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainPlacement {
    /// Before AffinityGraph routing — normalize, gate, or inject signals.
    PreRoute,
    /// After routing, at delivery — master EQ, limiter, sidechain.
    PostRoute,
}

/// An ordered chain of transforms applied to signals matching a type.
pub struct ProcessChain {
    name: String,
    signal_type: SignalType,
    steps: Vec<Box<dyn ProcessStep>>,
    placement: ChainPlacement,
    /// If true, chain runs on ALL signals of this type.
    /// If false, chain runs only on signals targeting specific ports.
    global: bool,
    /// Port filter: only apply to signals targeting these ports (when !global).
    target_ports: Vec<PortId>,
}

impl ProcessChain {
    /// Create a chain for a given signal type and placement.
    pub fn new(name: impl Into<String>, signal_type: SignalType, placement: ChainPlacement) -> Self {
        Self {
            name: name.into(),
            signal_type,
            steps: Vec::new(),
            placement,
            global: true,
            target_ports: Vec::new(),
        }
    }

    /// Add a processing step to the end of the chain.
    pub fn with_step(mut self, step: Box<dyn ProcessStep>) -> Self {
        self.steps.push(step);
        self
    }

    /// Restrict this chain to specific target ports (instead of all signals of this type).
    pub fn for_ports(mut self, ports: Vec<PortId>) -> Self {
        self.global = false;
        self.target_ports = ports;
        self
    }

    /// Apply all steps in order. Returns None if any step suppresses.
    pub fn apply(&mut self, signal: Signal) -> Option<Signal> {
        let mut sig = signal;
        for step in &mut self.steps {
            sig = step.process(sig)?;
        }
        Some(sig)
    }

    /// Check if this chain should apply to a signal of the given type targeting a port.
    pub fn applies_to(&self, signal_type: &SignalType, target_port: PortId) -> bool {
        if *signal_type != self.signal_type {
            return false;
        }
        if self.global {
            return true;
        }
        self.target_ports.contains(&target_port)
    }

    pub fn placement(&self) -> ChainPlacement {
        self.placement
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Collection of process chains with PreRoute/PostRoute separation.
pub struct ProcessChainSet {
    chains: Vec<ProcessChain>,
}

impl ProcessChainSet {
    pub fn new() -> Self {
        Self {
            chains: Vec::new(),
        }
    }

    /// Register a chain.
    pub fn add(&mut self, chain: ProcessChain) {
        self.chains.push(chain);
    }

    /// Apply all PreRoute chains to a signal. Returns the (possibly transformed) signal,
    /// or None if any chain suppressed it.
    pub fn apply_pre_route(&mut self, signal: Signal, signal_type: &SignalType) -> Option<Signal> {
        let mut sig = signal;
        for chain in &mut self.chains {
            if chain.placement == ChainPlacement::PreRoute && chain.signal_type == *signal_type && chain.global {
                sig = chain.apply(sig)?;
            }
        }
        Some(sig)
    }

    /// Apply all PostRoute chains targeting a specific port. Returns the (possibly transformed)
    /// signal, or None if any chain suppressed it.
    pub fn apply_post_route(
        &mut self,
        signal: Signal,
        signal_type: &SignalType,
        target_port: PortId,
    ) -> Option<Signal> {
        let mut sig = signal;
        for chain in &mut self.chains {
            if chain.placement == ChainPlacement::PostRoute
                && chain.applies_to(signal_type, target_port)
            {
                sig = chain.apply(sig)?;
            }
        }
        Some(sig)
    }

    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }

    pub fn len(&self) -> usize {
        self.chains.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DoubleFloat;
    impl ProcessStep for DoubleFloat {
        fn process(&mut self, signal: Signal) -> Option<Signal> {
            if let Signal::Float(v) = signal {
                Some(Signal::Float(v * 2.0))
            } else {
                Some(signal)
            }
        }
        fn accepts(&self) -> SignalType {
            SignalType::Float
        }
        fn name(&self) -> &str {
            "double"
        }
    }

    struct ClampFloat {
        min: f32,
        max: f32,
    }
    impl ProcessStep for ClampFloat {
        fn process(&mut self, signal: Signal) -> Option<Signal> {
            if let Signal::Float(v) = signal {
                Some(Signal::Float(v.clamp(self.min, self.max)))
            } else {
                Some(signal)
            }
        }
        fn accepts(&self) -> SignalType {
            SignalType::Float
        }
        fn name(&self) -> &str {
            "clamp"
        }
    }

    struct SuppressAbove {
        threshold: f32,
    }
    impl ProcessStep for SuppressAbove {
        fn process(&mut self, signal: Signal) -> Option<Signal> {
            if let Signal::Float(v) = signal {
                if v > self.threshold {
                    None
                } else {
                    Some(signal)
                }
            } else {
                Some(signal)
            }
        }
        fn accepts(&self) -> SignalType {
            SignalType::Float
        }
        fn name(&self) -> &str {
            "suppress"
        }
    }

    #[test]
    fn chain_transforms_signal() {
        let mut chain =
            ProcessChain::new("test", SignalType::Float, ChainPlacement::PostRoute)
                .with_step(Box::new(DoubleFloat))
                .with_step(Box::new(ClampFloat {
                    min: 0.0,
                    max: 1.0,
                }));

        // 0.3 → double → 0.6 → clamp → 0.6
        let result = chain.apply(Signal::Float(0.3)).unwrap();
        if let Signal::Float(v) = result {
            assert!((v - 0.6).abs() < f32::EPSILON);
        } else {
            panic!("expected Float");
        }

        // 0.8 → double → 1.6 → clamp → 1.0
        let result = chain.apply(Signal::Float(0.8)).unwrap();
        if let Signal::Float(v) = result {
            assert!((v - 1.0).abs() < f32::EPSILON);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn chain_suppresses_on_none() {
        let mut chain =
            ProcessChain::new("gate", SignalType::Float, ChainPlacement::PreRoute)
                .with_step(Box::new(SuppressAbove { threshold: 0.5 }));

        assert!(chain.apply(Signal::Float(0.3)).is_some());
        assert!(chain.apply(Signal::Float(0.8)).is_none());
    }

    #[test]
    fn chain_port_filter() {
        let chain = ProcessChain::new("filtered", SignalType::Float, ChainPlacement::PostRoute)
            .for_ports(vec![PortId(10), PortId(20)]);

        assert!(chain.applies_to(&SignalType::Float, PortId(10)));
        assert!(chain.applies_to(&SignalType::Float, PortId(20)));
        assert!(!chain.applies_to(&SignalType::Float, PortId(30)));
        // Wrong type
        assert!(!chain.applies_to(&SignalType::Bool, PortId(10)));
    }

    #[test]
    fn chain_placement() {
        let pre = ProcessChain::new("pre", SignalType::Float, ChainPlacement::PreRoute);
        let post = ProcessChain::new("post", SignalType::Float, ChainPlacement::PostRoute);
        assert_eq!(pre.placement(), ChainPlacement::PreRoute);
        assert_eq!(post.placement(), ChainPlacement::PostRoute);
    }

    #[test]
    fn chain_set_pre_and_post() {
        let mut set = ProcessChainSet::new();
        assert!(set.is_empty());

        set.add(
            ProcessChain::new("pre-double", SignalType::Float, ChainPlacement::PreRoute)
                .with_step(Box::new(DoubleFloat)),
        );
        set.add(
            ProcessChain::new("post-clamp", SignalType::Float, ChainPlacement::PostRoute)
                .with_step(Box::new(ClampFloat {
                    min: 0.0,
                    max: 1.0,
                })),
        );
        assert_eq!(set.len(), 2);

        // PreRoute: 0.3 → double → 0.6
        let result = set
            .apply_pre_route(Signal::Float(0.3), &SignalType::Float)
            .unwrap();
        if let Signal::Float(v) = result {
            assert!((v - 0.6).abs() < f32::EPSILON);
        }

        // PostRoute: 1.5 → clamp → 1.0
        let result = set
            .apply_post_route(Signal::Float(1.5), &SignalType::Float, PortId(0))
            .unwrap();
        if let Signal::Float(v) = result {
            assert!((v - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn chain_set_no_chains_passthrough() {
        let mut set = ProcessChainSet::new();
        // No chains → signal passes through unchanged
        let result = set
            .apply_pre_route(Signal::Float(42.0), &SignalType::Float)
            .unwrap();
        if let Signal::Float(v) = result {
            assert!((v - 42.0).abs() < f32::EPSILON);
        }
    }
}
