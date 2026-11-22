#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Code = 0,
    Chat = 1,
    Email = 2,
    Video = 3,
    Other = 255,
}

impl From<Role> for u8 {
    fn from(role: Role) -> Self {
        match role {
            Role::Code => 0,
            Role::Chat => 1,
            Role::Email => 2,
            Role::Video => 3,
            Role::Other => 255,
        }
    }
}

impl From<u8> for Role {
    fn from(value: u8) -> Self {
        match value {
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

pub const MAX_SCREENS: usize = 255;

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
        let mut w = Writer(Vec::new());
        match self {
            Self::Hello { version, pixels_per_degree, refresh_rate, max_decoders, max_layers } => {
                w.u8(tag::HELLO).u16(*version).f32(*pixels_per_degree).f32(*refresh_rate);
                w.u8(*max_decoders).u8(*max_layers);
            }
            Self::Screens(screens) => {
                let sent = screens.len().min(MAX_SCREENS);
                w.u8(tag::SCREENS).u8(u8::try_from(sent).unwrap_or(u8::MAX));
                for screen in screens.iter().take(sent) {
                    w.u8(screen.id).u8(u8::from(screen.role)).u16(screen.width).u16(screen.height);
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

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader { bytes };
        Ok(match reader.u8()? {
            tag::HELLO => Self::Hello {
                version: reader.u16()?,
                pixels_per_degree: reader.f32()?,
                refresh_rate: reader.f32()?,
                max_decoders: reader.u8()?,
                max_layers: reader.u8()?,
            },
            tag::SCREENS => {
                let n = reader.u8()?;
                let mut screens = Vec::with_capacity(usize::from(n));
                for _ in 0..n {
                    screens.push(Screen {
                        id: reader.u8()?,
                        role: Role::from(reader.u8()?),
                        width: reader.u16()?,
                        height: reader.u16()?,
                    });
                }
                Self::Screens(screens)
            }
            tag::VIDEO => {
                Self::Video { screen: reader.u8()?, key: reader.u8()? != 0, pts_us: reader.u64()?, data: reader.rest() }
            }
            tag::TILE => Self::Tile {
                screen: reader.u8()?,
                x: reader.u16()?,
                y: reader.u16()?,
                w: reader.u16()?,
                h: reader.u16()?,
                lz4: reader.rest(),
            },
            tag::CURSOR => {
                Self::Cursor { screen: reader.u8()?, x: reader.f32()?, y: reader.f32()?, shape: reader.u16()? }
            }
            tag::CURSOR_SHAPE => Self::CursorShape {
                shape: reader.u16()?,
                hot_x: reader.u16()?,
                hot_y: reader.u16()?,
                w: reader.u16()?,
                h: reader.u16()?,
                lz4: reader.rest(),
            },
            tag::KEYFRAME_REQUEST => Self::KeyframeRequest { screen: reader.u8()? },
            tag::PING => Self::Ping { t_us: reader.u64()? },
            tag::PONG => Self::Pong { t_us: reader.u64()? },
            other => return Err(DecodeError::UnknownType(other)),
        })
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }
    fn u16(&mut self, value: u16) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    fn f32(&mut self, value: f32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    fn bytes(&mut self, value: &[u8]) -> &mut Self {
        self.0.extend_from_slice(value);
        self
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let (chunk, rest) = self.bytes.split_first_chunk::<N>().ok_or(DecodeError::Truncated)?;
        self.bytes = rest;
        Ok(*chunk)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        let [byte] = self.take::<1>()?;
        Ok(byte)
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
        std::mem::take(&mut self.bytes).to_vec()
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
        for message in every_kind() {
            assert_eq!(Message::decode(&message.encode()), Ok(message));
        }
    }

    #[test]
    fn truncated_input_is_an_error_not_a_panic() {
        let bytes = Message::Cursor { screen: 0, x: 1.0, y: 2.0, shape: 0 }.encode();
        for n in 0..bytes.len() {
            assert_eq!(Message::decode(&bytes[..n]), Err(DecodeError::Truncated), "length {n}");
        }
    }

    #[test]
    fn unknown_tags_are_rejected() {
        assert_eq!(Message::decode(&[0]), Err(DecodeError::UnknownType(0)));
        assert_eq!(Message::decode(&[200, 1, 2]), Err(DecodeError::UnknownType(200)));
    }

    #[test]
    fn unknown_roles_decode_as_other() {
        let mut bytes = Message::Screens(vec![Screen { id: 0, role: Role::Code, width: 16, height: 16 }]).encode();
        bytes[3] = 77;
        let Ok(Message::Screens(screens)) = Message::decode(&bytes) else { panic!("screens expected") };
        assert_eq!(screens[0].role, Role::Other);
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

    #[test]
    fn every_known_role_survives_its_byte() {
        for role in [Role::Code, Role::Chat, Role::Email, Role::Video, Role::Other] {
            assert_eq!(Role::from(u8::from(role)), role, "{role:?}");
        }
    }
}
