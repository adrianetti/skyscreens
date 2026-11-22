#![forbid(unsafe_code)]

use link::{
    Message, PROTOCOL_VERSION, Priority, Reassembler, Screen, SendError, Sender, TCP_FRAGMENT, annexb, channel,
};
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_millis(100);
const POLL: Duration = Duration::from_millis(100);
const ALIGNMENT: u16 = 16;

#[derive(Debug)]
pub struct Headset {
    stream: TcpStream,
    sender: Sender,
    writer: Option<JoinHandle<io::Result<()>>>,
    reassembler: Reassembler,
    inbox: VecDeque<Message>,
    buf: Vec<u8>,
}

impl Headset {
    pub fn connect(addr: SocketAddr, timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(connected) => break connected,
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => thread::sleep(RETRY_DELAY),
            }
        };
        stream.set_nodelay(true)?;
        let sender = Sender::new(TCP_FRAGMENT);
        let writer = {
            let (pumping, socket) = (sender.clone(), stream.try_clone()?);
            thread::spawn(move || pumping.pump(socket))
        };
        let headset = Self {
            stream,
            sender,
            writer: Some(writer),
            reassembler: Reassembler::default(),
            inbox: VecDeque::new(),
            buf: vec![0; 256 * 1024],
        };
        headset.send(&hello()).map_err(io::Error::other)?;
        Ok(headset)
    }

    pub fn send(&self, message: &Message) -> Result<(), SendError> {
        self.sender.send(channel::HEADSET_CONTROL, Priority::Control, message)
    }

    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        if let Some(message) = self.inbox.pop_front() {
            return Ok(Some(message));
        }
        self.stream.set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.stream.read(&mut self.buf) {
            Ok(0) => Err(ErrorKind::ConnectionAborted.into()),
            Ok(n) => {
                let messages = self
                    .reassembler
                    .feed(&self.buf[..n])
                    .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
                self.inbox.extend(messages);
                Ok(self.inbox.pop_front())
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn recv_until(&mut self, timeout: Duration, wanted: impl Fn(&Message) -> bool) -> io::Result<Option<Message>> {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Some(message) = self.recv(left.min(POLL))?
                && wanted(&message)
            {
                return Ok(Some(message));
            }
        }
        Ok(None)
    }
}

impl Drop for Headset {
    fn drop(&mut self) {
        self.sender.close();
        let _already_closed = self.stream.shutdown(Shutdown::Both);
        if let Some(writer) = self.writer.take() {
            let _writer_result = writer.join();
        }
    }
}

pub fn hello() -> Message {
    Message::Hello {
        version: PROTOCOL_VERSION,
        pixels_per_degree: 25.0,
        refresh_rate: 90.0,
        max_decoders: 16,
        max_layers: 16,
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub duration: Duration,
    pub connect_timeout: Duration,
    pub request_keyframe: bool,
}

#[derive(Debug, Default)]
pub struct Report {
    pub screens: Vec<Screen>,
    pub frames: usize,
    pub keyframes: usize,
    pub bytes: usize,
    pub first_frame_ms: Option<f64>,
    pub keyframe_response_ms: Option<f64>,
    pub errors: Vec<String>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn to_json(&self) -> String {
        let screens: Vec<String> = self
            .screens
            .iter()
            .map(|screen| {
                format!(
                    r#"{{"id":{},"role":"{:?}","width":{},"height":{}}}"#,
                    screen.id, screen.role, screen.width, screen.height
                )
            })
            .collect();
        let errors: Vec<String> = self.errors.iter().map(|error| format!("{error:?}")).collect();
        format!(
            r#"{{"passed":{},"screens":[{}],"frames":{},"keyframes":{},"bytes":{},"first_frame_ms":{},"keyframe_response_ms":{},"errors":[{}]}}"#,
            self.passed(),
            screens.join(","),
            self.frames,
            self.keyframes,
            self.bytes,
            json_number(self.first_frame_ms),
            json_number(self.keyframe_response_ms),
            errors.join(","),
        )
    }

    fn on_video(&mut self, key: bool, data: &[u8], since_connect: Duration) {
        self.frames += 1;
        self.bytes += data.len();
        if self.frames == 1 {
            self.first_frame_ms = Some(ms(since_connect));
            if !key {
                self.errors.push("first frame is not a keyframe".into());
            }
        }
        if key {
            self.keyframes += 1;
            if !annexb::is_decodable_keyframe(data) {
                self.errors.push(format!("keyframe {} lacks SPS, PPS or IDR", self.keyframes));
            }
        }
    }

    fn check_screens(&mut self) {
        if self.screens.is_empty() {
            self.errors.push("no screen list received".into());
        }
        for screen in &self.screens {
            let (width, height) = (screen.width, screen.height);
            if width == 0 || height == 0 || width % ALIGNMENT != 0 || height % ALIGNMENT != 0 {
                self.errors.push(format!("screen {} is {width}x{height}, not a non-zero multiple of 16", screen.id));
            }
        }
    }
}

pub fn run(config: &Config) -> Report {
    let mut report = Report::default();
    let mut headset = match Headset::connect(config.addr, config.connect_timeout) {
        Ok(h) => h,
        Err(error) => {
            report.errors.push(format!("could not connect to {}: {error}", config.addr));
            return report;
        }
    };
    let connected = Instant::now();
    let mut requested: Option<Instant> = None;
    while connected.elapsed() < config.duration {
        match headset.recv(POLL) {
            Ok(Some(Message::Screens(screens))) => report.screens = screens,
            Ok(Some(Message::Video { key, pts_us, data, .. })) => {
                report.on_video(key, &data, connected.elapsed());
                if key && let Some(asked) = requested.take() {
                    report.keyframe_response_ms = Some(ms(asked.elapsed()));
                }
                if let Err(error) = headset.send(&Message::Pong { t_us: pts_us }) {
                    report.errors.push(format!("pong not sent: {error}"));
                }
                if config.request_keyframe && report.frames == 1 {
                    if let Err(error) = headset.send(&Message::KeyframeRequest { screen: 0 }) {
                        report.errors.push(format!("keyframe request not sent: {error}"));
                    }
                    requested = Some(Instant::now());
                }
            }
            Ok(_) => {}
            Err(error) => {
                report.errors.push(format!("connection lost: {error}"));
                break;
            }
        }
    }
    report.check_screens();
    if report.frames == 0 {
        report.errors.push("no video frames received".into());
    }
    if requested.is_some() {
        report.errors.push("keyframe request was not answered".into());
    }
    report
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn json_number(number: Option<f64>) -> String {
    number.map_or_else(|| "null".into(), |value| format!("{value:.1}"))
}
