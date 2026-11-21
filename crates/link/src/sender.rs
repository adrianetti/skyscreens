use crate::{FRAGMENT_HEADER, LENGTH_PREFIX, MAX_FRAGMENT, MAX_MESSAGE, message::Message};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

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

impl From<u8> for Priority {
    fn from(v: u8) -> Priority {
        match v {
            0 => Priority::Control,
            1 => Priority::Cursor,
            2 => Priority::Focused,
            3 => Priority::Video,
            4 => Priority::Background,
            _ => Priority::Refine,
        }
    }
}

const LEVELS: usize = Priority::Refine as usize + 1;

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    TooLarge(usize),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::TooLarge(n) => write!(f, "message of {n} bytes exceeds {MAX_MESSAGE}"),
        }
    }
}

impl std::error::Error for SendError {}

#[derive(Debug)]
struct Pending {
    channel: u8,
    id: u32,
    body: Vec<u8>,
    next: u16,
    count: u16,
}

#[derive(Debug)]
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

#[derive(Clone, Debug)]
pub struct Sender {
    inner: Arc<(Mutex<Queues>, Condvar)>,
    fragment: usize,
}

impl Sender {
    pub fn new(fragment: usize) -> Sender {
        assert!((1..=MAX_FRAGMENT).contains(&fragment), "fragment size {fragment} outside 1..={MAX_FRAGMENT}");
        Sender { inner: Arc::new((Mutex::new(Queues::default()), Condvar::new())), fragment }
    }

    pub fn send(&self, channel: u8, priority: Priority, msg: &Message) -> Result<(), SendError> {
        self.enqueue(channel, priority, msg, false)
    }

    pub fn replace(&self, channel: u8, priority: Priority, msg: &Message) -> Result<(), SendError> {
        self.enqueue(channel, priority, msg, true)
    }

    pub fn close(&self) {
        self.queues().closed = true;
        self.inner.1.notify_all();
    }

