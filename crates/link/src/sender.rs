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

impl Priority {
    pub const fn index(self) -> usize {
        match self {
            Self::Control => 0,
            Self::Cursor => 1,
            Self::Focused => 2,
            Self::Video => 3,
            Self::Background => 4,
            Self::Refine => 5,
        }
    }
}

impl From<Priority> for u8 {
    fn from(priority: Priority) -> Self {
        match priority {
            Priority::Control => 0,
            Priority::Cursor => 1,
            Priority::Focused => 2,
            Priority::Video => 3,
            Priority::Background => 4,
            Priority::Refine => 5,
        }
    }
}

impl From<u8> for Priority {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Control,
            1 => Self::Cursor,
            2 => Self::Focused,
            3 => Self::Video,
            4 => Self::Background,
            _ => Self::Refine,
        }
    }
}

const LEVELS: usize = 6;

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    TooLarge(usize),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(size) => write!(f, "message of {size} bytes exceeds the limit"),
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

impl Queues {
    fn push(&mut self, priority: Priority, channel: u8, body: Vec<u8>, count: u16, latest_only: bool) {
        let id = self.ids[usize::from(channel)];
        self.ids[usize::from(channel)] = id.wrapping_add(1);
        let level = &mut self.levels[priority.index()];
        if latest_only {
            level.retain(|pending| pending.channel != channel || pending.next > 0);
        }
        level.push_back(Pending { channel, id, body, next: 0, count });
    }
}

impl Default for Queues {
    fn default() -> Self {
        Self { levels: Default::default(), ids: [0; 256], closed: false }
    }
}

#[derive(Clone, Debug)]
pub struct Sender {
    inner: Arc<(Mutex<Queues>, Condvar)>,
    fragment: usize,
    max_message: usize,
}

impl Sender {
    pub fn new(fragment: usize) -> Self {
        Self::with_max_message(fragment, MAX_MESSAGE)
    }

    pub fn with_max_message(fragment: usize, max_message: usize) -> Self {
        assert!((1..=MAX_FRAGMENT).contains(&fragment), "fragment size {fragment} outside 1..={MAX_FRAGMENT}");
        Self { inner: Arc::new((Mutex::new(Queues::default()), Condvar::new())), fragment, max_message }
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
        let mut queues = self.queues();
        loop {
            if let Some(level) = queues.levels.iter_mut().find(|level| !level.is_empty()) {
                let pending = level.front_mut()?;
                let frame = self.take_fragment(pending);
                if pending.next == pending.count {
                    level.pop_front();
                }
                return Some(frame);
            }
            if queues.closed {
                return None;
            }
            queues = self.inner.1.wait(queues).unwrap_or_else(PoisonError::into_inner);
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
        if body.len() > self.max_message {
            return Err(SendError::TooLarge(body.len()));
        }
        let Ok(count) = u16::try_from(body.len().div_ceil(self.fragment).max(1)) else {
            return Err(SendError::TooLarge(body.len()));
        };
        self.queues().push(priority, channel, body, count, latest_only);
        self.inner.1.notify_one();
        Ok(())
    }

    fn take_fragment(&self, pending: &mut Pending) -> Vec<u8> {
        let start = usize::from(pending.next) * self.fragment;
        let end = (start + self.fragment).min(pending.body.len());
        let len = u32::try_from(FRAGMENT_HEADER + end - start).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(LENGTH_PREFIX + FRAGMENT_HEADER + end - start);
        out.extend(len.to_le_bytes());
        out.push(pending.channel);
        out.extend(pending.id.to_le_bytes());
        out.extend(pending.next.to_le_bytes());
        out.extend(pending.count.to_le_bytes());
        out.extend_from_slice(&pending.body[start..end]);
        pending.next += 1;
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
    use std::sync::mpsc;
    use std::time::Duration;

    fn within<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        let (done, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _receiver_gone = done.send(work());
        });
        result.recv_timeout(Duration::from_secs(5)).expect("the sender stopped making progress")
    }

    fn drain(sender: &Sender) -> Vec<Message> {
        sender.close();
        let sender = sender.clone();
        let stream = within(move || {
            let mut stream = Vec::new();
            sender.pump(&mut stream).unwrap();
            stream
        });
        Reassembler::default().feed(&stream).unwrap()
    }

    fn drain_bytes(sender: &Sender) -> Vec<u8> {
        sender.close();
        let sender = sender.clone();
        within(move || {
            let mut stream = Vec::new();
            sender.pump(&mut stream).unwrap();
            stream
        })
    }

