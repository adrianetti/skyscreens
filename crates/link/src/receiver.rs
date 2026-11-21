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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::BadFragment => write!(f, "fragment length out of range"),
            StreamError::MessageTooLarge => write!(f, "message exceeds {MAX_MESSAGE} bytes"),
            StreamError::Decode(e) => write!(f, "undecodable message: {e:?}"),
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

impl<'a> Fragment<'a> {
    fn parse(f: &'a [u8]) -> Fragment<'a> {
        Fragment {
            channel: f[0],
            id: u32::from_le_bytes([f[1], f[2], f[3], f[4]]),
            index: u16::from_le_bytes([f[5], f[6]]),
            count: u16::from_le_bytes([f[7], f[8]]),
            payload: &f[FRAGMENT_HEADER..],
        }
    }
}

#[derive(Debug, Default)]
pub struct Reassembler {
    buf: Vec<u8>,
    partial: HashMap<u8, Partial>,
}

impl Reassembler {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Message>, StreamError> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut at = 0;
        while self.buf.len() - at >= LENGTH_PREFIX {
            let prefix = [self.buf[at], self.buf[at + 1], self.buf[at + 2], self.buf[at + 3]];
            let len = u32::from_le_bytes(prefix) as usize;
            if !(FRAGMENT_HEADER..=MAX_FRAGMENT + FRAGMENT_HEADER).contains(&len) {
                return Err(StreamError::BadFragment);
            }
            if self.buf.len() - at - LENGTH_PREFIX < len {
                break;
            }
            let start = at + LENGTH_PREFIX;
            at = start + len;
            let fragment = Fragment::parse(&self.buf[start..at]);
            if let Some(body) = Self::accept(&mut self.partial, &fragment)? {
                out.push(Message::decode(&body).map_err(StreamError::Decode)?);
            }
        }
        self.buf.drain(..at);
        Ok(out)
    }

    fn accept(partial: &mut HashMap<u8, Partial>, f: &Fragment) -> Result<Option<Vec<u8>>, StreamError> {
        if f.index == 0 {
            partial.insert(f.channel, Partial { id: f.id, next: 0, body: Vec::new() });
        }
        let Some(p) = partial.get_mut(&f.channel).filter(|p| p.id == f.id && p.next == f.index) else {
            partial.remove(&f.channel);
            return Ok(None);
        };
        if p.body.len() + f.payload.len() > MAX_MESSAGE {
            partial.remove(&f.channel);
            return Err(StreamError::MessageTooLarge);
        }
        p.body.extend_from_slice(f.payload);
        p.next += 1;
        if p.next < f.count {
            return Ok(None);
        }
        Ok(partial.remove(&f.channel).map(|p| p.body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::{Priority, Sender};

    fn frame(channel: u8, id: u32, index: u16, count: u16, payload: &[u8]) -> Vec<u8> {
        let len = u32::try_from(FRAGMENT_HEADER + payload.len()).unwrap();
        [&len.to_le_bytes()[..], &[channel], &id.to_le_bytes(), &index.to_le_bytes(), &count.to_le_bytes(), payload]
            .concat()
    }

    #[test]
    fn handles_any_chunking() {
        let s = Sender::new(64);
        let msgs: Vec<Message> = (0..20u8)
            .map(|i| Message::Video { screen: 0, key: i == 0, pts_us: u64::from(i), data: vec![i; 300] })
            .collect();
        for m in &msgs {
            s.send(0, Priority::Video, m).unwrap();
        }
        s.close();
        let mut stream = Vec::new();
        s.pump(&mut stream).unwrap();
        for chunk in [1, 3, 7, 64, 1000] {
            let mut r = Reassembler::default();
            let got: Vec<Message> = stream.chunks(chunk).flat_map(|c| r.feed(c).unwrap()).collect();
            assert_eq!(got, msgs, "chunk size {chunk}");
        }
    }

    #[test]
    fn rejects_absurd_lengths() {
        let mut r = Reassembler::default();
        assert_eq!(r.feed(&u32::MAX.to_le_bytes()), Err(StreamError::BadFragment));
        let mut r = Reassembler::default();
        assert_eq!(r.feed(&3u32.to_le_bytes()), Err(StreamError::BadFragment));
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
}
