#![expect(clippy::unwrap_used, reason = "test helpers: a failed setup should fail the test")]

use link_helper::bridge::{EVENT_SIZE, event_kind};
use std::io::Read;
use std::net::TcpListener;
use std::process::{Command, Stdio};

fn helper(port: u16) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_skyscreens-link"));
    command.args(["--port", &port.to_string()]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn the_helper_announces_its_port_and_exits_when_stdin_closes() {
    let port = free_port();
    let mut child = helper(port).spawn().unwrap();
    let mut event = [0u8; EVENT_SIZE];
    child.stdout.take().unwrap().read_exact(&mut event).unwrap();
    assert_eq!(event[0], event_kind::LISTENING, "first event: {event:?}");
    assert_eq!(u64::from_le_bytes(event[2..].try_into().unwrap()), u64::from(port));
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success(), "a closed stdin is a clean shutdown");
}

#[test]
fn a_busy_port_exits_with_status_3() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let output = helper(taken.local_addr().unwrap().port()).output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot listen"));
}

#[test]
fn bad_arguments_exit_with_status_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_skyscreens-link")).arg("--bogus").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
