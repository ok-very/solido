//! Video decoder — decodes video files via ffmpeg-next on a background thread.
//!
//! Frames are scaled to 160x120 RGB24 for analysis, delivered via ring buffer.
//! Loops by default. Throttled to ~30fps.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::substrate::channel::{self, Receiver, Sender};

/// Max analysis dimension — the longer side scales to this,
/// the shorter side preserves aspect ratio.
pub const ANALYSIS_MAX_DIM: u32 = 160;
/// Target decode rate.
pub const DECODE_FPS: u32 = 30;

/// A decoded video frame at analysis resolution.
pub struct FrameBuffer {
    /// RGB24 pixels, row-major, 160x120.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_number: u64,
}

/// Handle to a running video decoder. Drop to stop.
pub struct VideoDecoder {
    pub frame_rx: Receiver<Arc<FrameBuffer>>,
    running: Arc<AtomicBool>,
    pub path: PathBuf,
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

impl VideoDecoder {
    /// Start decoding a video file. Returns None if file missing or ffmpeg can't open it.
    pub fn start(path: &Path, looping: bool) -> Option<Self> {
        if !path.exists() {
            log::warn!("VideoDecoder: file not found: {}", path.display());
            return None;
        }

        let running = Arc::new(AtomicBool::new(true));
        let (frame_tx, frame_rx) = channel::channel::<Arc<FrameBuffer>>(8);

        let path_owned = path.to_path_buf();
        let running_clone = running.clone();

        let handle = std::thread::Builder::new()
            .name("video-decoder".into())
            .spawn(move || {
                decode_loop(&path_owned, frame_tx, running_clone, looping);
            })
            .ok()?;

        log::info!("VideoDecoder: started for {}", path.display());

        Some(Self {
            frame_rx,
            running,
            path: path.to_path_buf(),
            reader_handle: Some(handle),
        })
    }

    /// Stop decoding.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.reader_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn decode_loop(
    path: &Path,
    mut frame_tx: Sender<Arc<FrameBuffer>>,
    running: Arc<AtomicBool>,
    looping: bool,
) {
    let frame_duration = std::time::Duration::from_millis(1000 / DECODE_FPS as u64);

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        match decode_file(path, &mut frame_tx, &running) {
            Ok(count) => {
                log::info!("VideoDecoder: decoded {count} frames from {}", path.display());
            }
            Err(e) => {
                log::warn!("VideoDecoder: decode error: {e}");
                // Wait a bit before retrying to avoid busy loop on persistent errors
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        if !looping || !running.load(Ordering::Relaxed) {
            break;
        }

        // Brief pause before loop restart
        std::thread::sleep(frame_duration);
    }
}

fn decode_file(
    path: &Path,
    frame_tx: &mut Sender<Arc<FrameBuffer>>,
    running: &Arc<AtomicBool>,
) -> Result<u64, ffmpeg_next::Error> {
    let mut ictx = ffmpeg_next::format::input(path)?;

    let stream = ictx
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let video_stream_index = stream.index();

    let context_decoder =
        ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    // Compute analysis resolution preserving aspect ratio
    let src_w = decoder.width();
    let src_h = decoder.height();
    let (out_w, out_h) = if src_w >= src_h {
        // Landscape or square
        let w = ANALYSIS_MAX_DIM;
        let h = ((src_h as f32 / src_w as f32) * w as f32).round() as u32;
        (w, h.max(2))
    } else {
        // Portrait
        let h = ANALYSIS_MAX_DIM;
        let w = ((src_w as f32 / src_h as f32) * h as f32).round() as u32;
        (w.max(2), h)
    };
    log::info!("VideoDecoder: source {}x{} → analysis {}x{}", src_w, src_h, out_w, out_h);

    let mut scaler = ffmpeg_next::software::scaling::context::Context::get(
        decoder.format(),
        src_w,
        src_h,
        ffmpeg_next::format::Pixel::RGB24,
        out_w,
        out_h,
        ffmpeg_next::software::scaling::flag::Flags::BILINEAR,
    )?;

    let frame_duration = std::time::Duration::from_millis(1000 / DECODE_FPS as u64);
    let mut frame_number: u64 = 0;
    let mut decoded = ffmpeg_next::util::frame::video::Video::empty();
    let mut rgb_frame = ffmpeg_next::util::frame::video::Video::empty();

    for (stream, packet) in ictx.packets() {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        if stream.index() != video_stream_index {
            continue;
        }

        decoder.send_packet(&packet)?;

        while decoder.receive_frame(&mut decoded).is_ok() {
            if !running.load(Ordering::Relaxed) {
                return Ok(frame_number);
            }

            let start = std::time::Instant::now();

            scaler.run(&decoded, &mut rgb_frame)?;

            let pixels = rgb_frame.data(0)[..((out_w * out_h * 3) as usize)].to_vec();
            let frame = Arc::new(FrameBuffer {
                pixels,
                width: out_w,
                height: out_h,
                frame_number,
            });
            frame_number += 1;

            // Non-blocking push — drop if consumer is behind
            let _ = frame_tx.try_send(frame);

            // Throttle to target FPS
            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }
    }

    // Flush decoder
    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        scaler.run(&decoded, &mut rgb_frame)?;
        let pixels = rgb_frame.data(0)[..((out_w * out_h * 3) as usize)].to_vec();
        let frame = Arc::new(FrameBuffer {
            pixels,
            width: out_w,
            height: out_h,
            frame_number,
        });
        frame_number += 1;
        let _ = frame_tx.try_send(frame);
    }

    Ok(frame_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_dim_reasonable() {
        assert_eq!(ANALYSIS_MAX_DIM, 160);
        // Landscape 1920x1080 → 160x90
        // Portrait 480x848 → 90x160
        // Square 500x500 → 160x160
    }
}
