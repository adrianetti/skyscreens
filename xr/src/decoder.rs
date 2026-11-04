//! Hardware H.264 decoding with MediaCodec, straight into the screen's OpenXR surface layer.
//! The compositor shows the newest decoded frame without waiting for the app's frame loop.

use crate::Result;
use ndk::media::media_codec::{DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection};
use ndk::media::media_format::MediaFormat;
use ndk::native_window::NativeWindow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// AMediaCodec may be fed and drained from different threads.
struct Codec(MediaCodec);
unsafe impl Send for Codec {}
unsafe impl Sync for Codec {}

pub struct Decoder {
    codec: Arc<Codec>,
    stop: Arc<AtomicBool>,
    output: Option<JoinHandle<()>>,
}

impl Decoder {
    /// Configures from a keyframe's SPS and PPS. `on_frame` gets each shown frame's timestamp.
    pub fn new(window: &NativeWindow, width: i32, height: i32, keyframe: &[u8], on_frame: impl Fn(u64) + Send + 'static) -> Result<Decoder> {
        let (sps, pps) = parameter_sets(keyframe).ok_or("keyframe without SPS and PPS")?;
        let codec = MediaCodec::from_decoder_type("video/avc").ok_or("no H.264 decoder")?;
        let mut format = MediaFormat::new();
        format.set_str("mime", "video/avc");
        format.set_i32("width", width);
        format.set_i32("height", height);
        format.set_buffer("csd-0", sps);
        format.set_buffer("csd-1", pps);
        // Same low-latency settings ALVR uses on Quest.
        format.set_i32("operating-rate", i16::MAX as i32);
        format.set_i32("priority", 0);
        format.set_i32("vendor.qti-ext-dec-low-latency.enable", 1);
        codec.configure(&format, Some(window), MediaCodecDirection::Decoder)?;
        codec.start()?;

        let codec = Arc::new(Codec(codec));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, s) = (codec.clone(), stop.clone());
        let output = std::thread::Builder::new().name("decode-out".into()).spawn(move || {
            while !s.load(Ordering::Relaxed) {
                match c.0.dequeue_output_buffer(Duration::from_millis(5)) {
                    Ok(DequeuedOutputBufferInfoResult::Buffer(b)) => {
                        let pts = b.info().presentation_time_us() as u64;
                        if c.0.release_output_buffer(b, true).is_ok() {
                            on_frame(pts);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("decoder output: {e}");
                        break;
                    }
                }
            }
        })?;
        Ok(Decoder { codec, stop, output: Some(output) })
    }

    /// Queues one Annex B access unit. Returns false if the decoder had no free input buffer.
    pub fn feed(&self, data: &[u8], pts_us: u64) -> Result<bool> {
        match self.codec.0.dequeue_input_buffer(Duration::from_millis(10))? {
            DequeuedInputBufferResult::Buffer(mut b) => {
                let dst = b.buffer_mut();
                if data.len() > dst.len() {
                    return Err("frame larger than decoder input buffer".into());
                }
                for (d, s) in dst.iter_mut().zip(data) {
                    d.write(*s);
                }
                self.codec.0.queue_input_buffer(b, 0, data.len(), pts_us, 0)?;
                Ok(true)
            }
            DequeuedInputBufferResult::TryAgainLater => Ok(false),
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.output.take() {
            let _ = t.join();
        }
        let _ = self.codec.0.stop();
    }
}

/// Finds the SPS and PPS NAL units (with start codes) in an Annex B keyframe.
fn parameter_sets(au: &[u8]) -> Option<(&[u8], &[u8])> {
    let starts: Vec<usize> = (0..au.len().saturating_sub(3)).filter(|&i| au[i..i + 4] == [0, 0, 0, 1]).collect();
    let mut sps = None;
    let mut pps = None;
    for (n, &s) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(au.len());
        match au.get(s + 4).map(|b| b & 0x1f) {
            Some(7) => sps = Some(&au[s..end]),
            Some(8) => pps = Some(&au[s..end]),
            _ => {}
        }
    }
    Some((sps?, pps?))
}