    pub fn next_frame(&self) -> Option<Vec<u8>> {
        let mut q = self.queues();
        loop {
            if let Some(level) = q.levels.iter_mut().find(|l| !l.is_empty()) {
                let pending = level.front_mut()?;
                let frame = self.take_fragment(pending);
                if pending.next == pending.count {
                    level.pop_front();
                }
                return Some(frame);
            }
            if q.closed {
                return None;
            }
            q = self.inner.1.wait(q).unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub fn pump(&self, mut w: impl Write) -> io::Result<()> {
        while let Some(frame) = self.next_frame() {
            w.write_all(&frame)?;
        }
        Ok(())
    }

    fn enqueue(&self, channel: u8, priority: Priority, msg: &Message, latest_only: bool) -> Result<(), SendError> {
        let body = msg.encode();
        if body.len() > MAX_MESSAGE {
            return Err(SendError::TooLarge(body.len()));
        }
        let count =
            u16::try_from(body.len().div_ceil(self.fragment).max(1)).map_err(|_| SendError::TooLarge(body.len()))?;
        let mut q = self.queues();
        let level = &mut q.levels[priority as usize];
        if latest_only {
            level.retain(|p| p.channel != channel || p.next > 0);
        }
        let id = q.ids[usize::from(channel)];
        q.ids[usize::from(channel)] = id.wrapping_add(1);
        q.levels[priority as usize].push_back(Pending { channel, id, body, next: 0, count });
        self.inner.1.notify_one();
        Ok(())
    }

    fn take_fragment(&self, p: &mut Pending) -> Vec<u8> {
        let start = usize::from(p.next) * self.fragment;
        let end = (start + self.fragment).min(p.body.len());
        let len = u32::try_from(FRAGMENT_HEADER + end - start).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(LENGTH_PREFIX + FRAGMENT_HEADER + end - start);
        out.extend(len.to_le_bytes());
        out.push(p.channel);
        out.extend(p.id.to_le_bytes());
        out.extend(p.next.to_le_bytes());
        out.extend(p.count.to_le_bytes());
        out.extend_from_slice(&p.body[start..end]);
        p.next += 1;
        out
    }

    fn queues(&self) -> MutexGuard<'_, Queues> {
        self.inner.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TCP_FRAGMENT, receiver::Reassembler};

    fn drain(s: &Sender) -> Vec<Message> {
        s.close();
        let mut stream = Vec::new();
        s.pump(&mut stream).unwrap();
        Reassembler::default().feed(&stream).unwrap()
    }

    #[test]
    fn cursor_jumps_ahead_of_a_large_refresh() {
        let s = Sender::new(TCP_FRAGMENT);
        let big = Message::Tile { screen: 0, x: 0, y: 0, w: 1616, h: 1008, lz4: vec![1; 480_000] };
        s.send(10, Priority::Refine, &big).unwrap();
        let first = s.next_frame().unwrap();
        s.send(1, Priority::Cursor, &Message::Cursor { screen: 0, x: 1.0, y: 2.0, shape: 0 }).unwrap();
        let second = s.next_frame().unwrap();
        assert_eq!(first[LENGTH_PREFIX], 10, "refresh started first");
        assert_eq!(second[LENGTH_PREFIX], 1, "cursor sent before the rest of the refresh");
    }

    #[test]
    fn interleaved_fragments_reassemble() {
        let s = Sender::new(1000);
        let video = Message::Video {
            screen: 0,
            key: false,
            pts_us: 7,
            data: (0..20_000u32).map(|i| i.to_le_bytes()[0]).collect(),
        };
        let tile = Message::Tile { screen: 1, x: 5, y: 6, w: 7, h: 8, lz4: vec![9; 5_000] };
        s.send(3, Priority::Video, &video).unwrap();
        s.send(4, Priority::Focused, &tile).unwrap();
        assert_eq!(drain(&s), vec![tile, video]);
    }

    #[test]
    fn replace_keeps_only_the_newest_cursor() {
        let s = Sender::new(TCP_FRAGMENT);
        for i in 0..5u8 {
            s.replace(1, Priority::Cursor, &Message::Cursor { screen: 0, x: f32::from(i), y: 0.0, shape: 0 }).unwrap();
        }
        assert_eq!(drain(&s), vec![Message::Cursor { screen: 0, x: 4.0, y: 0.0, shape: 0 }]);
    }

    #[test]
    fn replace_never_drops_a_message_that_started_sending() {
        let s = Sender::new(8);
        let first = Message::Video { screen: 0, key: true, pts_us: 1, data: vec![1; 40] };
        s.send(0, Priority::Video, &first).unwrap();
        let mut stream = s.next_frame().unwrap();
        let newest = Message::Video { screen: 0, key: false, pts_us: 2, data: vec![2; 40] };
        s.replace(0, Priority::Video, &newest).unwrap();
        s.close();
        s.pump(&mut stream).unwrap();
        assert_eq!(Reassembler::default().feed(&stream).unwrap(), vec![first, newest]);
    }

    #[test]
    fn oversized_messages_are_refused() {
        let s = Sender::new(TCP_FRAGMENT);
        let huge = Message::Video { screen: 0, key: false, pts_us: 0, data: vec![0; MAX_MESSAGE] };
        assert!(matches!(s.send(0, Priority::Video, &huge), Err(SendError::TooLarge(_))));
    }

    #[test]
    fn close_wakes_a_blocked_writer() {
        let s = Sender::new(TCP_FRAGMENT);
        let writer = {
            let s = s.clone();
            std::thread::spawn(move || s.next_frame())
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        s.close();
        assert_eq!(writer.join().unwrap(), None);
    }

    #[test]
    fn priorities_round_trip_through_u8() {
        for p in [
            Priority::Control,
            Priority::Cursor,
            Priority::Focused,
            Priority::Video,
            Priority::Background,
            Priority::Refine,
        ] {
            assert_eq!(Priority::from(p as u8), p);
        }
        assert_eq!(Priority::from(200), Priority::Refine);
    }
}