    fn next(sender: &Sender) -> Option<Vec<u8>> {
        let sender = sender.clone();
        within(move || sender.next_frame())
    }

    #[test]
    fn cursor_jumps_ahead_of_a_large_refresh() {
        let sender = Sender::new(TCP_FRAGMENT);
        let big = Message::Tile { screen: 0, x: 0, y: 0, w: 1616, h: 1008, lz4: vec![1; 480_000] };
        sender.send(10, Priority::Refine, &big).unwrap();
        let first = next(&sender).unwrap();
        sender.send(1, Priority::Cursor, &Message::Cursor { screen: 0, x: 1.0, y: 2.0, shape: 0 }).unwrap();
        let second = next(&sender).unwrap();
        assert_eq!(first[LENGTH_PREFIX], 10, "refresh started first");
        assert_eq!(second[LENGTH_PREFIX], 1, "cursor sent before the rest of the refresh");
    }

    #[test]
    fn interleaved_fragments_reassemble() {
        let sender = Sender::new(1000);
        let video = Message::Video {
            screen: 0,
            key: false,
            pts_us: 7,
            data: (0..20_000u32).map(|i| i.to_le_bytes()[0]).collect(),
        };
        let tile = Message::Tile { screen: 1, x: 5, y: 6, w: 7, h: 8, lz4: vec![9; 5_000] };
        sender.send(3, Priority::Video, &video).unwrap();
        sender.send(4, Priority::Focused, &tile).unwrap();
        assert_eq!(drain(&sender), vec![tile, video]);
    }

    #[test]
    fn replace_keeps_only_the_newest_cursor() {
        let sender = Sender::new(TCP_FRAGMENT);
        for i in 0..5u8 {
            sender
                .replace(1, Priority::Cursor, &Message::Cursor { screen: 0, x: f32::from(i), y: 0.0, shape: 0 })
                .unwrap();
        }
        assert_eq!(drain(&sender), vec![Message::Cursor { screen: 0, x: 4.0, y: 0.0, shape: 0 }]);
    }

    #[test]
    fn replace_never_drops_a_message_that_started_sending() {
        let sender = Sender::new(8);
        let first = Message::Video { screen: 0, key: true, pts_us: 1, data: vec![1; 40] };
        sender.send(0, Priority::Video, &first).unwrap();
        let mut stream = next(&sender).unwrap();
        let newest = Message::Video { screen: 0, key: false, pts_us: 2, data: vec![2; 40] };
        sender.replace(0, Priority::Video, &newest).unwrap();
        let rest = drain_bytes(&sender);
        stream.extend(rest);
        assert_eq!(Reassembler::default().feed(&stream).unwrap(), vec![first, newest]);
    }

    #[test]
    fn the_message_size_limit_is_exact() {
        let ping = Message::Ping { t_us: 1 };
        let size = ping.encode().len();
        assert!(Sender::with_max_message(4, size).send(0, Priority::Control, &ping).is_ok(), "at the limit");
        let refused = Sender::with_max_message(4, size - 1).send(0, Priority::Control, &ping);
        assert_eq!(refused, Err(SendError::TooLarge(size)), "one byte over");
        assert_eq!(SendError::TooLarge(size).to_string(), format!("message of {size} bytes exceeds the limit"));
    }

    #[test]
    fn the_default_limit_is_the_protocol_limit() {
        let huge = Message::Video { screen: 0, key: false, pts_us: 0, data: vec![0; MAX_MESSAGE] };
        assert!(matches!(Sender::new(TCP_FRAGMENT).send(0, Priority::Video, &huge), Err(SendError::TooLarge(_))));
    }

    #[test]
    fn close_wakes_a_blocked_writer() {
        let sender = Sender::new(TCP_FRAGMENT);
        let (done, result) = mpsc::channel();
        let blocked = sender.clone();
        std::thread::spawn(move || {
            let _receiver_gone = done.send(blocked.next_frame());
        });
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err(), "nothing to send yet, so it blocks");
        sender.close();
        assert_eq!(result.recv_timeout(Duration::from_secs(5)), Ok(None), "closing wakes it with None");
    }

    #[test]
    fn priorities_round_trip_through_u8() {
        for priority in [
            Priority::Control,
            Priority::Cursor,
            Priority::Focused,
            Priority::Video,
            Priority::Background,
            Priority::Refine,
        ] {
            assert_eq!(Priority::from(u8::from(priority)), priority);
            assert!(priority.index() < LEVELS);
        }
        assert_eq!(Priority::from(200), Priority::Refine);
    }
}
