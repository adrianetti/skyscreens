//! Turns a byte stream of fragments back into messages.

use crate::message::{DecodeError, Message};
use crate::FRAGMENT_HEADER;
use std::collections::HashMap;

/// Largest fragment accepted, to bound memory on a corrupt stream.
const MAX_FRAGMENT: usize = 1 << 20;

#[derive(Debug, PartialEq, Eq)]
pub enum StreamError {
    BadFragment,
    Decode(DecodeError),
}

struct Partial {
    id: u32,
    next: u16,
    body: Vec<u8>,
}

/// Feed it bytes as they arrive, in any chunk size; it returns completed messages.
#[derive(Default)]
pub struct Reassembler {
    buf: Vec<u8>,
    partial: HashMap<u8, Partial>,
}

impl Reassembler {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Message>, StreamError> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut at = 0;
        while self.buf.len() - at >= 4 {
            let len = u32::from_le_bytes(self.buf[at..at + 4].try_into().unwrap()) as usize;
            if !(FRAGMENT_HEADER..=MAX_FRAGMENT + FRAGMENT_HEADER).contains(&len) {
                return Err(StreamError::BadFragment);
            }
            if self.buf.len() - at - 4 < len {
                break;
            }
            let f = &self.buf[at + 4..at + 4 + len];
            let channel = f[0];
            let id = u32::from_le_bytes(f[1..5].try_into().unwrap());
            let index = u16::from_le_bytes(f[5..7].try_into().unwrap());
            let count = u16::from_le_bytes(f[7..9].try_into().unwrap());
            let payload = &f[FRAGMENT_HEADER..];
            at += 4 + len;

            if index == 0 {
                self.partial.insert(channel, Partial { id, next: 0, body: Vec::new() });
            }
            // A gap (only possible on lossy links) drops the message instead of corrupting it.
            let Some(p) = self.partial.get_mut(&channel).filter(|p| p.id == id && p.next == index) else {
                self.partial.remove(&channel);
                continue;
            };
            p.body.extend_from_slice(payload);
            p.next += 1;
            if p.next == count {
                let p = self.partial.remove(&channel).unwrap();
                out.push(Message::decode(&p.body).map_err(StreamError::Decode)?);
            }
        }
        self.buf.drain(..at);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::{Priority, Sender};

    #[test]
    fn handles_any_chunking() {
        let s = Sender::new(64);
        let msgs: Vec<Message> = (0..20).map(|i| Message::Video { screen: 0, key: i == 0, pts_us: i, data: vec![i as u8; 300] }).collect();
        for m in &msgs {
            s.send(0, Priority::Video, m);
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
    }
}
