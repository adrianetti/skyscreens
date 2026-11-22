use crate::TCP_FRAGMENT;
use crate::channel;
use crate::message::{Message, Screen};
use crate::receiver::Reassembler;
use crate::sender::{Priority, Sender};
use std::io::{self, Read};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    KeyframeRequest { screen: u8 },
    Pong { t_us: u64 },
}

type Handler = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Server {
    addr: SocketAddr,
    sender: Mutex<Option<Sender>>,
    screens: Mutex<Vec<Screen>>,
    connected: AtomicBool,
    handler: Mutex<Option<Handler>>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("addr", &self.addr)
            .field("connected", &self.is_connected())
            .finish_non_exhaustive()
    }
}

impl Server {
    pub fn bind(port: u16) -> io::Result<Arc<Self>> {
        Self::listen(TcpListener::bind(("127.0.0.1", port))?)
    }

    pub fn listen(listener: TcpListener) -> io::Result<Arc<Self>> {
        let server = Arc::new(Self {
            addr: listener.local_addr()?,
            sender: Mutex::new(None),
            screens: Mutex::new(Vec::new()),
            connected: AtomicBool::new(false),
            handler: Mutex::new(None),
        });
        let accepting = Arc::clone(&server);
        thread::Builder::new().name("link-accept".into()).spawn(move || accepting.accept_forever(&listener))?;
        Ok(server)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub fn on_event(&self, handler: impl Fn(Event) + Send + Sync + 'static) {
        *lock(&self.handler) = Some(Arc::new(handler));
    }

    pub fn set_screens(&self, screens: Vec<Screen>) -> bool {
        lock(&self.screens).clone_from(&screens);
        self.send(channel::MAC_CONTROL, Priority::Control, &Message::Screens(screens))
    }

    pub fn send_video(&self, screen: u8, key: bool, pts_us: u64, data: Vec<u8>, priority: Priority) -> bool {
        self.send(channel::video(screen), priority, &Message::Video { screen, key, pts_us, data })
    }

    pub fn send_tile(&self, screen: u8, rect: [u16; 4], lz4: Vec<u8>, priority: Priority) -> bool {
        let [x, y, w, h] = rect;
        self.send(channel::tile(screen), priority, &Message::Tile { screen, x, y, w, h, lz4 })
    }

    pub fn send_cursor(&self, screen: u8, x: f32, y: f32, shape: u16) -> bool {
        lock(&self.sender).as_ref().is_some_and(|s| {
            s.replace(channel::CURSOR, Priority::Cursor, &Message::Cursor { screen, x, y, shape }).is_ok()
        })
    }

    fn send(&self, channel: u8, priority: Priority, m: &Message) -> bool {
        lock(&self.sender).as_ref().is_some_and(|s| s.send(channel, priority, m).is_ok())
    }

    fn accept_forever(self: Arc<Self>, listener: &TcpListener) {
        for stream in listener.incoming().flatten() {
            self.serve(&stream);
        }
    }

    fn serve(self: &Arc<Self>, stream: &TcpStream) {
        let _ = stream.set_nodelay(true);
        let Ok(reader) = stream.try_clone() else { return };
        let sender = Sender::new(TCP_FRAGMENT);
        let screens = lock(&self.screens).clone();
        let _ = sender.send(channel::MAC_CONTROL, Priority::Control, &Message::Screens(screens));
        *lock(&self.sender) = Some(sender.clone());
        self.connected.store(true, Ordering::SeqCst);
        let me = Arc::clone(self);
        let read = thread::spawn(move || me.read_until_closed(reader));
        let _ = sender.pump(stream);
        let _ = stream.shutdown(Shutdown::Both);
        let _ = read.join();
        self.connected.store(false, Ordering::SeqCst);
        *lock(&self.sender) = None;
    }

    fn read_until_closed(&self, mut stream: TcpStream) {
        let mut reassembler = Reassembler::default();
        let mut buf = vec![0u8; 64 * 1024];
        while let Ok(n @ 1..) = stream.read(&mut buf) {
            let Ok(messages) = reassembler.feed(&buf[..n]) else { break };
            let handler = lock(&self.handler).clone();
            for event in messages.iter().filter_map(event_from) {
                if let Some(h) = &handler {
                    h(event);
                }
            }
        }
        if let Some(s) = lock(&self.sender).as_ref() {
            s.close();
        }
    }
}

fn event_from(m: &Message) -> Option<Event> {
    match *m {
        Message::KeyframeRequest { screen } => Some(Event::KeyframeRequest { screen }),
        Message::Pong { t_us } => Some(Event::Pong { t_us }),
        Message::Hello { .. }
        | Message::Screens(_)
        | Message::Video { .. }
        | Message::Tile { .. }
        | Message::Cursor { .. }
        | Message::CursorShape { .. }
        | Message::Ping { .. } => None,
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
