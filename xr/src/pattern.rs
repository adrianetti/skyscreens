const GRID: [u8; 3] = [80, 80, 80];
const BACKGROUND: [u8; 3] = [30, 30, 30];
const WHITE: [u8; 3] = [255; 3];
const BLACK: [u8; 3] = [0; 3];
const BARS: [[u8; 3]; 7] =
    [[255, 255, 255], [0, 255, 255], [255, 255, 0], [0, 255, 0], [255, 0, 255], [0, 0, 255], [255, 0, 0]];
const PATCH: usize = 100;
const PATCH_SPACING: usize = 300;
const BAR_HEIGHT: usize = 120;

pub fn test_pattern(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; width * height * 4];
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let [blue, green, red] = color_at(i % width, i / width, width, height);
        pixel.copy_from_slice(&[blue, green, red, 255]);
    }
    pixels
}

fn color_at(x: usize, y: usize, w: usize, h: usize) -> [u8; 3] {
    let (cx, cy) = (w / 2, h / 2);
    let in_patch = |center_x: usize| x.abs_diff(center_x) < PATCH && y.abs_diff(cy) < PATCH;
    let alternate = |on: bool| if on { WHITE } else { BLACK };
    if x % PATCH == 0 || y % PATCH == 0 {
        GRID
    } else if in_patch(cx) {
        alternate((x + y) % 2 == 0)
    } else if in_patch(cx.saturating_sub(PATCH_SPACING)) {
        alternate(x % 2 == 0)
    } else if in_patch(cx + PATCH_SPACING) {
        alternate(y % 2 == 0)
    } else if y + BAR_HEIGHT > h {
        BARS[x * BARS.len() / w]
    } else {
        BACKGROUND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_is_opaque_bgra_of_the_requested_size() {
        let (w, h) = (640, 400);
        let px = test_pattern(w, h);
        assert_eq!(px.len(), w * h * 4);
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn center_patch_is_a_one_pixel_checkerboard() {
        let (w, h) = (640, 400);
        let px = test_pattern(w, h);
        let at = |x: usize, y: usize| px[(y * w + x) * 4];
        assert_ne!(at(w / 2 + 1, h / 2 + 1), at(w / 2 + 2, h / 2 + 1));
        assert_eq!(at(w / 2 + 1, h / 2 + 1), at(w / 2 + 2, h / 2 + 2));
    }
}
