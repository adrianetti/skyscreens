#![forbid(unsafe_code)]

pub mod bridge;

use bridge::{EVENT_SIZE, Record, encode_event, encode_listening, read_record};
use link::{Message, Server};
use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::thread;

pub fn run(server: &Server, mut input: impl Read, events: impl Write + Send + 'static) -> io::Result<()> {
    let (event_tx, event_rx) = mpsc::channel::<[u8; EVENT_SIZE]>();
    let writer = thread::Builder::new().name("link-events".into()).spawn(move || write_events(&event_rx, events))?;
    let listening = event_tx.send(encode_listening(server.local_addr().port()));
    let forwarding = event_tx.clone();
    server.on_event(move |event| {
        let _ignored_after_shutdown = forwarding.send(encode_event(event));
    });
    let result = listening.map_err(io::Error::other).and_then(|()| forward_records(server, &mut input));
    server.on_event(|_| {});
    drop(event_tx);
    let written = writer.join().map_err(|panic| io::Error::other(format!("event writer panicked: {panic:?}")))?;
    result.and(written)
}

fn forward_records(server: &Server, input: &mut impl Read) -> io::Result<()> {
    while let Some(record) = read_record(input)? {
        forward(server, record);
    }
    Ok(())
}

fn write_events(events: &mpsc::Receiver<[u8; EVENT_SIZE]>, mut out: impl Write) -> io::Result<()> {
    for event in events {
        out.write_all(&event)?;
        out.flush()?;
    }
    Ok(())
}

pub fn forward(server: &Server, record: Record) -> bool {
    let priority = record.priority;
    match record.message {
        Message::Screens(screens) => server.set_screens(screens),
        Message::Video { screen, key, pts_us, data } => server.send_video(screen, key, pts_us, data, priority),
        Message::Tile { screen, x, y, w, h, lz4 } => server.send_tile(screen, [x, y, w, h], lz4, priority),
        Message::Cursor { screen, x, y, shape } => server.send_cursor(screen, x, y, shape),
        Message::Hello { .. }
        | Message::CursorShape { .. }
        | Message::KeyframeRequest { .. }
        | Message::Ping { .. }
        | Message::Pong { .. } => false,
    }
}
