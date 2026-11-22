use link::{
    Message, PROTOCOL_VERSION, Priority, Reassembler, Screen, SendError, Sender, TCP_FRAGMENT, annexb, channel,
};
use std::collections::VecDeque;
use std::fmt::Write as _;
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
                Ok(s) => break s,
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => thread::sleep(RETRY_DELAY),
            }
        };
        stream.set_nodelay(true)?;
        let sender = Sender::new(TCP_FRAGMENT);
        let writer = {
            let (s, w) = (sender.clone(), stream.try_clone()?);
            thread::spawn(move || s.pump(w))
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

    pub fn send(&self, m: &Message) -> Result<(), SendError> {
        self.sender.send(channel::HEADSET_CONTROL, Priority::Control, m)
    }

    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        if let Some(m) = self.inbox.pop_front() {
            return Ok(Some(m));
        }
        self.stream.set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.stream.read(&mut self.buf) {
            Ok(0) => Err(ErrorKind::ConnectionAborted.into()),
            Ok(n) => {
                let messages =
                    self.reassembler.feed(&self.buf[..n]).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
                self.inbox.extend(messages);
                Ok(self.inbox.pop_front())
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn recv_until(&mut self, timeout: Duration, wanted: impl Fn(&Message) -> bool) -> io::Result<Option<Message>> {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Some(m) = self.recv(left.min(POLL))?
                && wanted(&m)
            {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }
}

impl Drop for Headset {
    fn drop(&mut self) {
        self.sender.close();
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(w) = self.writer.take() {
            let _ = w.join();
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
            .map(|s| format!(r#"{{"id":{},"role":"{:?}","width":{},"height":{}}}"#, s.id, s.role, s.width, s.height))
            .collect();
        let errors: Vec<String> = self.errors.iter().map(|e| format!("{e:?}")).collect();
        let mut out = String::new();
        let _ = write!(
            out,
            r#"{{"passed":{},"screens":[{}],"frames":{},"keyframes":{},"bytes":{},"first_frame_ms":{},"keyframe_response_ms":{},"errors":[{}]}}"#,
            self.passed(),
            screens.join(","),
            self.frames,
            self.keyframes,
            self.bytes,
            json_number(self.first_frame_ms),
            json_number(self.keyframe_response_ms),
            errors.join(","),
        );
        out
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
        for s in &self.screens {
            if s.width == 0 || s.height == 0 || s.width % ALIGNMENT != 0 || s.height % ALIGNMENT != 0 {
                self.errors.push(format!("screen {} is {}x{}, not a non-zero multiple of 16", s.id, s.width, s.height));
            }
        }
    }
}

pub fn run(config: &Config) -> Report {
    let mut report = Report::default();
    let mut headset = match Headset::connect(config.addr, config.connect_timeout) {
        Ok(h) => h,
        Err(e) => {
            report.errors.push(format!("could not connect to {}: {e}", config.addr));
            return report;
        }
    };
    let connected = Instant::now();
    let mut requested: Option<Instant> = None;
    while connected.elapsed() < config.duration {
        match headset.recv(POLL) {
            Ok(Some(Message::Screens(s))) => report.screens = s,
            Ok(Some(Message::Video { key, pts_us, data, .. })) => {
                report.on_video(key, &data, connected.elapsed());
                if key && let Some(t) = requested.take() {
                    report.keyframe_response_ms = Some(ms(t.elapsed()));
                }
                let _ = headset.send(&Message::Pong { t_us: pts_us });
                if config.request_keyframe && report.frames == 1 {
                    let _ = headset.send(&Message::KeyframeRequest { screen: 0 });
                    requested = Some(Instant::now());
                }
            }
            Ok(_) => {}
            Err(e) => {
                report.errors.push(format!("connection lost: {e}"));
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

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn json_number(v: Option<f64>) -> String {
    v.map_or_else(|| "null".into(), |v| format!("{v:.1}"))
}
