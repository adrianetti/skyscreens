pub const TEST_SCREEN_W: u16 = 1616;
pub const TEST_SCREEN_H: u16 = 1008;
pub const SCREEN_DEGREES: f32 = 58.0;
pub const DISTANCE_M: f32 = 1.5;

pub fn width_m(degrees: f32, distance_m: f32) -> f32 {
    2.0 * distance_m * (degrees.to_radians() / 2.0).tan()
}

pub fn quad_size_m(width_px: u16, height_px: u16) -> (f32, f32) {
    let width = width_m(SCREEN_DEGREES, DISTANCE_M);
    (width, width * f32::from(height_px) / f32::from(width_px.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_58_degree_screen_at_1_5_m_is_about_1_66_m_wide() {
        assert!((width_m(58.0, 1.5) - 1.663).abs() < 0.001);
    }

    #[test]
    fn quads_keep_the_screen_aspect_ratio() {
        let (w, h) = quad_size_m(TEST_SCREEN_W, TEST_SCREEN_H);
        let expected = f32::from(TEST_SCREEN_W) / f32::from(TEST_SCREEN_H);
        assert!((w / h - expected).abs() < 1e-4);
    }

    #[test]
    fn a_zero_width_screen_does_not_divide_by_zero() {
        assert!(quad_size_m(0, 100).1.is_finite());
    }
}
