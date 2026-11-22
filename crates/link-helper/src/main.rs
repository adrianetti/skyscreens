#![forbid(unsafe_code)]
#![expect(clippy::print_stderr, reason = "the helper reports fatal errors on stderr, which the Mac app logs")]

use link::{DEFAULT_PORT, Server};
use std::io;
use std::process::ExitCode;

const EXIT_PORT_BUSY: u8 = 3;
const USAGE: &str = "usage: skyscreens-link [--port 7243]";

fn main() -> ExitCode {
    let port = match port_from(std::env::args().skip(1)) {
        Ok(port) => port,
        Err(error) => {
            eprintln!("skyscreens-link: {error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let server = match Server::bind(port) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("skyscreens-link: cannot listen on 127.0.0.1:{port}: {error}");
            return ExitCode::from(EXIT_PORT_BUSY);
        }
    };
    match link_helper::run(&server, io::stdin().lock(), io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("skyscreens-link: {error}");
            ExitCode::FAILURE
        }
    }
}

fn port_from(mut args: impl Iterator<Item = String>) -> Result<u16, String> {
    match (args.next().as_deref(), args.next(), args.next()) {
        (None, _, _) => Ok(DEFAULT_PORT),
        (Some("--port"), Some(value), None) => value.parse().map_err(|error| format!("--port {value}: {error}")),
        _ => Err("unexpected arguments".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn the_port_defaults_to_the_protocol_port() {
        assert_eq!(port_from(args(&[])), Ok(DEFAULT_PORT));
    }

    #[test]
    fn a_port_can_be_chosen() {
        assert_eq!(port_from(args(&["--port", "9000"])), Ok(9000));
    }

    #[test]
    fn bad_arguments_are_refused() {
        for bad in [&["--port"][..], &["--port", "x"], &["--port", "1", "2"], &["--verbose"]] {
            assert!(port_from(args(bad)).is_err(), "{bad:?}");
        }
    }
}
