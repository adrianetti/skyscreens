//! C ABI for the Mac app (Swift). The Mac listens; the headset connects.
//!
//! Over USB, `adb reverse tcp:7243 tcp:7243` makes the headset's localhost reach the Mac.

use crate::message::{Message, Role, Screen};
use crate::receiver::Reassembler;
use crate::sender::{Priority, Sender};
use crate::TCP_FRAGMENT;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// A listening Mac endpoint. Accepts one headset at a time and keeps listening after it leaves.
pub struct SkServer {
    sender: Mutex<Option<Sender>>,
    screens: Mutex<Vec<Screen>>,
    connected: AtomicBool,
    on_message: Mutex<Option<(extern "C" fn(*mut std::ffi::c_void, u8, u8), SendPtr)>>,
}

#[derive(Clone, Copy)]
struct SendPtr(*mut std::ffi::c_void);
unsafe impl Send for SendPtr {}

fn priority(p: u8) -> Priority {
    match p {
        0 => Priority::Control,
        1 => Priority::Cursor,
        2 => Priority::Focused,
        3 => Priority::Video,
        4 => Priority::Background,
        _ => Priority::Refine,
    }
}

impl SkServer {
    fn serve(self: Arc<Self>, listener: TcpListener) {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_nodelay(true);
            let sender = Sender::new(TCP_FRAGMENT);
            sender.send(255, Priority::Control, &Message::Screens(self.screens.lock().unwrap().clone()));
            *self.sender.lock().unwrap() = Some(sender.clone());
            self.connected.store(true, Ordering::SeqCst);
            let reader = stream.try_clone().ok();
            let me = self.clone();
            let read = thread::spawn(move || {
                if let Some(r) = reader {
                    me.read_loop(r);
                }
            });
            let _ = sender.pump(&stream);
            let _ = stream.shutdown(std::net::Shutdown::Both);
            let _ = read.join();
            self.connected.store(false, Ordering::SeqCst);
            *self.sender.lock().unwrap() = None;
        }
    }

    /// Reports headset requests to the app as (kind, screen): 1 = keyframe request.
    fn read_loop(&self, mut stream: TcpStream) {
        let mut r = Reassembler::default();
        let mut buf = vec![0u8; 64 * 1024];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            let Ok(msgs) = r.feed(&buf[..n]) else { break };
            for m in msgs {
                if let Message::KeyframeRequest { screen } = m {
                    if let Some((cb, ctx)) = *self.on_message.lock().unwrap() {
                        cb(ctx.0, 1, screen);
                    }
                }
            }
        }
        if let Some(s) = self.sender.lock().unwrap().as_ref() {
            s.close();
        }
    }

    fn send(&self, channel: u8, p: u8, m: &Message, latest_only: bool) -> bool {
        match self.sender.lock().unwrap().as_ref() {
            Some(s) if latest_only => s.replace(channel, priority(p), m),
            Some(s) => s.send(channel, priority(p), m),
            None => return false,
        }
        true
    }
}

/// Starts listening on `port`. Returns null if the port is taken.
#[no_mangle]
pub extern "C" fn sk_server_start(port: u16) -> *const SkServer {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else { return std::ptr::null() };
    let server = Arc::new(SkServer {
        sender: Mutex::new(None),
        screens: Mutex::new(Vec::new()),
        connected: AtomicBool::new(false),
        on_message: Mutex::new(None),
    });
    let s = server.clone();
    thread::spawn(move || s.serve(listener));
    Arc::into_raw(server)
}

/// # Safety
/// `server` must come from `sk_server_start`.
#[no_mangle]
pub unsafe extern "C" fn sk_server_connected(server: *const SkServer) -> bool {
    (*server).connected.load(Ordering::SeqCst)
}

/// Called from a link thread when the headset asks for something.
///
/// # Safety
/// `server` must come from `sk_server_start`; `ctx` must stay valid while the server runs.
#[no_mangle]
pub unsafe extern "C" fn sk_server_on_message(server: *const SkServer, cb: extern "C" fn(*mut std::ffi::c_void, u8, u8), ctx: *mut std::ffi::c_void) {
    *(*server).on_message.lock().unwrap() = Some((cb, SendPtr(ctx)));
}

/// Sets the screen list (ids, roles, widths, heights: `count` entries each) and sends it.
///
/// # Safety
/// Each array must hold `count` elements.
#[no_mangle]
pub unsafe extern "C" fn sk_set_screens(server: *const SkServer, count: u8, ids: *const u8, roles: *const u8, widths: *const u16, heights: *const u16) {
    let n = count as usize;
    let screens: Vec<Screen> = (0..n)
        .map(|i| Screen {
            id: *ids.add(i),
            role: match *roles.add(i) {
                0 => Role::Code,
                1 => Role::Chat,
                2 => Role::Email,
                3 => Role::Video,
                _ => Role::Other,
            },
            width: *widths.add(i),
            height: *heights.add(i),
        })
        .collect();
    *(*server).screens.lock().unwrap() = screens.clone();
    (*server).send(255, 0, &Message::Screens(screens), false);
}

/// # Safety
/// `data` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn sk_send_video(server: *const SkServer, screen: u8, key: bool, pts_us: u64, data: *const u8, len: usize, prio: u8) -> bool {
    let data = std::slice::from_raw_parts(data, len).to_vec();
    (*server).send(screen.wrapping_mul(4), prio, &Message::Video { screen, key, pts_us, data }, false)
}

/// # Safety
/// `lz4` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn sk_send_tile(server: *const SkServer, screen: u8, x: u16, y: u16, w: u16, h: u16, lz4: *const u8, len: usize, prio: u8) -> bool {
    let lz4 = std::slice::from_raw_parts(lz4, len).to_vec();
    (*server).send(screen.wrapping_mul(4) + 1, prio, &Message::Tile { screen, x, y, w, h, lz4 }, false)
}

/// Sends the cursor position; an older unsent position is dropped.
///
/// # Safety
/// `server` must come from `sk_server_start`.
#[no_mangle]
pub unsafe extern "C" fn sk_send_cursor(server: *const SkServer, screen: u8, x: f32, y: f32, shape: u16) -> bool {
    (*server).send(254, 1, &Message::Cursor { screen, x, y, shape }, true)
}
