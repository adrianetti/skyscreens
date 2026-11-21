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

fn connect(s: &Server) -> Headset {
    let h = Headset::connect(s.local_addr(), WAIT).unwrap();
    assert!(wait_until(|| s.is_connected()), "server never saw the headset");
    h
}

#[test]
fn a_headset_gets_the_screen_list_on_connect() {
    let s = server();
    assert!(!s.set_screens(vec![code_screen()]), "nothing to send to before a headset connects");
    let mut h = connect(&s);
    assert_eq!(h.recv_until(WAIT, |_| true).unwrap(), Some(Message::Screens(vec![code_screen()])));
}

#[test]
fn video_flows_only_while_a_headset_is_connected() {
    let s = server();
    assert!(!s.send_video(0, true, 1, keyframe(1), Priority::Focused));
    let mut h = connect(&s);
    assert!(s.send_video(0, true, 7, keyframe(2), Priority::Focused));
    let got = h.recv_until(WAIT, |m| matches!(m, Message::Video { .. })).unwrap();
    assert_eq!(got, Some(Message::Video { screen: 0, key: true, pts_us: 7, data: keyframe(2) }));
    drop(h);
    assert!(wait_until(|| !s.is_connected()), "server still thinks the headset is there");
    assert!(!s.send_video(0, false, 8, vec![1], Priority::Focused));
}

#[test]
fn keyframe_requests_and_pongs_reach_the_mac() {
    let s = server();
    let (tx, rx) = mpsc::channel();
    s.on_event(move |e| {
        let _ = tx.send(e);
    });
    let h = connect(&s);
    h.send(&Message::KeyframeRequest { screen: 2 }).unwrap();
    h.send(&Message::Pong { t_us: 99 }).unwrap();
    h.send(&Message::Ping { t_us: 1 }).unwrap();
    let events: Vec<Event> = (0..2).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(events, [Event::KeyframeRequest { screen: 2 }, Event::Pong { t_us: 99 }]);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "only keyframe requests and pongs are events");
}

#[test]
fn the_mac_accepts_a_new_headset_after_one_leaves() {
    let s = server();
    s.set_screens(vec![code_screen()]);
    for _ in 0..3 {
        let mut h = connect(&s);
        assert!(matches!(h.recv_until(WAIT, |_| true).unwrap(), Some(Message::Screens(_))));
        drop(h);
        assert!(wait_until(|| !s.is_connected()));
    }
}

#[test]
fn cursor_positions_reach_the_headset() {
    let s = server();
    let mut h = connect(&s);
    assert!(s.send_cursor(0, 12.5, 40.0, 3));
    let got = h.recv_until(WAIT, |m| matches!(m, Message::Cursor { .. })).unwrap();
    assert_eq!(got, Some(Message::Cursor { screen: 0, x: 12.5, y: 40.0, shape: 3 }));
}

fn mac_that_answers_keyframe_requests(first_frame: Vec<u8>, first_is_key: bool) -> Arc<Server> {
    let s = server();
    s.set_screens(vec![code_screen()]);
    let weak = Arc::downgrade(&s);
    s.on_event(move |e| {
        if let (Event::KeyframeRequest { screen }, Some(s)) = (e, weak.upgrade()) {
            s.send_video(screen, true, 2, keyframe(3), Priority::Focused);
        }
    });
    let sending = s.clone();
    thread::spawn(move || {
        if wait_until(|| sending.is_connected()) {
            sending.send_video(0, first_is_key, 1, first_frame, Priority::Focused);
        }
    });
    s
}

fn eval(s: &Server) -> fake_headset::Report {
    run(&Config {
        addr: s.local_addr(),
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
    assert!(report.errors.iter().any(|e| e.contains("first frame is not a keyframe")), "{}", report.to_json());
}

#[test]
fn the_eval_fails_when_a_keyframe_lacks_parameter_sets() {
    let report = eval(&mac_that_answers_keyframe_requests(nal(0x65, &[1; 32]), true));
    assert!(report.errors.iter().any(|e| e.contains("lacks SPS, PPS or IDR")), "{}", report.to_json());
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
    assert!(report.errors.iter().any(|e| e.contains("could not connect")), "{}", report.to_json());
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
