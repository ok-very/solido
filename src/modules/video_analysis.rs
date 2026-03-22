//! Video analysis module — extracts perceptual features from video frames.
//!
//! Decodes video on a background thread, computes brightness, warmth,
//! motion energy, and edge density at 30Hz. Emits as Float signals through
//! the affinity graph. Organisms learn which visual features are musically
//! useful via Hebbian feedback.

use std::path::Path;
use std::sync::Arc;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema, ModuleTier};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};
use crate::substrate::video::{FrameBuffer, VideoDecoder};

// ─── Feature Extraction ──────────────────────────────────────────────

/// Compute mean luminance [0, 1] from RGB24 buffer.
fn compute_brightness(pixels: &[u8]) -> f32 {
    let count = pixels.len() / 3;
    if count == 0 {
        return 0.0;
    }
    let sum: f32 = pixels
        .chunks_exact(3)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .sum();
    sum / (count as f32 * 255.0)
}

/// Compute warmth (red-blue ratio) [-1, 1] from RGB24 buffer.
fn compute_warmth(pixels: &[u8]) -> f32 {
    let count = pixels.len() / 3;
    if count == 0 {
        return 0.0;
    }
    let mut r_sum: f32 = 0.0;
    let mut b_sum: f32 = 0.0;
    for p in pixels.chunks_exact(3) {
        r_sum += p[0] as f32;
        b_sum += p[2] as f32;
    }
    ((r_sum - b_sum) / (count as f32 * 255.0)).clamp(-1.0, 1.0)
}

/// Compute motion energy [0, 1] from frame difference.
fn compute_motion_energy(current: &[f32], previous: &[f32]) -> f32 {
    if current.len() != previous.len() || current.is_empty() {
        return 0.0;
    }
    let n = current.len() as f32;
    let sum_sq: f32 = current
        .iter()
        .zip(previous.iter())
        .map(|(a, b)| {
            let d = a - b;
            d * d
        })
        .sum();
    let rms = (sum_sq / n).sqrt();
    (rms / 64.0).clamp(0.0, 1.0)
}

/// Compute edge density [0, 1] via simplified Sobel on luminance buffer.
fn compute_edge_density(luma: &[f32], w: usize, h: usize) -> f32 {
    if w < 3 || h < 3 {
        return 0.0;
    }
    let mut edge_sum: f32 = 0.0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let idx = |yy: usize, xx: usize| luma[yy * w + xx];
            let gx = -idx(y - 1, x - 1) + idx(y - 1, x + 1) - 2.0 * idx(y, x - 1)
                + 2.0 * idx(y, x + 1)
                - idx(y + 1, x - 1)
                + idx(y + 1, x + 1);
            let gy = -idx(y - 1, x - 1) - 2.0 * idx(y - 1, x) - idx(y - 1, x + 1)
                + idx(y + 1, x - 1)
                + 2.0 * idx(y + 1, x)
                + idx(y + 1, x + 1);
            edge_sum += gx.abs() + gy.abs();
        }
    }
    (edge_sum / ((w * h) as f32 * 512.0)).clamp(0.0, 1.0)
}

/// Convert RGB24 buffer to luminance [0, 255] floats.
fn rgb_to_luma(pixels: &[u8]) -> Vec<f32> {
    pixels
        .chunks_exact(3)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect()
}

// ─── Module ──────────────────────────────────────────────────────────

pub struct VideoAnalysisModule {
    schema: ModuleSchema,
    decoder: Option<VideoDecoder>,

    // Feature outputs
    brightness: f32,
    warmth: f32,
    motion_energy: f32,
    edge_density: f32,

    // State
    prev_luma: Vec<f32>,
    frames_processed: u64,
    /// Latest decoded frame retained for GPU texture upload.
    latest_frame: Option<Arc<FrameBuffer>>,

    // Ports
    brightness_port: PortId,
    warmth_port: PortId,
    motion_port: PortId,
    edge_port: PortId,
    source_path_port: PortId,
}

impl VideoAnalysisModule {
    /// Current video features for broadcast to organism DSP cells.
    pub fn features(&self) -> (f32, f32, f32, f32) {
        (self.brightness, self.warmth, self.motion_energy, self.edge_density)
    }

    /// Number of video frames processed since start.
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Whether a video decoder is active.
    pub fn is_active(&self) -> bool {
        self.decoder.is_some()
    }

    /// Latest decoded frame for GPU texture upload. RGB24, row-major.
    pub fn latest_frame(&self) -> Option<&Arc<FrameBuffer>> {
        self.latest_frame.as_ref()
    }

