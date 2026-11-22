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
pub(crate) struct Shared {
    sender: Mutex<Option<Sender>>,
    window: Mutex<Option<(NativeWindow, i32, i32)>>,
    decoder: Mutex<Option<Decoder>>,
    last_request: Mutex<Option<Instant>>,
}

impl Shared {
    pub(crate) fn detach_window(&self) {
        *lock(&self.decoder) = None;
        *lock(&self.window) = None;
    }

    pub(crate) fn set_window(&self, window: NativeWindow, width: i32, height: i32) {
        *lock(&self.decoder) = None;
        *lock(&self.window) = Some((window, width, height));
        self.request_keyframe();
    }

    fn send(&self, message: &Message) {
        if let Some(sender) = lock(&self.sender).as_ref()
            && let Err(error) = sender.send(channel::HEADSET_CONTROL, Priority::Control, message)
        {
            log::warn!("send: {error}");
        }
    }

    fn request_keyframe(&self) {
        if self.keyframe_request_due() {
            self.send(&Message::KeyframeRequest { screen: 0 });
        }
    }

    fn keyframe_request_due(&self) -> bool {
        let mut last = lock(&self.last_request);
        let due = last.is_none_or(|sent| sent.elapsed() > KEYFRAME_REQUEST_INTERVAL);
        if due {
            *last = Some(Instant::now());
        }
        drop(last);
        due
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
                Ok(started) => *decoder = Some(started),
                Err(error) => return log::error!("decoder: {error}"),
            }
        }
        let Some(active) = decoder.as_ref() else { return };
        match active.feed(data, pts_us) {
            Ok(true) => {}
            Ok(false) => log::warn!("decoder busy, frame dropped"),
            Err(error) => {
                log::error!("decode: {error}");
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

pub(crate) fn spawn(shared: Arc<Shared>, screens: mpsc::Sender<Vec<Screen>>) -> std::io::Result<()> {
    std::thread::Builder::new().name("net".into()).spawn(move || reconnect_forever(&shared, &screens))?;
    Ok(())
}

fn reconnect_forever(shared: &Arc<Shared>, screens: &mpsc::Sender<Vec<Screen>>) -> ! {
    loop {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", DEFAULT_PORT)) {
            log::info!("connected to the Mac");
            if let Err(error) = session(stream, shared, screens) {
                log::warn!("connection: {error}");
            }
            log::info!("disconnected");
            *lock(&shared.decoder) = None;
        }
        std::thread::sleep(RECONNECT_DELAY);
    }
}

fn session(stream: TcpStream, shared: &Arc<Shared>, screens: &mpsc::Sender<Vec<Screen>>) -> crate::Result<()> {
    stream.set_nodelay(true)?;
    let sender = Sender::new(TCP_FRAGMENT);
    sender.send(channel::HEADSET_CONTROL, Priority::Control, &hello())?;
    *lock(&shared.sender) = Some(sender.clone());
    let writer = {
        let (pumping, socket) = (sender.clone(), stream.try_clone()?);
        std::thread::spawn(move || pumping.pump(socket))
    };
    let result = receive(stream, shared, screens);
    sender.close();
    *lock(&shared.sender) = None;
    match writer.join() {
        Ok(Err(error)) => log::info!("writer stopped: {error}"),
        Err(_) => log::error!("writer thread panicked"),
        Ok(Ok(())) => {}
    }
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
        for message in reassembler.feed(&buf[..n])? {
            match message {
                Message::Screens(list) => {
                    if let Err(error) = screens.send(list) {
                        log::warn!("frame loop is gone: {error}");
                    }
                }
                Message::Video { screen: 0, key, pts_us, data } => shared.on_video(key, pts_us, &data),
                Message::Hello { .. }
                | Message::Video { .. }
                | Message::Tile { .. }
                | Message::Cursor { .. }
                | Message::CursorShape { .. }
                | Message::KeyframeRequest { .. }
                | Message::Ping { .. }
                | Message::Pong { .. } => {}
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
