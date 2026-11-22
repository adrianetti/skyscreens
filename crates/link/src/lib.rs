#![forbid(unsafe_code)]

pub mod annexb;
pub mod channel;
pub mod message;
pub mod receiver;
pub mod sender;
pub mod server;

pub use message::{DecodeError, Message, Role, Screen};
pub use receiver::{Reassembler, StreamError};
pub use sender::{Priority, SendError, Sender};
pub use server::{Event, Server};

pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 7243;
pub const TCP_FRAGMENT: usize = 16 * 1024;
pub const FRAGMENT_HEADER: usize = 9;
pub const LENGTH_PREFIX: usize = 4;
pub const MAX_FRAGMENT: usize = 1 << 20;
pub const MAX_MESSAGE: usize = 64 << 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_constants_are_pinned() {
        assert_eq!((DEFAULT_PORT, PROTOCOL_VERSION), (7243, 1), "the headset and Mac agree on these");
        assert_eq!((LENGTH_PREFIX, FRAGMENT_HEADER), (4, 9), "fragment framing");
        assert_eq!((TCP_FRAGMENT, MAX_FRAGMENT, MAX_MESSAGE), (16_384, 1_048_576, 67_108_864), "size limits");
    }
}
