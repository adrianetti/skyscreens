//! skyscreens wire protocol and transport, shared by the Mac and headset apps.
//!
//! Every message is split into fragments. A scheduler interleaves fragments from
//! different channels by priority, so a large lossless refresh never delays the cursor.
//! The same fragment format works over TCP (USB through adb) and, later, UDP (Wi-Fi).

pub mod ffi;
pub mod message;
pub mod receiver;
pub mod sender;

pub use message::{Message, Role, Screen};
pub use receiver::Reassembler;
pub use sender::{Priority, Sender};

/// Bumped on every incompatible wire change. Peers refuse to talk across versions.
pub const PROTOCOL_VERSION: u16 = 1;

/// TCP port the Mac listens on. Over USB, `adb reverse tcp:7243 tcp:7243` maps it onto the headset.
pub const DEFAULT_PORT: u16 = 7243;

/// Fragment payload size on TCP. Small enough that one fragment takes under 0.5 ms on USB 2.
pub const TCP_FRAGMENT: usize = 16 * 1024;

/// Fragment header: channel (u8), message id (u32), index (u16), count (u16).
pub const FRAGMENT_HEADER: usize = 9;
