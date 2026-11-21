use link::{DecodeError, Message, Priority, Reassembler, Role, Screen, Sender, StreamError};
use std::collections::HashMap;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn byte(&mut self) -> u8 {
        self.next().to_le_bytes()[0]
    }
    fn short(&mut self) -> u16 {
        u16::from_le_bytes([self.byte(), self.byte()])
    }
    fn float(&mut self) -> f32 {
        f32::from(self.short()) / 7.0
    }
    fn bytes(&mut self, max: u64) -> Vec<u8> {
        (0..self.below(max)).map(|_| self.byte()).collect()
    }
    fn size(&mut self, max: u64) -> usize {
        usize::try_from(self.below(max)).unwrap()
    }
}

fn random_message(r: &mut Rng) -> Message {
    match r.below(9) {
        0 => Message::Hello {
            version: r.short(),
            pixels_per_degree: r.float(),
            refresh_rate: r.float(),
            max_decoders: r.byte(),
            max_layers: r.byte(),
        },
        1 => Message::Screens(
            (0..r.below(20))
                .map(|_| Screen { id: r.byte(), role: Role::from(r.byte() % 4), width: r.short(), height: r.short() })
                .collect(),
        ),
        2 => Message::Video { screen: r.byte(), key: r.below(2) == 0, pts_us: r.next(), data: r.bytes(5000) },
        3 => Message::Tile {
            screen: r.byte(),
            x: r.short(),
            y: r.short(),
            w: r.short(),
            h: r.short(),
            lz4: r.bytes(3000),
        },
        4 => Message::Cursor { screen: r.byte(), x: r.float(), y: r.float(), shape: r.short() },
        5 => Message::CursorShape {
            shape: r.short(),
            hot_x: r.short(),
            hot_y: r.short(),
            w: r.short(),
            h: r.short(),
            lz4: r.bytes(500),
        },
        6 => Message::KeyframeRequest { screen: r.byte() },
        7 => Message::Ping { t_us: r.next() },
        _ => Message::Pong { t_us: r.next() },
    }
}

#[test]
fn random_messages_round_trip() {
    let mut r = Rng(0x5eed_0001);
    for case in 0..3000 {
        let m = random_message(&mut r);
        assert_eq!(Message::decode(&m.encode()), Ok(m), "case {case}");
    }
}

#[test]
fn arbitrary_bytes_never_panic_the_decoder() {
    let mut r = Rng(0x5eed_0002);
    for _ in 0..20_000 {
        let b = r.bytes(64);
        match Message::decode(&b) {
            Ok(m) => assert!(!m.encode().is_empty()),
            Err(DecodeError::Truncated | DecodeError::UnknownType(_)) => {}
        }
    }
}

#[test]
fn arbitrary_streams_never_panic_the_reassembler() {
    let mut r = Rng(0x5eed_0003);
    for _ in 0..5_000 {
        let mut reassembler = Reassembler::default();
        let mut stream = r.bytes(200);
        if r.below(2) == 0 && stream.len() >= 4 {
            stream[..4].copy_from_slice(&u32::try_from(r.below(64) + 9).unwrap().to_le_bytes());
        }
        match reassembler.feed(&stream) {
            Ok(_) | Err(StreamError::BadFragment | StreamError::MessageTooLarge | StreamError::Decode(_)) => {}
        }
    }
}

#[test]
fn every_message_arrives_once_and_in_order_per_channel() {
    let mut r = Rng(0x5eed_0004);
    for case in 0..200 {
        let sender = Sender::new(r.size(2000) + 1);
        let channels: Vec<(u8, Priority)> =
            (0..=r.below(6)).map(|_| (r.byte(), Priority::from(r.byte() % 6))).collect();
        let mut sent: HashMap<u8, Vec<Message>> = HashMap::new();
        for _ in 0..r.below(40) {
            let (channel, priority) = channels[r.size(channels.len() as u64)];
            let m = random_message(&mut r);
            sender.send(channel, priority, &m).unwrap();
            sent.entry(channel).or_default().push(m);
        }
        sender.close();
        let mut stream = Vec::new();
        sender.pump(&mut stream).unwrap();

        let mut reassembler = Reassembler::default();
        let mut got: Vec<Message> = Vec::new();
        let mut at = 0;
        while at < stream.len() {
            let n = r.size(4096) + 1;
            let end = (at + n).min(stream.len());
            got.extend(reassembler.feed(&stream[at..end]).unwrap());
            at = end;
        }
        let total: usize = sent.values().map(Vec::len).sum();
        assert_eq!(got.len(), total, "case {case}");
        for (channel, expected) in &sent {
            let mut cursor = got.iter();
            for m in expected {
                assert!(cursor.any(|g| g == m), "case {case}: channel {channel} lost or reordered {m:?}");
            }
        }
    }
}

#[test]
fn a_higher_priority_message_waits_for_at_most_one_fragment() {
    let mut r = Rng(0x5eed_0005);
    for _ in 0..200 {
        let fragment = r.size(4000) + 64;
        let sender = Sender::new(fragment);
        let low = Message::Tile { screen: 0, x: 0, y: 0, w: 1, h: 1, lz4: vec![0; fragment * (r.size(20) + 2)] };
        sender.send(9, Priority::Refine, &low).unwrap();
        let _first_low_fragment = sender.next_frame().unwrap();
        sender.send(1, Priority::Cursor, &Message::Cursor { screen: 0, x: 1.0, y: 1.0, shape: 0 }).unwrap();
        assert_eq!(sender.next_frame().unwrap()[4], 1, "cursor must be the very next fragment");
    }
}
