#![expect(unsafe_code, reason = "AMediaCodec is thread-safe, but the ndk wrapper does not say so")]

use crate::Result;
use link::annexb;
use ndk::media::media_codec::{
    DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::native_window::NativeWindow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

const MIME_H264: &str = "video/avc";
const LOW_LATENCY_OPTIONS: [(&str, i32); 3] =
    [("operating-rate", 32_767), ("priority", 0), ("vendor.qti-ext-dec-low-latency.enable", 1)];
const OUTPUT_POLL: Duration = Duration::from_millis(5);
const INPUT_WAIT: Duration = Duration::from_millis(10);

struct ThreadSafeCodec(MediaCodec);

unsafe impl Send for ThreadSafeCodec {}
unsafe impl Sync for ThreadSafeCodec {}

pub(crate) struct Decoder {
    codec: Arc<ThreadSafeCodec>,
    stop: Arc<AtomicBool>,
    output: Option<JoinHandle<()>>,
}

impl Decoder {
    pub(crate) fn new(
        window: &NativeWindow,
        width: i32,
        height: i32,
        keyframe: &[u8],
        on_frame: impl Fn(u64) + Send + 'static,
    ) -> Result<Self> {
        let (sps, pps) = annexb::parameter_sets(keyframe).ok_or("keyframe without SPS and PPS")?;
        let codec = MediaCodec::from_decoder_type(MIME_H264).ok_or("no H.264 decoder")?;
        let mut format = MediaFormat::new();
        format.set_str("mime", MIME_H264);
        format.set_i32("width", width);
        format.set_i32("height", height);
        format.set_buffer("csd-0", sps);
        format.set_buffer("csd-1", pps);
        for (key, value) in LOW_LATENCY_OPTIONS {
            format.set_i32(key, value);
        }
        codec.configure(&format, Some(window), MediaCodecDirection::Decoder)?;
        codec.start()?;

        let codec = Arc::new(ThreadSafeCodec(codec));
        let stop = Arc::new(AtomicBool::new(false));
        let output = {
            let (codec, stop) = (Arc::clone(&codec), Arc::clone(&stop));
            std::thread::Builder::new()
                .name("decode-out".into())
                .spawn(move || release_frames_until_stopped(&codec, &stop, on_frame))?
        };
        Ok(Self { codec, stop, output: Some(output) })
    }

    pub(crate) fn feed(&self, data: &[u8], pts_us: u64) -> Result<bool> {
        match self.codec.0.dequeue_input_buffer(INPUT_WAIT)? {
            DequeuedInputBufferResult::Buffer(mut input) => {
                let dst = input.buffer_mut();
                if data.len() > dst.len() {
                    return Err("frame larger than decoder input buffer".into());
                }
                for (slot, byte) in dst.iter_mut().zip(data) {
                    slot.write(*byte);
                }
                self.codec.0.queue_input_buffer(input, 0, data.len(), pts_us, 0)?;
                Ok(true)
            }
            DequeuedInputBufferResult::TryAgainLater => Ok(false),
        }
    }
}

fn release_frames_until_stopped(codec: &ThreadSafeCodec, stop: &AtomicBool, on_frame: impl Fn(u64)) {
    while !stop.load(Ordering::Relaxed) {
        match codec.0.dequeue_output_buffer(OUTPUT_POLL) {
            Ok(DequeuedOutputBufferInfoResult::Buffer(output)) => {
                let pts = u64::try_from(output.info().presentation_time_us()).unwrap_or_default();
                if codec.0.release_output_buffer(output, true).is_ok() {
                    on_frame(pts);
                }
            }
            Ok(_) => {}
            Err(error) => {
                log::warn!("decoder output: {error}");
                break;
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.output.take()
            && thread.join().is_err()
        {
            log::error!("decoder output thread panicked");
        }
        if let Err(error) = self.codec.0.stop() {
            log::warn!("decoder stop: {error}");
        }
    }
}
