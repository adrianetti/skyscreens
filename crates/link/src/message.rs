//! Messages and their binary encoding (little-endian, no external dependencies).

/// What a screen is for, mirrored from the Mac app's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Code = 0,
    Chat = 1,
    Email = 2,
    Video = 3,
    Other = 255,
}

impl Role {
    fn from_u8(v: u8) -> Role {
        match v {
            0 => Role::Code,
            1 => Role::Chat,
            2 => Role::Email,
            3 => Role::Video,
            _ => Role::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub id: u8,
    pub role: Role,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// First message in each direction. The headset describes itself so the Mac can size screens.
    Hello { version: u16, pixels_per_degree: f32, refresh_rate: f32, max_decoders: u8, max_layers: u8 },
    /// The full set of screens. Sent on connect and whenever it changes.
    Screens(Vec<Screen>),
    /// One H.264 access unit in Annex B form.
    Video { screen: u8, key: bool, pts_us: u64, data: Vec<u8> },
    /// A rectangle of BGRA pixels, compressed as one raw LZ4 block.
    Tile { screen: u8, x: u16, y: u16, w: u16, h: u16, lz4: Vec<u8> },
    /// Cursor position in screen pixels. `screen` 255 means the cursor is on the laptop screen.
    Cursor { screen: u8, x: f32, y: f32, shape: u16 },
    /// A cursor image (BGRA, LZ4), sent once per shape.
    CursorShape { shape: u16, hot_x: u16, hot_y: u16, w: u16, h: u16, lz4: Vec<u8> },
    /// The headset asks for a keyframe, for example after taking over a decoder.
    KeyframeRequest { screen: u8 },
    Ping { t_us: u64 },
    Pong { t_us: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnknownType(u8),
}

mod tag {
    pub const HELLO: u8 = 1;
    pub const SCREENS: u8 = 2;
    pub const VIDEO: u8 = 3;
    pub const TILE: u8 = 4;
    pub const CURSOR: u8 = 5;
    pub const CURSOR_SHAPE: u8 = 6;
    pub const KEYFRAME_REQUEST: u8 = 7;
    pub const PING: u8 = 8;
    pub const PONG: u8 = 9;
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.payload_len() + 32);
        match self {
            Message::Hello { version, pixels_per_degree, refresh_rate, max_decoders, max_layers } => {
                b.push(tag::HELLO);
                b.extend(version.to_le_bytes());
                b.extend(pixels_per_degree.to_le_bytes());
                b.extend(refresh_rate.to_le_bytes());
                b.push(*max_decoders);
                b.push(*max_layers);
            }
            Message::Screens(screens) => {
                b.push(tag::SCREENS);
                b.push(screens.len() as u8);
                for s in screens {
                    b.push(s.id);
                    b.push(s.role as u8);
                    b.extend(s.width.to_le_bytes());
                    b.extend(s.height.to_le_bytes());
                }
            }
            Message::Video { screen, key, pts_us, data } => {
                b.push(tag::VIDEO);
                b.push(*screen);
                b.push(*key as u8);
                b.extend(pts_us.to_le_bytes());
                b.extend_from_slice(data);
            }
            Message::Tile { screen, x, y, w, h, lz4 } => {
                b.push(tag::TILE);
                b.push(*screen);
                for v in [x, y, w, h] {
                    b.extend(v.to_le_bytes());
                }
                b.extend_from_slice(lz4);
            }
            Message::Cursor { screen, x, y, shape } => {
                b.push(tag::CURSOR);
                b.push(*screen);
                b.extend(x.to_le_bytes());
                b.extend(y.to_le_bytes());
                b.extend(shape.to_le_bytes());
            }
            Message::CursorShape { shape, hot_x, hot_y, w, h, lz4 } => {
                b.push(tag::CURSOR_SHAPE);
                for v in [shape, hot_x, hot_y, w, h] {
                    b.extend(v.to_le_bytes());
                }
                b.extend_from_slice(lz4);
            }
            Message::KeyframeRequest { screen } => {
                b.push(tag::KEYFRAME_REQUEST);
                b.push(*screen);
            }
            Message::Ping { t_us } => {
                b.push(tag::PING);
                b.extend(t_us.to_le_bytes());
            }
            Message::Pong { t_us } => {
                b.push(tag::PONG);
                b.extend(t_us.to_le_bytes());
            }
        }
        b
    }

    fn payload_len(&self) -> usize {
        match self {
            Message::Video { data, .. } => data.len(),
            Message::Tile { lz4, .. } | Message::CursorShape { lz4, .. } => lz4.len(),
            _ => 0,
        }
    }

    pub fn decode(b: &[u8]) -> Result<Message, DecodeError> {
        let mut r = Reader { b, i: 0 };
        Ok(match r.u8()? {
            tag::HELLO => Message::Hello {
                version: r.u16()?,
                pixels_per_degree: r.f32()?,
                refresh_rate: r.f32()?,
                max_decoders: r.u8()?,
                max_layers: r.u8()?,
            },
            tag::SCREENS => {
                let n = r.u8()?;
                let mut screens = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    screens.push(Screen { id: r.u8()?, role: Role::from_u8(r.u8()?), width: r.u16()?, height: r.u16()? });
                }
                Message::Screens(screens)
            }
            tag::VIDEO => Message::Video { screen: r.u8()?, key: r.u8()? != 0, pts_us: r.u64()?, data: r.rest() },
            tag::TILE => Message::Tile { screen: r.u8()?, x: r.u16()?, y: r.u16()?, w: r.u16()?, h: r.u16()?, lz4: r.rest() },
            tag::CURSOR => Message::Cursor { screen: r.u8()?, x: r.f32()?, y: r.f32()?, shape: r.u16()? },
            tag::CURSOR_SHAPE => Message::CursorShape {
                shape: r.u16()?,
                hot_x: r.u16()?,
                hot_y: r.u16()?,
                w: r.u16()?,
                h: r.u16()?,
                lz4: r.rest(),
            },
            tag::KEYFRAME_REQUEST => Message::KeyframeRequest { screen: r.u8()? },
            tag::PING => Message::Ping { t_us: r.u64()? },
            tag::PONG => Message::Pong { t_us: r.u64()? },
            t => return Err(DecodeError::UnknownType(t)),
        })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let s = self.b.get(self.i..self.i + N).ok_or(DecodeError::Truncated)?;
        self.i += N;
        Ok(s.try_into().unwrap())
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

    #[test]
    fn every_message_round_trips() {
        let all = vec![
            Message::Hello { version: 1, pixels_per_degree: 25.0, refresh_rate: 90.0, max_decoders: 4, max_layers: 16 },
            Message::Screens(vec![Screen { id: 0, role: Role::Code, width: 1616, height: 1008 }, Screen { id: 3, role: Role::Video, width: 1296, height: 736 }]),
            Message::Video { screen: 1, key: true, pts_us: 123_456_789, data: vec![0, 0, 0, 1, 0x67, 9, 9] },
            Message::Tile { screen: 2, x: 330, y: 700, w: 320, h: 24, lz4: vec![1, 2, 3] },
            Message::Cursor { screen: 0, x: 812.5, y: 40.25, shape: 3 },
            Message::CursorShape { shape: 3, hot_x: 4, hot_y: 1, w: 32, h: 32, lz4: vec![7; 40] },
            Message::KeyframeRequest { screen: 2 },
            Message::Ping { t_us: 42 },
            Message::Pong { t_us: 42 },
        ];
        for m in all {
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
}
