use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Index into a `Vec<Shared>` on the audio thread.
/// Replaces `HashMap<String, Shared>` lookups with direct array indexing (~3 cycles vs ~80).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleId(pub u16);

/// Lock-free shared parameter handle.
///
/// Drop-in replacement for `fundsp::prelude32::Shared`.
/// Stores an `f32` as atomic `u32` bits via `Arc<AtomicU32>`.
/// Clone is O(1).
#[derive(Clone)]
pub struct Shared {
    inner: Arc<AtomicU32>,
}

impl Shared {
    pub fn new(initial: f32) -> Self {
        Self {
            inner: Arc::new(AtomicU32::new(initial.to_bits())),
        }
    }

    pub fn value(&self) -> f32 {
        f32::from_bits(self.inner.load(Ordering::Relaxed))
    }

    pub fn set(&self, value: f32) {
        self.inner.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// Create a new shared parameter with the given initial value.
pub fn shared(initial: f32) -> Shared {
    Shared::new(initial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_roundtrip() {
        let s = shared(42.5);
        assert!((s.value() - 42.5).abs() < f32::EPSILON);
        s.set(0.0);
        assert!((s.value()).abs() < f32::EPSILON);
    }

    #[test]
    fn shared_clone_shares_state() {
        let a = shared(1.0);
        let b = a.clone();
        a.set(99.0);
        assert!((b.value() - 99.0).abs() < f32::EPSILON);
    }

    #[test]
    fn shared_negative_values() {
        let s = shared(-3.14);
        assert!((s.value() - (-3.14)).abs() < 0.001);
    }
}
