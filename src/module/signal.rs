#![allow(dead_code)]
use std::sync::Arc;

/// Every signal type that can flow between Modules through the affinity graph.
///
/// Video-ready types (FrameRef, Embedding, PixelSample) exist from day one
/// so camera/LLM modules wire in without protocol changes.
#[derive(Debug, Clone)]
pub enum Signal {
    Float(f32),
    Bool(bool),
    /// Stateless bang — no payload, just "now".
    Trigger,
    Text(Arc<str>),
    /// Block-rate audio snapshot (typically 256–1024 samples).
    AudioBlock(Arc<Vec<f32>>),
    /// Shared ref to a video frame — zero-copy via Arc.
    FrameRef(Arc<FrameBuffer>),
    /// Projected vector (e.g. LLaVA embedding reduced to 4D).
    Embedding([f32; 4]),
    /// RGBA at a single point (cursor probe, pixel sample).
    PixelSample([f32; 4]),
    /// Arbitrary-length control pattern (euclidean rhythms, gravity weights).
    Pattern(Arc<Vec<f32>>),
}

/// Discriminant tag for Signal — used in port schemas and error reporting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignalType {
    Float,
    Bool,
    Trigger,
    Text,
    AudioBlock,
    FrameRef,
    Embedding,
    PixelSample,
    Pattern,
}

impl Signal {
    pub fn signal_type(&self) -> SignalType {
        match self {
            Signal::Float(_) => SignalType::Float,
            Signal::Bool(_) => SignalType::Bool,
            Signal::Trigger => SignalType::Trigger,
            Signal::Text(_) => SignalType::Text,
            Signal::AudioBlock(_) => SignalType::AudioBlock,
            Signal::FrameRef(_) => SignalType::FrameRef,
            Signal::Embedding(_) => SignalType::Embedding,
            Signal::PixelSample(_) => SignalType::PixelSample,
            Signal::Pattern(_) => SignalType::Pattern,
        }
    }

    pub fn matches_type(&self, expected: &SignalType) -> bool {
        self.signal_type() == *expected
    }

    /// Scalar magnitude for affinity impact tracking.
    ///
    /// Trigger = 1.0, Bool = 0/1, Float = abs value,
    /// AudioBlock/Pattern = RMS, Embedding/PixelSample = L2 norm,
    /// FrameRef = 1.0 (always "present"), Text = length / 100.
    pub fn magnitude(&self) -> f32 {
        match self {
            Signal::Float(v) => v.abs(),
            Signal::Bool(b) => if *b { 1.0 } else { 0.0 },
            Signal::Trigger => 1.0,
            Signal::Text(s) => s.len() as f32 / 100.0,
            Signal::AudioBlock(buf) => {
                if buf.is_empty() {
                    return 0.0;
                }
                let sum_sq: f32 = buf.iter().map(|s| s * s).sum();
                (sum_sq / buf.len() as f32).sqrt()
            }
            Signal::FrameRef(_) => 1.0,
            Signal::Embedding(e) => {
                let sum_sq: f32 = e.iter().map(|v| v * v).sum();
                sum_sq.sqrt()
            }
            Signal::PixelSample(p) => {
                let sum_sq: f32 = p.iter().map(|v| v * v).sum();
                sum_sq.sqrt()
            }
            Signal::Pattern(p) => {
                if p.is_empty() {
                    return 0.0;
                }
                let sum_sq: f32 = p.iter().map(|v| v * v).sum();
                (sum_sq / p.len() as f32).sqrt()
            }
        }
    }
}

impl std::fmt::Display for SignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A video frame buffer shared between modules via `Arc<FrameBuffer>`.
///
/// Allows camera, video file, screen capture, and LLaVA output to share
/// frames without copying. CPU pixels are always present; GPU texture is
/// uploaded on demand by the renderer.
#[derive(Debug)]
pub struct FrameBuffer {
    /// CPU-side RGBA8 pixel data.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: f64,
    /// Optional GPU texture handle for zero-copy rendering.
    pub gpu_texture: Option<Arc<wgpu::Texture>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_type_matches() {
        let s = Signal::Float(1.0);
        assert!(s.matches_type(&SignalType::Float));
        assert!(!s.matches_type(&SignalType::Bool));
    }

    #[test]
    fn trigger_type() {
        let s = Signal::Trigger;
        assert_eq!(s.signal_type(), SignalType::Trigger);
        assert_eq!(s.magnitude(), 1.0);
    }

    #[test]
    fn frame_ref_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Arc<FrameBuffer>>();
        assert_sync::<Arc<FrameBuffer>>();
    }

    #[test]
    fn audio_block_rms() {
        let s = Signal::AudioBlock(Arc::new(vec![0.5, -0.5, 0.5, -0.5]));
        assert!((s.magnitude() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn embedding_magnitude() {
        let s = Signal::Embedding([1.0, 0.0, 0.0, 0.0]);
        assert!((s.magnitude() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn empty_pattern_magnitude() {
        let s = Signal::Pattern(Arc::new(vec![]));
        assert_eq!(s.magnitude(), 0.0);
    }
}
