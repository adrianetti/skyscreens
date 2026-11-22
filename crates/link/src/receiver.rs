#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use crate::message::{DecodeError, Message};
use crate::{FRAGMENT_HEADER, LENGTH_PREFIX, MAX_FRAGMENT, MAX_MESSAGE};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum StreamError {
    BadFragment,
    MessageTooLarge,
    Decode(DecodeError),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadFragment => write!(formatter, "fragment length out of range"),
            Self::MessageTooLarge => write!(formatter, "message exceeds the size limit"),
            Self::Decode(error) => write!(formatter, "undecodable message: {error:?}"),
        }
    }
}

impl std::error::Error for StreamError {}

#[derive(Debug)]
struct Partial {
    id: u32,
    next: u16,
    body: Vec<u8>,
}

struct Fragment<'a> {
    channel: u8,
    id: u32,
    index: u16,
    count: u16,
    payload: &'a [u8],
}

const MAX_FRAME: usize = MAX_FRAGMENT + FRAGMENT_HEADER;

impl<'a> Fragment<'a> {
    fn parse(frame: &'a [u8]) -> Option<Self> {
        let (header, payload) = frame.split_first_chunk::<FRAGMENT_HEADER>()?;
        let [channel, id0, id1, id2, id3, index0, index1, count0, count1] = *header;
        Some(Fragment {
            channel,
            id: u32::from_le_bytes([id0, id1, id2, id3]),
            index: u16::from_le_bytes([index0, index1]),
            count: u16::from_le_bytes([count0, count1]),
            payload,
        })
    }
}

#[derive(Debug)]
pub struct Reassembler {
    buf: Vec<u8>,
    partial: HashMap<u8, Partial>,
    max_message: usize,
}

impl Default for Reassembler {
    fn default() -> Self {
        Self::with_max_message(MAX_MESSAGE)
    }
}

impl Reassembler {
    pub fn with_max_message(max_message: usize) -> Self {
        Self { buf: Vec::new(), partial: HashMap::new(), max_message }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Message>, StreamError> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut consumed = 0usize;
        while let Some((prefix, after)) =
            self.buf.get(consumed..).unwrap_or_default().split_first_chunk::<LENGTH_PREFIX>()
        {
            let len = usize::try_from(u32::from_le_bytes(*prefix)).unwrap_or(usize::MAX);
            if !(FRAGMENT_HEADER..=MAX_FRAME).contains(&len) {
                return Err(StreamError::BadFragment);
            }
            let Some(frame) = after.get(..len) else { break };
            let fragment = Fragment::parse(frame).ok_or(StreamError::BadFragment)?;
            if let Some(body) = Self::accept(&mut self.partial, &fragment, self.max_message)? {
                out.push(Message::decode(&body).map_err(StreamError::Decode)?);
            }
            consumed = consumed.saturating_add(LENGTH_PREFIX).saturating_add(len);
        }
        self.buf.drain(..consumed);
        Ok(out)
    }

