#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Code = 0,
    Chat = 1,
    Email = 2,
    Video = 3,
    Other = 255,
}

impl From<u8> for Role {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Code,
            1 => Self::Chat,
            2 => Self::Email,
            3 => Self::Video,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub id: u8,
    pub role: Role,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Hello { version: u16, pixels_per_degree: f32, refresh_rate: f32, max_decoders: u8, max_layers: u8 },
    Screens(Vec<Screen>),
    Video { screen: u8, key: bool, pts_us: u64, data: Vec<u8> },
    Tile { screen: u8, x: u16, y: u16, w: u16, h: u16, lz4: Vec<u8> },
    Cursor { screen: u8, x: f32, y: f32, shape: u16 },
    CursorShape { shape: u16, hot_x: u16, hot_y: u16, w: u16, h: u16, lz4: Vec<u8> },
    KeyframeRequest { screen: u8 },
    Ping { t_us: u64 },
    Pong { t_us: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnknownType(u8),
}

pub const MAX_SCREENS: usize = u8::MAX as usize;

mod tag {
    pub(super) const HELLO: u8 = 1;
    pub(super) const SCREENS: u8 = 2;
    pub(super) const VIDEO: u8 = 3;
    pub(super) const TILE: u8 = 4;
    pub(super) const CURSOR: u8 = 5;
    pub(super) const CURSOR_SHAPE: u8 = 6;
    pub(super) const KEYFRAME_REQUEST: u8 = 7;
    pub(super) const PING: u8 = 8;
    pub(super) const PONG: u8 = 9;
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::with_capacity(self.payload_len() + 32));
        match self {
            Self::Hello { version, pixels_per_degree, refresh_rate, max_decoders, max_layers } => {
                w.u8(tag::HELLO).u16(*version).f32(*pixels_per_degree).f32(*refresh_rate);
                w.u8(*max_decoders).u8(*max_layers);
            }
            Self::Screens(screens) => {
                let sent = &screens[..screens.len().min(MAX_SCREENS)];
                w.u8(tag::SCREENS).u8(u8::try_from(sent.len()).unwrap_or(u8::MAX));
                for s in sent {
                    w.u8(s.id).u8(s.role as u8).u16(s.width).u16(s.height);
                }
            }
            Self::Video { screen, key, pts_us, data } => {
                w.u8(tag::VIDEO).u8(*screen).u8(u8::from(*key)).u64(*pts_us).bytes(data);
            }
            Self::Tile { screen, x, y, w: width, h, lz4 } => {
                w.u8(tag::TILE).u8(*screen).u16(*x).u16(*y).u16(*width).u16(*h).bytes(lz4);
            }
            Self::Cursor { screen, x, y, shape } => {
                w.u8(tag::CURSOR).u8(*screen).f32(*x).f32(*y).u16(*shape);
            }
            Self::CursorShape { shape, hot_x, hot_y, w: width, h, lz4 } => {
                w.u8(tag::CURSOR_SHAPE).u16(*shape).u16(*hot_x).u16(*hot_y).u16(*width).u16(*h).bytes(lz4);
            }
            Self::KeyframeRequest { screen } => {
                w.u8(tag::KEYFRAME_REQUEST).u8(*screen);
            }
            Self::Ping { t_us } => {
                w.u8(tag::PING).u64(*t_us);
            }
            Self::Pong { t_us } => {
                w.u8(tag::PONG).u64(*t_us);
            }
        }
        w.0
    }

    fn payload_len(&self) -> usize {
        match self {
            Self::Video { data, .. } => data.len(),
            Self::Tile { lz4, .. } | Self::CursorShape { lz4, .. } => lz4.len(),
            Self::Hello { .. }
            | Self::Screens(_)
            | Self::Cursor { .. }
            | Self::KeyframeRequest { .. }
            | Self::Ping { .. }
            | Self::Pong { .. } => 0,
        }
    }

    pub fn decode(b: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader { b, i: 0 };
        Ok(match r.u8()? {
            tag::HELLO => Self::Hello {
                version: r.u16()?,
                pixels_per_degree: r.f32()?,
                refresh_rate: r.f32()?,
                max_decoders: r.u8()?,
                max_layers: r.u8()?,
            },
            tag::SCREENS => {
                let n = r.u8()?;
                let mut screens = Vec::with_capacity(usize::from(n));
                for _ in 0..n {
                    screens.push(Screen { id: r.u8()?, role: Role::from(r.u8()?), width: r.u16()?, height: r.u16()? });
                }
                Self::Screens(screens)
            }
            tag::VIDEO => Self::Video { screen: r.u8()?, key: r.u8()? != 0, pts_us: r.u64()?, data: r.rest() },
            tag::TILE => {
                Self::Tile { screen: r.u8()?, x: r.u16()?, y: r.u16()?, w: r.u16()?, h: r.u16()?, lz4: r.rest() }
            }
            tag::CURSOR => Self::Cursor { screen: r.u8()?, x: r.f32()?, y: r.f32()?, shape: r.u16()? },
            tag::CURSOR_SHAPE => Self::CursorShape {
                shape: r.u16()?,
                hot_x: r.u16()?,
                hot_y: r.u16()?,
                w: r.u16()?,
                h: r.u16()?,
                lz4: r.rest(),
            },
            tag::KEYFRAME_REQUEST => Self::KeyframeRequest { screen: r.u8()? },
            tag::PING => Self::Ping { t_us: r.u64()? },
            tag::PONG => Self::Pong { t_us: r.u64()? },
            t => return Err(DecodeError::UnknownType(t)),
        })
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let (chunk, _) = self.b.get(self.i..).and_then(<[u8]>::split_first_chunk::<N>).ok_or(DecodeError::Truncated)?;
        self.i += N;
        Ok(*chunk)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.take()?))
    }
    fn rest(&mut self) -> Vec<u8> {
        let v = self.b[self.i..].to_vec();
        self.i = self.b.len();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_kind() -> Vec<Message> {
        vec![
            Message::Hello { version: 1, pixels_per_degree: 25.0, refresh_rate: 90.0, max_decoders: 4, max_layers: 16 },
            Message::Screens(vec![
                Screen { id: 0, role: Role::Code, width: 1616, height: 1008 },
                Screen { id: 3, role: Role::Video, width: 1296, height: 736 },
            ]),
            Message::Video { screen: 1, key: true, pts_us: 123_456_789, data: vec![0, 0, 0, 1, 0x67, 9, 9] },
            Message::Tile { screen: 2, x: 330, y: 700, w: 320, h: 24, lz4: vec![1, 2, 3] },
            Message::Cursor { screen: 0, x: 812.5, y: 40.25, shape: 3 },
            Message::CursorShape { shape: 3, hot_x: 4, hot_y: 1, w: 32, h: 32, lz4: vec![7; 40] },
            Message::KeyframeRequest { screen: 2 },
            Message::Ping { t_us: 42 },
            Message::Pong { t_us: 42 },
        ]
    }

    #[test]
    fn every_message_round_trips() {
        for m in every_kind() {
            assert_eq!(Message::decode(&m.encode()), Ok(m));
        }
    }

    #[test]
    fn truncated_input_is_an_error_not_a_panic() {
        let b = Message::Cursor { screen: 0, x: 1.0, y: 2.0, shape: 0 }.encode();
        for n in 0..b.len() {
            assert_eq!(Message::decode(&b[..n]), Err(DecodeError::Truncated), "length {n}");
        }
    }

    #[test]
    fn unknown_tags_are_rejected() {
        assert_eq!(Message::decode(&[0]), Err(DecodeError::UnknownType(0)));
        assert_eq!(Message::decode(&[200, 1, 2]), Err(DecodeError::UnknownType(200)));
    }

    #[test]
    fn unknown_roles_decode_as_other() {
        let mut b = Message::Screens(vec![Screen { id: 0, role: Role::Code, width: 16, height: 16 }]).encode();
        b[3] = 77;
        let Ok(Message::Screens(s)) = Message::decode(&b) else { panic!("screens expected") };
        assert_eq!(s[0].role, Role::Other);
    }

    #[test]
    fn the_wire_format_is_stable() {
        let hello =
            Message::Hello { version: 1, pixels_per_degree: 25.0, refresh_rate: 90.0, max_decoders: 4, max_layers: 16 };
        assert_eq!(hello.encode(), [1, 1, 0, 0, 0, 0xc8, 0x41, 0, 0, 0xb4, 0x42, 4, 16]);
        let video = Message::Video { screen: 2, key: true, pts_us: 0x0102, data: vec![0xaa] };
        assert_eq!(video.encode(), [3, 2, 1, 2, 1, 0, 0, 0, 0, 0, 0, 0xaa]);
        assert_eq!(Message::KeyframeRequest { screen: 9 }.encode(), [7, 9]);
    }
}
