#![expect(clippy::unwrap_used, reason = "test helpers: a failed setup should fail the test")]

use fake_headset::Headset;
use link::{Event, Message, Priority, Role, Screen, Server};
use link_helper::bridge::{EVENT_SIZE, MAX_RECORD, Record, encode_event, encode_listening, encode_record, read_record};
use std::io::{Cursor, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(5);

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _infallible = write!(out, "{byte:02x}");
        out
    })
}

fn vector(name: &str) -> String {
    let vectors = include_str!("vectors.json");
    let at = vectors.find(&format!("\"name\": \"{name}\"")).unwrap();
    let hex_at = at + vectors[at..].find("\"hex\": \"").unwrap() + 8;
    vectors[hex_at..hex_at + vectors[hex_at..].find('"').unwrap()].to_owned()
}

fn code_screen() -> Screen {
    Screen { id: 0, role: Role::Code, width: 1616, height: 1008 }
}

fn video() -> Message {
    Message::Video { screen: 0, key: true, pts_us: 258, data: vec![0, 0, 0, 1] }
}

#[test]
fn records_match_the_golden_vectors_swift_also_checks() {
    let screens = Message::Screens(vec![code_screen()]);
    assert_eq!(hex(&encode_record(Priority::Control, &screens)), vector("one code screen, control priority"));
    assert_eq!(hex(&encode_record(Priority::Focused, &video())), vector("keyframe video, focused priority"));
}

#[test]
fn events_match_the_golden_vectors_swift_also_checks() {
    let cases = [
        (encode_event(Event::KeyframeRequest { screen: 2 }), "keyframe request for screen 2"),
        (encode_event(Event::Pong { t_us: 4242 }), "pong"),
        (encode_event(Event::Connected), "connected"),
        (encode_event(Event::Disconnected), "disconnected"),
        (encode_listening(7243), "listening on 7243"),
    ];
    for (bytes, name) in cases {
        assert_eq!(hex(&bytes), vector(name), "{name}");
    }
}

#[test]
fn records_round_trip_and_end_cleanly() {
    let stream = [encode_record(Priority::Focused, &video()), encode_record(Priority::Refine, &video())].concat();
    let mut input = Cursor::new(stream);
    assert_eq!(read_record(&mut input).unwrap(), Some(Record { priority: Priority::Focused, message: video() }));
    assert_eq!(read_record(&mut input).unwrap(), Some(Record { priority: Priority::Refine, message: video() }));
    assert_eq!(read_record(&mut input).unwrap(), None);
}

#[test]
fn malformed_records_are_errors_not_panics() {
    let truncated = encode_record(Priority::Focused, &video())[..9].to_vec();
    let cases = [truncated, 1u32.to_le_bytes().to_vec(), u32::MAX.to_le_bytes().to_vec(), vec![3, 0, 0, 0, 2, 250, 0]];
    for bytes in cases {
        let error = read_record(&mut Cursor::new(bytes.clone())).unwrap_err();
        assert!(matches!(error.kind(), ErrorKind::InvalidData | ErrorKind::UnexpectedEof), "{bytes:?}: {error}");
    }
}

#[test]
fn the_record_size_limit_is_exact() {
    let at_limit = u32::try_from(MAX_RECORD).unwrap().to_le_bytes();
    let over_limit = u32::try_from(MAX_RECORD + 1).unwrap().to_le_bytes();
    let accepted_then_truncated = read_record(&mut Cursor::new(at_limit.to_vec())).unwrap_err();
    assert_eq!(accepted_then_truncated.kind(), ErrorKind::UnexpectedEof, "a record of MAX_RECORD bytes is allowed");
    let rejected = read_record(&mut Cursor::new(over_limit.to_vec())).unwrap_err();
    assert_eq!(rejected.kind(), ErrorKind::InvalidData, "one byte more is refused before reading it");
    assert_eq!(MAX_RECORD, link::MAX_MESSAGE + 1, "a record is one priority byte plus a message");
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn read_errors_are_not_mistaken_for_the_end_of_input() {
    let error = read_record(&mut FailingReader).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::BrokenPipe);
}

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn events(&self) -> Vec<[u8; EVENT_SIZE]> {
        self.0.lock().unwrap().chunks_exact(EVENT_SIZE).map(|chunk| chunk.try_into().unwrap()).collect()
    }
}

fn eventually(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn the_helper_forwards_mac_records_and_reports_headset_events() {
    let server = Server::listen(TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
    let port = server.local_addr().port();
    let events = SharedBuffer::default();
    let (reader, mut writer) = std::io::pipe().unwrap();
    let helper = {
        let (server, events) = (Arc::clone(&server), events.clone());
        thread::spawn(move || link_helper::run(&server, reader, events))
    };
    assert!(eventually(|| events.events().first() == Some(&encode_listening(port))), "no listening event");

    writer.write_all(&encode_record(Priority::Control, &Message::Screens(vec![code_screen()]))).unwrap();
    let mut headset = Headset::connect(server.local_addr(), WAIT).unwrap();
    assert!(eventually(|| events.events().contains(&encode_event(Event::Connected))), "no connected event");
    assert_eq!(headset.recv_until(WAIT, |_| true).unwrap(), Some(Message::Screens(vec![code_screen()])));

    writer.write_all(&encode_record(Priority::Focused, &video())).unwrap();
    assert_eq!(headset.recv_until(WAIT, |message| matches!(message, Message::Video { .. })).unwrap(), Some(video()));

    headset.send(&Message::Pong { t_us: 4242 }).unwrap();
    assert!(eventually(|| events.events().contains(&encode_event(Event::Pong { t_us: 4242 }))), "no pong event");

    drop(headset);
    assert!(eventually(|| events.events().contains(&encode_event(Event::Disconnected))), "no disconnected event");
    drop(writer);
    helper.join().unwrap().unwrap();
}

#[test]
fn the_helper_stops_on_a_malformed_record() {
    let server = Server::listen(TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
    let garbage = Cursor::new(vec![3, 0, 0, 0, 2, 250, 0]);
    let error = link_helper::run(&server, garbage, SharedBuffer::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}