    pub fn new(video_path: Option<&str>) -> Self {
        let brightness_out = Port::output("brightness", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Mean frame luminance");
        let warmth_out = Port::output("warmth", SignalType::Float, PortRate::Block)
            .with_range(-1.0, 1.0)
            .with_description("Red-blue color temperature");
        let motion_out = Port::output("motion_energy", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Frame-to-frame motion RMS");
        let edge_out = Port::output("edge_density", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Spatial edge density (Sobel)");
        let source_in = Port::input("source_path", SignalType::Text, PortRate::Event)
            .with_description("Video file path to load");

        let brightness_port = brightness_out.id;
        let warmth_port = warmth_out.id;
        let motion_port = motion_out.id;
        let edge_port = edge_out.id;
        let source_path_port = source_in.id;

        let schema = ModuleSchema::new("video_analysis", ModuleCategory::Input)
            .with_description("Extracts perceptual features from video at 30Hz")
            .with_tier(ModuleTier::Infrastructure)
            .with_output(brightness_out)
            .with_output(warmth_out)
            .with_output(motion_out)
            .with_output(edge_out)
            .with_input(source_in);

        let decoder = video_path.and_then(|p| {
            // Initialize ffmpeg once
            let _ = ffmpeg_next::init();
            VideoDecoder::start(Path::new(p), true)
        });

        Self {
            schema,
            decoder,
            brightness: 0.0,
            warmth: 0.0,
            motion_energy: 0.0,
            edge_density: 0.0,
            prev_luma: Vec::new(),
            frames_processed: 0,
            latest_frame: None,
            brightness_port,
            warmth_port,
            motion_port,
            edge_port,
            source_path_port,
        }
    }

    fn process_frame(&mut self, frame: &FrameBuffer) {
        let w = frame.width as usize;
        let h = frame.height as usize;

        self.brightness = compute_brightness(&frame.pixels);
        self.warmth = compute_warmth(&frame.pixels);

        let luma = rgb_to_luma(&frame.pixels);

        if self.prev_luma.len() == luma.len() {
            self.motion_energy = compute_motion_energy(&luma, &self.prev_luma);
        }

        self.edge_density = compute_edge_density(&luma, w, h);

        self.prev_luma = luma;
        self.frames_processed += 1;
    }
}

impl ModuleCore for VideoAnalysisModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.brightness_port, Signal::Float(self.brightness)));
        buffer.push((self.warmth_port, Signal::Float(self.warmth)));
        buffer.push((self.motion_port, Signal::Float(self.motion_energy)));
        buffer.push((self.edge_port, Signal::Float(self.edge_density)));
    }

    fn receive_signal(&mut self, port: PortId, signal: Signal) -> Result<(), SignalError> {
        if port == self.source_path_port {
            if let Signal::Text(path) = signal {
                log::info!("VideoAnalysis: loading new source: {path}");
                // Stop existing decoder
                if let Some(ref mut dec) = self.decoder {
                    dec.stop();
                }
                let _ = ffmpeg_next::init();
                self.decoder = VideoDecoder::start(Path::new(&*path), true);
                self.prev_luma.clear();
                self.frames_processed = 0;
                return Ok(());
            }
            return Err(SignalError::WrongType {
                expected: SignalType::Text,
                got: signal.signal_type(),
            });
        }
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // Drain the latest frame from the decoder (skip old ones)
        if let Some(ref mut decoder) = self.decoder {
            let mut latest: Option<Arc<FrameBuffer>> = None;
            while let Some(frame) = decoder.frame_rx.try_recv() {
                latest = Some(frame);
            }
            if let Some(frame) = latest {
                self.process_frame(&frame);
                self.latest_frame = Some(frame);
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TW: usize = 32;
    const TH: usize = 24;
    const TPIX: usize = TW * TH * 3;

    fn white_frame() -> Vec<u8> {
        vec![255u8; TPIX]
    }

    fn black_frame() -> Vec<u8> {
        vec![0u8; TPIX]
    }

    fn red_frame() -> Vec<u8> {
        let mut pixels = vec![0u8; TPIX];
        for p in pixels.chunks_exact_mut(3) {
            p[0] = 255;
            p[1] = 0;
            p[2] = 0;
        }
        pixels
    }

    fn blue_frame() -> Vec<u8> {
        let mut pixels = vec![0u8; TPIX];
        for p in pixels.chunks_exact_mut(3) {
            p[0] = 0;
            p[1] = 0;
            p[2] = 255;
        }
        pixels
    }

    #[test]
    fn brightness_white() {
        let b = compute_brightness(&white_frame());
        assert!((b - 1.0).abs() < 0.01, "white should be ~1.0: {b}");
    }

    #[test]
    fn brightness_black() {
        let b = compute_brightness(&black_frame());
        assert!(b.abs() < 0.01, "black should be ~0.0: {b}");
    }

    #[test]
    fn warmth_red_positive() {
        let w = compute_warmth(&red_frame());
        assert!(w > 0.5, "red frame should be warm: {w}");
    }

    #[test]
    fn warmth_blue_negative() {
        let w = compute_warmth(&blue_frame());
        assert!(w < -0.5, "blue frame should be cool: {w}");
    }

    #[test]
    fn warmth_white_neutral() {
        let w = compute_warmth(&white_frame());
        assert!(w.abs() < 0.01, "white should be neutral warmth: {w}");
    }

    #[test]
    fn motion_identical_frames() {
        let luma = rgb_to_luma(&white_frame());
        let m = compute_motion_energy(&luma, &luma);
        assert!(m.abs() < 0.001, "identical frames should have zero motion: {m}");
    }

    #[test]
    fn motion_different_frames() {
        let a = rgb_to_luma(&black_frame());
        let b = rgb_to_luma(&white_frame());
        let m = compute_motion_energy(&b, &a);
        assert!(m > 0.5, "black→white should have high motion: {m}");
    }

    #[test]
    fn edge_density_flat() {
        let luma = rgb_to_luma(&white_frame());
        let e = compute_edge_density(&luma, TW, TH);
        assert!(e.abs() < 0.01, "flat frame should have zero edges: {e}");
    }

    #[test]
    fn edge_density_gradient() {
        let mut pixels = vec![0u8; TPIX];
        for y in 0..TH {
            for x in 0..TW {
                let v = (x * 255 / TW) as u8;
                let i = (y * TW + x) * 3;
                pixels[i] = v;
                pixels[i + 1] = v;
                pixels[i + 2] = v;
            }
        }
        let luma = rgb_to_luma(&pixels);
        let e = compute_edge_density(&luma, TW, TH);
        assert!(e > 0.01, "gradient should have edges: {e}");
    }

    #[test]
    fn module_creates_without_video() {
        let module = VideoAnalysisModule::new(None);
        assert_eq!(module.schema().name, "video_analysis");
        assert_eq!(module.brightness, 0.0);
    }
}
