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

    #[test]
    fn every_screen_has_its_own_lanes() {
        let lanes: Vec<u8> = (0..16).flat_map(|screen| [video(screen), tile(screen)]).collect();
        let mut unique = lanes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), lanes.len(), "lanes collide: {lanes:?}");
        assert_eq!((video(1), tile(1)), (4, 5), "lanes are spaced by LANES_PER_SCREEN");
    }
}
