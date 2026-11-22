#![expect(clippy::unwrap_used, reason = "test helpers: a failed setup should fail the test")]

use fake_headset::{Config, Headset, run};
use link::{Event, Message, Priority, Role, Screen, Server, annexb::START_CODE};
use std::net::TcpListener;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(5);

fn server() -> Arc<Server> {
    Server::listen(TcpListener::bind("127.0.0.1:0").unwrap()).unwrap()
}

fn code_screen() -> Screen {
    Screen { id: 0, role: Role::Code, width: 1616, height: 1008 }
}

fn nal(header: u8, body: &[u8]) -> Vec<u8> {
    [&START_CODE[..], &[header], body].concat()
}

fn keyframe(fill: u8) -> Vec<u8> {
    [nal(0x67, &[0x42, 0, 0x1f]), nal(0x68, &[0xce, 0x3c]), nal(0x65, &[fill; 64])].concat()
}

fn wait_until(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

fn connect(mac: &Server) -> Headset {
    let headset = Headset::connect(mac.local_addr(), WAIT).unwrap();
    assert!(wait_until(|| mac.is_connected()), "server never saw the headset");
    headset
}

#[test]
fn a_headset_gets_the_screen_list_on_connect() {
    let mac = server();
    assert!(!mac.set_screens(vec![code_screen()]), "nothing to send to before a headset connects");
    let mut headset = connect(&mac);
    assert_eq!(headset.recv_until(WAIT, |_| true).unwrap(), Some(Message::Screens(vec![code_screen()])));
}

#[test]
fn video_flows_only_while_a_headset_is_connected() {
    let mac = server();
    assert!(!mac.send_video(0, true, 1, keyframe(1), Priority::Focused));
    let mut headset = connect(&mac);
    assert!(mac.send_video(0, true, 7, keyframe(2), Priority::Focused));
    let got = headset.recv_until(WAIT, |message| matches!(message, Message::Video { .. })).unwrap();
    assert_eq!(got, Some(Message::Video { screen: 0, key: true, pts_us: 7, data: keyframe(2) }));
    drop(headset);
    assert!(wait_until(|| !mac.is_connected()), "server still thinks the headset is there");
    assert!(!mac.send_video(0, false, 8, vec![1], Priority::Focused));
}

#[test]
fn keyframe_requests_and_pongs_reach_the_mac() {
    let mac = server();
    let (tx, rx) = mpsc::channel();
    mac.on_event(move |event| {
        let _receiver_gone = tx.send(event);
    });
    let headset = connect(&mac);
    headset.send(&Message::KeyframeRequest { screen: 2 }).unwrap();
    headset.send(&Message::Pong { t_us: 99 }).unwrap();
    headset.send(&Message::Ping { t_us: 1 }).unwrap();
    let events: Vec<Event> = (0..3).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(events, [Event::Connected, Event::KeyframeRequest { screen: 2 }, Event::Pong { t_us: 99 }]);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "pings are not events");
    drop(headset);
    assert_eq!(rx.recv_timeout(WAIT).unwrap(), Event::Disconnected);
}

#[test]
fn the_mac_accepts_a_new_headset_after_one_leaves() {
    let mac = server();
    mac.set_screens(vec![code_screen()]);
    for _ in 0..3 {
        let mut headset = connect(&mac);
        assert!(matches!(headset.recv_until(WAIT, |_| true).unwrap(), Some(Message::Screens(_))));
        drop(headset);
        assert!(wait_until(|| !mac.is_connected()));
    }
}

#[test]
fn lossless_tiles_flow_only_while_a_headset_is_connected() {
    let mac = server();
    assert!(!mac.send_tile(0, [1, 2, 3, 4], vec![9; 8], Priority::Refine), "nobody to send to yet");
    let mut headset = connect(&mac);
    assert!(mac.send_tile(2, [10, 20, 30, 40], vec![7; 16], Priority::Refine));
    let got = headset.recv_until(WAIT, |message| matches!(message, Message::Tile { .. })).unwrap();
    assert_eq!(got, Some(Message::Tile { screen: 2, x: 10, y: 20, w: 30, h: 40, lz4: vec![7; 16] }));
}

#[test]
fn cursor_positions_reach_the_headset() {
    let mac = server();
    let mut headset = connect(&mac);
    assert!(mac.send_cursor(0, 12.5, 40.0, 3));
    let got = headset.recv_until(WAIT, |message| matches!(message, Message::Cursor { .. })).unwrap();
    assert_eq!(got, Some(Message::Cursor { screen: 0, x: 12.5, y: 40.0, shape: 3 }));
}

fn mac_that_answers_keyframe_requests(first_frame: Vec<u8>, first_is_key: bool) -> Arc<Server> {
    let mac = server();
    mac.set_screens(vec![code_screen()]);
    let weak = Arc::downgrade(&mac);
    mac.on_event(move |event| {
        if let (Event::KeyframeRequest { screen }, Some(running)) = (event, weak.upgrade()) {
            running.send_video(screen, true, 2, keyframe(3), Priority::Focused);
        }
    });
    let sending = Arc::clone(&mac);
    thread::spawn(move || {
        if wait_until(|| sending.is_connected()) {
            sending.send_video(0, first_is_key, 1, first_frame, Priority::Focused);
        }
    });
    mac
}

fn eval(mac: &Server) -> fake_headset::Report {
    run(&Config {
        addr: mac.local_addr(),
        duration: Duration::from_millis(800),
        connect_timeout: WAIT,
        request_keyframe: true,
    })
}

#[test]
fn the_eval_passes_against_a_correct_mac() {
    let report = eval(&mac_that_answers_keyframe_requests(keyframe(1), true));
    assert!(report.passed(), "{}", report.to_json());
    assert_eq!((report.frames, report.keyframes), (2, 2));
    assert!(report.keyframe_response_ms.is_some());
    assert_eq!(report.screens, vec![code_screen()]);
}

#[test]
fn the_eval_fails_when_the_first_frame_cannot_start_a_decoder() {
    let report = eval(&mac_that_answers_keyframe_requests(nal(0x41, &[1; 32]), false));
    assert!(!report.passed());
    assert!(report.errors.iter().any(|error| error.contains("first frame is not a keyframe")), "{}", report.to_json());
}

#[test]
fn the_eval_fails_when_a_keyframe_lacks_parameter_sets() {
    let report = eval(&mac_that_answers_keyframe_requests(nal(0x65, &[1; 32]), true));
    assert!(report.errors.iter().any(|error| error.contains("lacks SPS, PPS or IDR")), "{}", report.to_json());
}

#[test]
fn the_eval_fails_when_nothing_listens() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let report = run(&Config {
        addr: port,
        duration: Duration::from_millis(100),
        connect_timeout: Duration::from_millis(300),
        request_keyframe: false,
    });
    assert!(report.errors.iter().any(|error| error.contains("could not connect")), "{}", report.to_json());
}

#[test]
fn reports_are_valid_json_shaped() {
    let report = eval(&mac_that_answers_keyframe_requests(keyframe(1), true));
    let json = report.to_json();
    assert!(
        json.starts_with(r#"{"passed":true,"screens":[{"id":0,"role":"Code","width":1616,"height":1008}]"#),
        "{json}"
    );
    assert!(json.ends_with(r#""errors":[]}"#), "{json}");
}
