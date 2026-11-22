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
    Connected,
    Disconnected,
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
        lock(&self.sender).as_ref().is_some_and(|sender| {
            sender.replace(channel::CURSOR, Priority::Cursor, &Message::Cursor { screen, x, y, shape }).is_ok()
        })
    }

    fn send(&self, channel: u8, priority: Priority, message: &Message) -> bool {
        lock(&self.sender).as_ref().is_some_and(|sender| sender.send(channel, priority, message).is_ok())
    }

    fn accept_forever(self: Arc<Self>, listener: &TcpListener) {
        for stream in listener.incoming().flatten() {
            self.serve(&stream);
        }
    }

    fn serve(self: &Arc<Self>, stream: &TcpStream) {
        if let Err(error) = stream.set_nodelay(true) {
            log::warn!("headset socket: nodelay: {error}");
        }
        let Ok(reader) = stream.try_clone() else { return };
        let sender = Sender::new(TCP_FRAGMENT);
        let screens = lock(&self.screens).clone();
        if let Err(error) = sender.send(channel::MAC_CONTROL, Priority::Control, &Message::Screens(screens)) {
            log::warn!("screen list not sent: {error}");
        }
        *lock(&self.sender) = Some(sender.clone());
        self.connected.store(true, Ordering::SeqCst);
        self.emit(Event::Connected);
        let me = Arc::clone(self);
        let read = thread::spawn(move || me.read_until_closed(reader));
        if let Err(error) = sender.pump(stream) {
            log::info!("headset connection ended: {error}");
        }
        if let Err(error) = stream.shutdown(Shutdown::Both) {
            log::debug!("headset socket already closed: {error}");
        }
        if read.join().is_err() {
            log::error!("headset reader thread panicked");
        }
        self.connected.store(false, Ordering::SeqCst);
        *lock(&self.sender) = None;
        self.emit(Event::Disconnected);
    }

    fn emit(&self, event: Event) {
        let handler = lock(&self.handler).clone();
        if let Some(handle) = handler {
            handle(event);
        }
    }

    fn read_until_closed(&self, mut stream: TcpStream) {
        let mut reassembler = Reassembler::default();
        let mut buf = vec![0u8; 64 * 1024];
        while let Ok(n @ 1..) = stream.read(&mut buf) {
            let Ok(messages) = reassembler.feed(&buf[..n]) else { break };
            for event in messages.iter().filter_map(event_from) {
                self.emit(event);
            }
        }
        if let Some(sender) = lock(&self.sender).as_ref() {
            sender.close();
        }
    }
}

fn event_from(message: &Message) -> Option<Event> {
    match *message {
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_names_the_address_and_state() {
        let server = Server::listen(TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
        let text = format!("{server:?}");
        assert!(text.starts_with("Server { addr: 127.0.0.1:"), "{text}");
        assert!(text.contains("connected: false"), "{text}");
    }
}
