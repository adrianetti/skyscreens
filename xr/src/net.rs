//! Connection to the Mac. Over USB, `adb reverse tcp:7243 tcp:7243` makes localhost reach it.
//! Video goes straight from the network thread into the decoder, never through the frame loop.

use crate::decoder::Decoder;
use link::{Message, Priority, Reassembler, Screen, Sender, DEFAULT_PORT, PROTOCOL_VERSION, TCP_FRAGMENT};
use ndk::native_window::NativeWindow;
use std::io::Read;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Channel for messages the headset sends back.
const CONTROL: u8 = 250;

#[derive(Default)]
pub struct Shared {
    sender: Mutex<Option<Sender>>,
    /// The surface of the screen's layer, set by the frame loop once the layer exists.
    window: Mutex<Option<(NativeWindow, i32, i32)>>,
    decoder: Mutex<Option<Decoder>>,
    last_request: Mutex<Option<Instant>>,
}

impl Shared {
    /// Called when the layer's surface changes; the decoder restarts on the next keyframe.
    pub fn set_window(&self, window: NativeWindow, width: i32, height: i32) {
        *self.decoder.lock().unwrap() = None;
        *self.window.lock().unwrap() = Some((window, width, height));
        self.request_keyframe();
    }

    fn send(&self, m: &Message) {
        if let Some(s) = self.sender.lock().unwrap().as_ref() {
            s.send(CONTROL, Priority::Control, m);
        }
    }

    fn request_keyframe(&self) {
        let mut last = self.last_request.lock().unwrap();
        if last.map_or(true, |t| t.elapsed() > Duration::from_millis(200)) {
            *last = Some(Instant::now());
            self.send(&Message::KeyframeRequest { screen: 0 });
        }
    }

    fn on_video(self: &Arc<Self>, key: bool, pts_us: u64, data: &[u8]) {
        let mut decoder = self.decoder.lock().unwrap();
        if decoder.is_none() {
            let window = self.window.lock().unwrap();
            let Some((w, width, height)) = window.as_ref() else { return };
            if !key {
                drop(window);
                drop(decoder);
                return self.request_keyframe();
            }
            let me = Arc::downgrade(self);
            match Decoder::new(w, *width, *height, data, move |pts| {
                if let Some(me) = me.upgrade() {
                    me.send(&Message::Pong { t_us: pts });
                }
            }) {
                Ok(d) => *decoder = Some(d),
                Err(e) => return log::error!("decoder: {e}"),
            }
        }
        match decoder.as_ref().unwrap().feed(data, pts_us) {
            Ok(true) => {}
            Ok(false) => log::warn!("decoder busy, frame dropped"),
            Err(e) => {
                log::error!("decode: {e}");
                *decoder = None;
            }
        }
    }
}

/// Connects in the background and reconnects forever. Screen lists go to the frame loop.
pub fn spawn(shared: Arc<Shared>, screens: mpsc::Sender<Vec<Screen>>) {
    std::thread::Builder::new()
        .name("net".into())
        .spawn(move || loop {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", DEFAULT_PORT)) {
                log::info!("connected to the Mac");
                if let Err(e) = session(stream, &shared, &screens) {
                    log::warn!("connection: {e}");
                }
                log::info!("disconnected");
                *shared.decoder.lock().unwrap() = None;
            }
            std::thread::sleep(Duration::from_millis(500));
        })
        .expect("net thread");
}

fn session(stream: TcpStream, shared: &Arc<Shared>, screens: &mpsc::Sender<Vec<Screen>>) -> crate::Result<()> {
    stream.set_nodelay(true)?;
    let sender = Sender::new(TCP_FRAGMENT);
    sender.send(CONTROL, Priority::Control, &Message::Hello { version: PROTOCOL_VERSION, pixels_per_degree: 25.0, refresh_rate: 90.0, max_decoders: 16, max_layers: 16 });
    *shared.sender.lock().unwrap() = Some(sender.clone());
    let writer = {
        let (s, w) = (sender.clone(), stream.try_clone()?);
        std::thread::spawn(move || s.pump(w))
    };

    let mut r = Reassembler::default();
    let mut buf = vec![0u8; 256 * 1024];
    let mut stream = stream;
    let result = loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) => break Err(e.into()),
        };
        let msgs = match r.feed(&buf[..n]) {
            Ok(m) => m,
            Err(e) => break Err(format!("{e:?}").into()),
        };
        for m in msgs {
            match m {
                Message::Screens(s) => {
                    let _ = screens.send(s);
                }
                Message::Video { screen: 0, key, pts_us, data } => shared.on_video(key, pts_us, &data),
                _ => {}
            }
        }
    };
    sender.close();
    *shared.sender.lock().unwrap() = None;
    let _ = writer.join();
    result
}
