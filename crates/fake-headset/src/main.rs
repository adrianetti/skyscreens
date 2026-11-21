#![expect(clippy::print_stdout, clippy::print_stderr, reason = "command line tool that reports on stdout")]

use fake_headset::{Config, run};
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: fake-headset [--port 7243] [--seconds 8] [--connect-timeout 15] [--no-keyframe-request]";

fn main() -> ExitCode {
    let config = match parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let report = run(&config);
    println!("{}", report.to_json());
    if report.passed() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut config = Config {
        addr: SocketAddr::from((Ipv4Addr::LOCALHOST, link::DEFAULT_PORT)),
        duration: Duration::from_secs(8),
        connect_timeout: Duration::from_secs(15),
        request_keyframe: true,
    };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--port" => config.addr.set_port(value()?.parse().map_err(|e| format!("--port: {e}"))?),
            "--seconds" => config.duration = seconds(&value()?)?,
            "--connect-timeout" => config.connect_timeout = seconds(&value()?)?,
            "--no-keyframe-request" => config.request_keyframe = false,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(config)
}

fn seconds(v: &str) -> Result<Duration, String> {
    v.parse::<f64>().ok().and_then(|s| Duration::try_from_secs_f64(s).ok()).ok_or(format!("not a duration: {v}"))
}
