use crate::decoder::Decoder;
use link::{DEFAULT_PORT, Message, PROTOCOL_VERSION, Priority, Reassembler, Screen, Sender, TCP_FRAGMENT, channel};
use ndk::native_window::NativeWindow;
use std::io::Read;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const PIXELS_PER_DEGREE: f32 = 25.0;
const REFRESH_RATE: f32 = 90.0;
const MAX_DECODERS: u8 = 16;
const MAX_LAYERS: u8 = 16;
const KEYFRAME_REQUEST_INTERVAL: Duration = Duration::from_millis(200);
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
const READ_BUFFER: usize = 256 * 1024;

#[derive(Default)]
pub struct Shared {
    sender: Mutex<Option<Sender>>,
    window: Mutex<Option<(NativeWindow, i32, i32)>>,
    decoder: Mutex<Option<Decoder>>,
    last_request: Mutex<Option<Instant>>,
}

impl Shared {
    pub fn detach_window(&self) {
        *lock(&self.decoder) = None;
        *lock(&self.window) = None;
    }

    pub fn set_window(&self, window: NativeWindow, width: i32, height: i32) {
        *lock(&self.decoder) = None;
        *lock(&self.window) = Some((window, width, height));
        self.request_keyframe();
    }

    fn send(&self, m: &Message) {
        if let Some(s) = lock(&self.sender).as_ref()
            && let Err(e) = s.send(channel::HEADSET_CONTROL, Priority::Control, m)
        {
            log::warn!("send: {e}");
        }
    }

    fn request_keyframe(&self) {
        let mut last = lock(&self.last_request);
        if last.is_none_or(|t| t.elapsed() > KEYFRAME_REQUEST_INTERVAL) {
            *last = Some(Instant::now());
            self.send(&Message::KeyframeRequest { screen: 0 });
        }
    }

    fn on_video(self: &Arc<Self>, key: bool, pts_us: u64, data: &[u8]) {
        let mut decoder = lock(&self.decoder);
        if decoder.is_none() {
            let window = lock(&self.window);
            let Some((w, width, height)) = window.as_ref() else { return };
            if !key {
                drop(window);
                drop(decoder);
                return self.request_keyframe();
            }
            match Decoder::new(w, *width, *height, data, self.pong_sender()) {
                Ok(d) => *decoder = Some(d),
                Err(e) => return log::error!("decoder: {e}"),
            }
        }
        let Some(active) = decoder.as_ref() else { return };
        match active.feed(data, pts_us) {
            Ok(true) => {}
            Ok(false) => log::warn!("decoder busy, frame dropped"),
            Err(e) => {
                log::error!("decode: {e}");
                *decoder = None;
            }
        }
    }

    fn pong_sender(self: &Arc<Self>) -> impl Fn(u64) + Send + 'static {
        let me = Arc::downgrade(self);
        move |pts| {
            if let Some(me) = me.upgrade() {
                me.send(&Message::Pong { t_us: pts });
            }
        }
    }
}

pub fn spawn(shared: Arc<Shared>, screens: mpsc::Sender<Vec<Screen>>) -> std::io::Result<()> {
    std::thread::Builder::new().name("net".into()).spawn(move || {
        loop {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", DEFAULT_PORT)) {
                log::info!("connected to the Mac");
                if let Err(e) = session(stream, &shared, &screens) {
                    log::warn!("connection: {e}");
                }
                log::info!("disconnected");
                *lock(&shared.decoder) = None;
            }
            std::thread::sleep(RECONNECT_DELAY);
        }
    })?;
    Ok(())
}

fn session(stream: TcpStream, shared: &Arc<Shared>, screens: &mpsc::Sender<Vec<Screen>>) -> crate::Result<()> {
    stream.set_nodelay(true)?;
    let sender = Sender::new(TCP_FRAGMENT);
    sender.send(channel::HEADSET_CONTROL, Priority::Control, &hello())?;
    *lock(&shared.sender) = Some(sender.clone());
    let writer = {
        let (s, w) = (sender.clone(), stream.try_clone()?);
        std::thread::spawn(move || s.pump(w))
    };
    let result = receive(stream, shared, screens);
    sender.close();
    *lock(&shared.sender) = None;
    let _ = writer.join();
    result
}

fn receive(mut stream: TcpStream, shared: &Arc<Shared>, screens: &mpsc::Sender<Vec<Screen>>) -> crate::Result<()> {
    let mut reassembler = Reassembler::default();
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        for m in reassembler.feed(&buf[..n])? {
            match m {
                Message::Screens(s) => {
                    let _ = screens.send(s);
                }
                Message::Video { screen: 0, key, pts_us, data } => shared.on_video(key, pts_us, &data),
                _ => {}
            }
        }
    }
}

fn hello() -> Message {
    Message::Hello {
        version: PROTOCOL_VERSION,
        pixels_per_degree: PIXELS_PER_DEGREE,
        refresh_rate: REFRESH_RATE,
        max_decoders: MAX_DECODERS,
        max_layers: MAX_LAYERS,
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
