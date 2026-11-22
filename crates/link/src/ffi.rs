#![expect(unsafe_code, reason = "C ABI for the Mac app: raw pointers and exported symbols")]

use crate::message::{Role, Screen};
use crate::sender::Priority;
use crate::server::{Event, Server};
use std::ffi::c_void;
use std::sync::Arc;

pub const SK_DEFAULT_PORT: u16 = 7243;
pub const SK_EVENT_KEYFRAME_REQUEST: u8 = 1;
pub const SK_EVENT_PONG: u8 = 2;
pub const SK_PRIORITY_CONTROL: u8 = 0;
pub const SK_PRIORITY_CURSOR: u8 = 1;
pub const SK_PRIORITY_FOCUSED: u8 = 2;
pub const SK_PRIORITY_VIDEO: u8 = 3;
pub const SK_PRIORITY_BACKGROUND: u8 = 4;
pub const SK_PRIORITY_REFINE: u8 = 5;

const _: () = {
    assert!(SK_DEFAULT_PORT == crate::DEFAULT_PORT, "SK_DEFAULT_PORT must match the Rust value");
    assert!(SK_PRIORITY_CONTROL == Priority::Control as u8, "SK_PRIORITY_CONTROL must match the Rust value");
    assert!(SK_PRIORITY_CURSOR == Priority::Cursor as u8, "SK_PRIORITY_CURSOR must match the Rust value");
    assert!(SK_PRIORITY_FOCUSED == Priority::Focused as u8, "SK_PRIORITY_FOCUSED must match the Rust value");
    assert!(SK_PRIORITY_VIDEO == Priority::Video as u8, "SK_PRIORITY_VIDEO must match the Rust value");
    assert!(SK_PRIORITY_BACKGROUND == Priority::Background as u8, "SK_PRIORITY_BACKGROUND must match the Rust value");
    assert!(SK_PRIORITY_REFINE == Priority::Refine as u8, "SK_PRIORITY_REFINE must match the Rust value");
};

#[derive(Debug)]
pub struct SkServer {
    _opaque: (),
}

pub type SkCallback = extern "C" fn(ctx: *mut c_void, kind: u8, screen: u8, value: u64);

#[derive(Clone, Copy)]
struct Context(*mut c_void);

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

