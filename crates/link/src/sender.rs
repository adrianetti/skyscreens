//! Priority scheduler: splits messages into fragments and sends the most urgent fragment first.

use crate::{message::Message, FRAGMENT_HEADER};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Condvar, Mutex};

/// Lower value goes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    Control = 0,
    Cursor = 1,
    Focused = 2,
    Video = 3,
    Background = 4,
    Refine = 5,
}

const LEVELS: usize = 6;

/// One message waiting to be sent, consumed fragment by fragment.
struct Pending {
    channel: u8,
    id: u32,
    body: Vec<u8>,
    next: u16,
    count: u16,
    fragment: usize,
}

struct Queues {
    levels: [VecDeque<Pending>; LEVELS],
    ids: [u32; 256],
    closed: bool,
}

impl Default for Queues {
    fn default() -> Self {
        Queues { levels: Default::default(), ids: [0; 256], closed: false }
    }
}

/// Thread-safe sender. `send` never blocks on the network; a writer thread drains the queues.
#[derive(Clone)]
pub struct Sender {
    inner: Arc<(Mutex<Queues>, Condvar)>,
    fragment: usize,
}

impl Sender {
    pub fn new(fragment: usize) -> Sender {
        Sender { inner: Arc::new((Mutex::new(Queues::default()), Condvar::new())), fragment }
    }

    /// Queues a message. `channel` keeps fragments of one stream in order (for example screen × lane);
    /// different channels interleave freely.
    pub fn send(&self, channel: u8, priority: Priority, msg: &Message) {
        let body = msg.encode();
        let count = body.len().div_ceil(self.fragment).max(1) as u16;
        let (lock, cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        let id = q.ids[channel as usize];
        q.ids[channel as usize] = id.wrapping_add(1);
        q.levels[priority as usize].push_back(Pending { channel, id, body, next: 0, count, fragment: self.fragment });
        cv.notify_one();
    }

    /// Drops every queued message of a channel that has not started sending. Used for
    /// "latest wins" streams like the cursor, where an old position is worthless.
    pub fn replace(&self, channel: u8, priority: Priority, msg: &Message) {
        {
            let mut q = self.inner.0.lock().unwrap();
            q.levels[priority as usize].retain(|p| p.channel != channel || p.next > 0);
        }
        self.send(channel, priority, msg);
    }

    pub fn close(&self) {
        let (lock, cv) = &*self.inner;
        lock.lock().unwrap().closed = true;
        cv.notify_all();
    }

    /// Blocks until a fragment is ready and returns it framed for a byte stream:
    /// u32 length, then the fragment header and payload. Returns None once closed.
    pub fn next_frame(&self) -> Option<Vec<u8>> {
        let (lock, cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        loop {
            if let Some(level) = q.levels.iter_mut().find(|l| !l.is_empty()) {
                let p = level.front_mut().unwrap();
                let start = p.next as usize * p.fragment;
                let end = (start + p.fragment).min(p.body.len());
                let mut out = Vec::with_capacity(4 + FRAGMENT_HEADER + end - start);
                out.extend(((FRAGMENT_HEADER + end - start) as u32).to_le_bytes());
                out.push(p.channel);
                out.extend(p.id.to_le_bytes());
                out.extend(p.next.to_le_bytes());
                out.extend(p.count.to_le_bytes());
                out.extend_from_slice(&p.body[start..end]);
                p.next += 1;
                if p.next == p.count {
                    level.pop_front();
                }
                return Some(out);
            }
            if q.closed {
                return None;
            }
            q = cv.wait(q).unwrap();
        }
    }

    /// Writes fragments to `w` until closed or the write fails.
    pub fn pump(&self, mut w: impl Write) -> io::Result<()> {
        while let Some(frame) = self.next_frame() {
            w.write_all(&frame)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{receiver::Reassembler, TCP_FRAGMENT};

    #[test]
    fn cursor_jumps_ahead_of_a_large_refresh() {
        let s = Sender::new(TCP_FRAGMENT);
        let big = Message::Tile { screen: 0, x: 0, y: 0, w: 1616, h: 1008, lz4: vec![1; 480_000] };
        s.send(10, Priority::Refine, &big);
        let first = s.next_frame().unwrap();
        s.send(1, Priority::Cursor, &Message::Cursor { screen: 0, x: 1.0, y: 2.0, shape: 0 });
        let second = s.next_frame().unwrap();
        assert_eq!(first[4], 10, "refresh started first");
        assert_eq!(second[4], 1, "cursor sent before the rest of the refresh");
    }

    #[test]
    fn interleaved_fragments_reassemble() {
        let s = Sender::new(1000);
        let video = Message::Video { screen: 0, key: false, pts_us: 7, data: (0..20_000u32).map(|i| i as u8).collect() };
        let tile = Message::Tile { screen: 1, x: 5, y: 6, w: 7, h: 8, lz4: vec![9; 5_000] };
        s.send(3, Priority::Video, &video);
        s.send(4, Priority::Focused, &tile);
        s.close();
        let mut stream = Vec::new();
        s.pump(&mut stream).unwrap();
        let mut r = Reassembler::default();
        let got = r.feed(&stream).unwrap();
        assert_eq!(got, vec![tile, video]);
    }

    #[test]
    fn replace_keeps_only_the_newest_cursor() {
        let s = Sender::new(TCP_FRAGMENT);
        for i in 0..5 {
            s.replace(1, Priority::Cursor, &Message::Cursor { screen: 0, x: i as f32, y: 0.0, shape: 0 });
        }
        s.close();
        let mut stream = Vec::new();
        s.pump(&mut stream).unwrap();
        let got = Reassembler::default().feed(&stream).unwrap();
        assert_eq!(got, vec![Message::Cursor { screen: 0, x: 4.0, y: 0.0, shape: 0 }]);
    }
}