    fn accept(
        partial: &mut HashMap<u8, Partial>,
        fragment: &Fragment<'_>,
        max_message: usize,
    ) -> Result<Option<Vec<u8>>, StreamError> {
        if fragment.index == 0 {
            partial.insert(fragment.channel, Partial { id: fragment.id, next: 0, body: Vec::new() });
        }
        let in_sequence = |pending: &&mut Partial| pending.id == fragment.id && pending.next == fragment.index;
        let Some(pending) = partial.get_mut(&fragment.channel).filter(in_sequence) else {
            partial.remove(&fragment.channel);
            return Ok(None);
        };
        if pending.body.len().saturating_add(fragment.payload.len()) > max_message {
            partial.remove(&fragment.channel);
            return Err(StreamError::MessageTooLarge);
        }
        pending.body.extend_from_slice(fragment.payload);
        pending.next = pending.next.saturating_add(1);
        if pending.next < fragment.count {
            return Ok(None);
        }
        Ok(partial.remove(&fragment.channel).map(|done| done.body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::{Priority, Sender};

    fn frame(channel: u8, id: u32, index: u16, count: u16, payload: &[u8]) -> Vec<u8> {
        let len = u32::try_from(FRAGMENT_HEADER.saturating_add(payload.len())).unwrap();
        [&len.to_le_bytes()[..], &[channel], &id.to_le_bytes(), &index.to_le_bytes(), &count.to_le_bytes(), payload]
            .concat()
    }

    #[test]
    fn handles_any_chunking() {
        let sender = Sender::new(64);
        let msgs: Vec<Message> = (0..20u8)
            .map(|i| Message::Video { screen: 0, key: i == 0, pts_us: u64::from(i), data: vec![i; 300] })
            .collect();
        for message in &msgs {
            sender.send(0, Priority::Video, message).unwrap();
        }
        sender.close();
        let mut stream = Vec::new();
        sender.pump(&mut stream).unwrap();
        for chunk in [1, 3, 7, 64, 1000] {
            let mut reassembler = Reassembler::default();
            let got: Vec<Message> = stream.chunks(chunk).flat_map(|piece| reassembler.feed(piece).unwrap()).collect();
            assert_eq!(got, msgs, "chunk size {chunk}");
        }
    }

    #[test]
    fn rejects_absurd_lengths() {
        assert_eq!(Reassembler::default().feed(&u32::MAX.to_le_bytes()), Err(StreamError::BadFragment));
        assert_eq!(Reassembler::default().feed(&3u32.to_le_bytes()), Err(StreamError::BadFragment));
    }

    #[test]
    fn a_missing_fragment_drops_only_that_message() {
        let first = Message::Ping { t_us: 5 }.encode();
        let second = Message::Pong { t_us: 6 }.encode();
        let stream = [frame(1, 0, 0, 2, &first[..4]), frame(1, 1, 0, 1, &second)].concat();
        assert_eq!(Reassembler::default().feed(&stream).unwrap(), vec![Message::Pong { t_us: 6 }]);
        let gap = [frame(2, 0, 1, 2, &first[4..]), frame(2, 1, 0, 1, &second)].concat();
        assert_eq!(Reassembler::default().feed(&gap).unwrap(), vec![Message::Pong { t_us: 6 }]);
    }

    #[test]
    fn undecodable_messages_are_errors() {
        let stream = frame(0, 0, 0, 1, &[250, 1, 2]);
        assert_eq!(Reassembler::default().feed(&stream), Err(StreamError::Decode(DecodeError::UnknownType(250))));
    }

    #[test]
    fn the_frame_size_limit_is_exact() {
        let at_limit = u32::try_from(MAX_FRAME).unwrap().to_le_bytes();
        assert_eq!(Reassembler::default().feed(&at_limit), Ok(vec![]), "a full-size frame waits for its bytes");
        let over_limit = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes();
        assert_eq!(Reassembler::default().feed(&over_limit), Err(StreamError::BadFragment));
        assert_eq!(MAX_FRAME, MAX_FRAGMENT + FRAGMENT_HEADER, "a frame is a header plus a fragment");
    }

    #[test]
    fn the_message_size_limit_is_exact() {
        let body = Message::Ping { t_us: 9 }.encode();
        let stream = frame(1, 0, 0, 1, &body);
        let exact = Reassembler::with_max_message(body.len()).feed(&stream);
        assert_eq!(exact, Ok(vec![Message::Ping { t_us: 9 }]));
        let too_small = Reassembler::with_max_message(body.len() - 1).feed(&stream);
        assert_eq!(too_small, Err(StreamError::MessageTooLarge));
    }

    #[test]
    fn a_fragment_from_another_message_on_the_channel_is_dropped() {
        let body = Message::Ping { t_us: 3 }.encode();
        let (head, tail) = body.split_at(4);
        let stream = [frame(5, 10, 0, 2, head), frame(5, 11, 1, 2, tail)].concat();
        assert_eq!(Reassembler::default().feed(&stream), Ok(vec![]));
        let out_of_order = [frame(5, 10, 0, 2, head), frame(5, 10, 0, 2, head), frame(5, 10, 2, 3, tail)].concat();
        assert_eq!(Reassembler::default().feed(&out_of_order), Ok(vec![]));
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(StreamError::BadFragment.to_string(), "fragment length out of range");
        assert_eq!(StreamError::MessageTooLarge.to_string(), "message exceeds the size limit");
        assert_eq!(StreamError::Decode(DecodeError::Truncated).to_string(), "undecodable message: Truncated");
    }
}
