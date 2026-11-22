use link::{DecodeError, Message, Priority, Reassembler, Role, Screen, Sender, StreamError};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

fn pumped(sender: &Sender) -> Result<Vec<u8>, String> {
    let sender = sender.clone();
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stream = Vec::new();
        let pumped = sender.pump(&mut stream).map(|()| stream);
        let _receiver_gone = done.send(pumped);
    });
    result
        .recv_timeout(Duration::from_secs(10))
        .map_err(|stalled| format!("pump stalled: {stalled}"))?
        .map_err(|error| format!("pump failed: {error}"))
}

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
        usize::try_from(self.below(max)).unwrap_or(usize::MAX)
    }
}

fn random_message(rng: &mut Rng) -> Message {
    match rng.below(9) {
        0 => Message::Hello {
            version: rng.short(),
            pixels_per_degree: rng.float(),
            refresh_rate: rng.float(),
            max_decoders: rng.byte(),
            max_layers: rng.byte(),
        },
        1 => Message::Screens(
            (0..rng.below(20))
                .map(|_| Screen {
                    id: rng.byte(),
                    role: Role::from(rng.byte() % 4),
                    width: rng.short(),
                    height: rng.short(),
                })
                .collect(),
        ),
        2 => Message::Video { screen: rng.byte(), key: rng.below(2) == 0, pts_us: rng.next(), data: rng.bytes(5000) },
        3 => Message::Tile {
            screen: rng.byte(),
            x: rng.short(),
            y: rng.short(),
            w: rng.short(),
            h: rng.short(),
            lz4: rng.bytes(3000),
        },
        4 => Message::Cursor { screen: rng.byte(), x: rng.float(), y: rng.float(), shape: rng.short() },
        5 => Message::CursorShape {
            shape: rng.short(),
            hot_x: rng.short(),
            hot_y: rng.short(),
            w: rng.short(),
            h: rng.short(),
            lz4: rng.bytes(500),
        },
        6 => Message::KeyframeRequest { screen: rng.byte() },
        7 => Message::Ping { t_us: rng.next() },
        _ => Message::Pong { t_us: rng.next() },
    }
}

#[test]
fn random_messages_round_trip() {
    let mut rng = Rng(0x5eed_0001);
    for case in 0..3000 {
        let message = random_message(&mut rng);
        assert_eq!(Message::decode(&message.encode()), Ok(message), "case {case}");
    }
}

#[test]
fn arbitrary_bytes_never_panic_the_decoder() {
    let mut rng = Rng(0x5eed_0002);
    for _ in 0..20_000 {
        let bytes = rng.bytes(64);
        match Message::decode(&bytes) {
            Ok(message) => assert!(!message.encode().is_empty(), "decoded messages re-encode"),
            Err(DecodeError::Truncated | DecodeError::UnknownType(_)) => {}
        }
    }
}

#[test]
fn arbitrary_streams_never_panic_the_reassembler() {
    let mut rng = Rng(0x5eed_0003);
    for _ in 0..5_000 {
        let mut reassembler = Reassembler::default();
        let mut stream = rng.bytes(200);
        if rng.below(2) == 0 && stream.len() >= 4 {
            stream[..4].copy_from_slice(&u32::try_from(rng.below(64) + 9).unwrap().to_le_bytes());
        }
        match reassembler.feed(&stream) {
            Ok(_) | Err(StreamError::BadFragment | StreamError::MessageTooLarge | StreamError::Decode(_)) => {}
        }
    }
}

#[test]
fn every_message_arrives_once_and_in_order_per_channel() {
    let mut rng = Rng(0x5eed_0004);
    for case in 0..200 {
        let sender = Sender::new(rng.size(2000) + 1);
        let channels: Vec<(u8, Priority)> =
            (0..=rng.below(6)).map(|_| (rng.byte(), Priority::from(rng.byte() % 6))).collect();
        let mut sent: HashMap<u8, Vec<Message>> = HashMap::new();
        for _ in 0..rng.below(40) {
            let (channel, priority) = channels[rng.size(u64::try_from(channels.len()).unwrap_or(u64::MAX))];
            let message = random_message(&mut rng);
            sender.send(channel, priority, &message).unwrap();
            sent.entry(channel).or_default().push(message);
        }
        sender.close();
        let stream = pumped(&sender).unwrap();

        let mut reassembler = Reassembler::default();
        let mut got: Vec<Message> = Vec::new();
        let mut at = 0;
        while at < stream.len() {
            let n = rng.size(4096) + 1;
            let end = (at + n).min(stream.len());
            got.extend(reassembler.feed(&stream[at..end]).unwrap());
            at = end;
        }
        let total: usize = sent.values().map(Vec::len).sum();
        assert_eq!(got.len(), total, "case {case}");
        for (channel, expected) in &sent {
            let mut cursor = got.iter();
            for message in expected {
                assert!(
                    cursor.any(|got_message| got_message == message),
                    "case {case}: channel {channel} lost or reordered {message:?}"
                );
            }
        }
    }
}

#[test]
fn a_higher_priority_message_waits_for_at_most_one_fragment() {
    let mut rng = Rng(0x5eed_0005);
    for _ in 0..200 {
        let fragment = rng.size(4000) + 64;
        let sender = Sender::new(fragment);
        let low = Message::Tile { screen: 0, x: 0, y: 0, w: 1, h: 1, lz4: vec![0; fragment * (rng.size(20) + 2)] };
        sender.send(9, Priority::Refine, &low).unwrap();
        let _first_low_fragment = sender.next_frame().unwrap();
        sender.send(1, Priority::Cursor, &Message::Cursor { screen: 0, x: 1.0, y: 1.0, shape: 0 }).unwrap();
        assert_eq!(sender.next_frame().unwrap()[4], 1, "cursor must be the very next fragment");
    }
}
