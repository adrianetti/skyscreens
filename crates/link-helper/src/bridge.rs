#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use link::{Event, MAX_MESSAGE, Message, Priority};
use std::io::{self, ErrorKind, Read};

pub const LENGTH_PREFIX: usize = 4;
pub const EVENT_SIZE: usize = 10;
pub const MAX_RECORD: usize = MAX_MESSAGE + 1;

pub mod event_kind {
    pub const KEYFRAME_REQUEST: u8 = 1;
    pub const PONG: u8 = 2;
    pub const CONNECTED: u8 = 3;
    pub const DISCONNECTED: u8 = 4;
    pub const LISTENING: u8 = 5;
}

#[derive(Debug, PartialEq)]
pub struct Record {
    pub priority: Priority,
    pub message: Message,
}

pub fn encode_record(priority: Priority, message: &Message) -> Vec<u8> {
    let body = message.encode();
    let length = u32::try_from(body.len().saturating_add(1)).unwrap_or(u32::MAX);
    [&length.to_le_bytes()[..], &[u8::from(priority)], &body].concat()
}

pub fn read_record(input: &mut impl Read) -> io::Result<Option<Record>> {
    let mut prefix = [0u8; LENGTH_PREFIX];
    match input.read_exact(&mut prefix) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = usize::try_from(u32::from_le_bytes(prefix)).unwrap_or(usize::MAX);
    if !(2..=MAX_RECORD).contains(&length) {
        return Err(io::Error::new(ErrorKind::InvalidData, format!("record of {length} bytes")));
    }
    let mut record = vec![0u8; length];
    input.read_exact(&mut record)?;
    let Some((&priority, body)) = record.split_first() else {
        return Err(ErrorKind::InvalidData.into());
    };
    let message =
        Message::decode(body).map_err(|error| io::Error::new(ErrorKind::InvalidData, format!("{error:?}")))?;
    Ok(Some(Record { priority: Priority::from(priority), message }))
}

pub fn encode_event(event: Event) -> [u8; EVENT_SIZE] {
    match event {
        Event::KeyframeRequest { screen } => event_bytes(event_kind::KEYFRAME_REQUEST, screen, 0),
        Event::Pong { t_us } => event_bytes(event_kind::PONG, 0, t_us),
        Event::Connected => event_bytes(event_kind::CONNECTED, 0, 0),
        Event::Disconnected => event_bytes(event_kind::DISCONNECTED, 0, 0),
    }
}

pub fn encode_listening(port: u16) -> [u8; EVENT_SIZE] {
    event_bytes(event_kind::LISTENING, 0, u64::from(port))
}

fn event_bytes(kind: u8, screen: u8, value: u64) -> [u8; EVENT_SIZE] {
    let [v0, v1, v2, v3, v4, v5, v6, v7] = value.to_le_bytes();
    [kind, screen, v0, v1, v2, v3, v4, v5, v6, v7]
}