#[unsafe(no_mangle)]
pub extern "C" fn sk_server_start(port: u16) -> *const SkServer {
    Server::bind(port).map_or(std::ptr::null(), |s| Arc::into_raw(s).cast())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_server_connected(server: *const SkServer) -> bool {
    unsafe { handle(server) }.is_some_and(Server::is_connected)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_server_on_message(server: *const SkServer, cb: SkCallback, ctx: *mut c_void) {
    let Some(server) = (unsafe { handle(server) }) else { return };
    let ctx = Context(ctx);
    server.on_event(move |event| {
        let ctx = ctx;
        match event {
            Event::KeyframeRequest { screen } => cb(ctx.0, SK_EVENT_KEYFRAME_REQUEST, screen, 0),
            Event::Pong { t_us } => cb(ctx.0, SK_EVENT_PONG, 0, t_us),
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_set_screens(
    server: *const SkServer,
    count: u8,
    ids: *const u8,
    roles: *const u8,
    widths: *const u16,
    heights: *const u16,
) -> bool {
    let Some(server) = (unsafe { handle(server) }) else { return false };
    let n = usize::from(count);
    let ids = unsafe { slice(ids, n) };
    let roles = unsafe { slice(roles, n) };
    let widths = unsafe { slice(widths, n) };
    let heights = unsafe { slice(heights, n) };
    let screens = (0..n)
        .map(|i| Screen { id: ids[i], role: Role::from(roles[i]), width: widths[i], height: heights[i] })
        .collect();
    server.set_screens(screens)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_send_video(
    server: *const SkServer,
    screen: u8,
    key: bool,
    pts_us: u64,
    data: *const u8,
    len: usize,
    priority: u8,
) -> bool {
    let Some(server) = (unsafe { handle(server) }) else { return false };
    let data = unsafe { slice(data, len) }.to_vec();
    server.send_video(screen, key, pts_us, data, Priority::from(priority))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_send_tile(
    server: *const SkServer,
    screen: u8,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    lz4: *const u8,
    len: usize,
    priority: u8,
) -> bool {
    let Some(server) = (unsafe { handle(server) }) else { return false };
    let lz4 = unsafe { slice(lz4, len) }.to_vec();
    server.send_tile(screen, [x, y, w, h], lz4, Priority::from(priority))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sk_send_cursor(server: *const SkServer, screen: u8, x: f32, y: f32, shape: u16) -> bool {
    unsafe { handle(server) }.is_some_and(|s| s.send_cursor(screen, x, y, shape))
}

unsafe fn handle<'a>(server: *const SkServer) -> Option<&'a Server> {
    unsafe { server.cast::<Server>().as_ref() }
}

unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() || len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(ptr, len) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Reassembler, Screen};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static LAST_PONG: AtomicU64 = AtomicU64::new(0);

    extern "C" fn record(_ctx: *mut c_void, kind: u8, _screen: u8, value: u64) {
        if kind == SK_EVENT_PONG {
            LAST_PONG.store(value, Ordering::SeqCst);
        }
    }

    fn handle_for_tests() -> (*const SkServer, std::net::SocketAddr) {
        let server = Server::listen(TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
        let addr = server.local_addr();
        (Arc::into_raw(server).cast(), addr)
    }

    fn eventually(condition: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn null_handles_are_rejected_without_crashing() {
        let null = std::ptr::null();
        let none = std::ptr::null();
        assert!(!unsafe { sk_server_connected(null) }, "connected");
        assert!(!unsafe { sk_set_screens(null, 0, none, none, std::ptr::null(), std::ptr::null()) }, "screens");
        assert!(!unsafe { sk_send_video(null, 0, true, 0, none, 0, SK_PRIORITY_FOCUSED) }, "video");
        assert!(!unsafe { sk_send_tile(null, 0, 0, 0, 1, 1, none, 0, SK_PRIORITY_REFINE) }, "tile");
        assert!(!unsafe { sk_send_cursor(null, 0, 0.0, 0.0, 0) }, "cursor");
        unsafe { sk_server_on_message(null, record, std::ptr::null_mut()) };
    }

    #[test]
    fn the_c_interface_streams_to_a_headset_and_reports_pongs() {
        let (server, addr) = handle_for_tests();
        let (ids, roles, widths, heights) = ([0u8], [0u8], [1616u16], [1008u16]);
        unsafe { sk_server_on_message(server, record, std::ptr::null_mut()) };
        unsafe { sk_set_screens(server, 1, ids.as_ptr(), roles.as_ptr(), widths.as_ptr(), heights.as_ptr()) };
        let mut headset = TcpStream::connect(addr).unwrap();
        assert!(eventually(|| unsafe { sk_server_connected(server) }));
        let frame = [0u8, 0, 0, 1, 0x65, 7];
        assert!(unsafe { sk_send_video(server, 0, true, 5, frame.as_ptr(), frame.len(), SK_PRIORITY_FOCUSED) });

        headset.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reassembler = Reassembler::default();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        while got.len() < 2 {
            let n = headset.read(&mut buf).unwrap();
            got.extend(reassembler.feed(&buf[..n]).unwrap());
        }
        let screen = Screen { id: 0, role: Role::Code, width: 1616, height: 1008 };
        assert_eq!(got[0], Message::Screens(vec![screen]));
        assert_eq!(got[1], Message::Video { screen: 0, key: true, pts_us: 5, data: frame.to_vec() });

        let reply = crate::Sender::new(crate::TCP_FRAGMENT);
        reply.send(crate::channel::HEADSET_CONTROL, Priority::Control, &Message::Pong { t_us: 4242 }).unwrap();
        reply.close();
        reply.pump(&mut headset).unwrap();
        headset.flush().unwrap();
        assert!(eventually(|| LAST_PONG.load(Ordering::SeqCst) == 4242));
    }
}
