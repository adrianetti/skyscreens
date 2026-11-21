pub const MAC_CONTROL: u8 = 255;
pub const CURSOR: u8 = 254;
pub const HEADSET_CONTROL: u8 = 250;
pub const LANES_PER_SCREEN: u8 = 4;

pub const fn video(screen: u8) -> u8 {
    screen.wrapping_mul(LANES_PER_SCREEN)
}

pub const fn tile(screen: u8) -> u8 {
    video(screen).wrapping_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_lanes_never_collide_with_control_channels() {
        for screen in 0..16 {
            for lane in [video(screen), tile(screen)] {
                assert!(![MAC_CONTROL, CURSOR, HEADSET_CONTROL].contains(&lane), "screen {screen}");
            }
            assert_ne!(video(screen), tile(screen));
        }
    }
}
