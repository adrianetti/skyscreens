//! A BGRA test image for judging sharpness before real screens arrive: a grid,
//! 1-pixel checkers and stripes, and color bars.

#[allow(dead_code)]
pub fn test_pattern(w: usize, h: usize) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (w / 2, h / 2);
            let v: [u8; 3] = if x % 100 == 0 || y % 100 == 0 {
                [80, 80, 80]
            } else if x.abs_diff(cx) < 100 && y.abs_diff(cy) < 100 {
                if (x + y) % 2 == 0 { [255; 3] } else { [0; 3] }
            } else if x.abs_diff(cx - 300) < 100 && y.abs_diff(cy) < 100 {
                if x % 2 == 0 { [255; 3] } else { [0; 3] }
            } else if x.abs_diff(cx + 300) < 100 && y.abs_diff(cy) < 100 {
                if y % 2 == 0 { [255; 3] } else { [0; 3] }
            } else if y > h - 120 {
                const BARS: [[u8; 3]; 7] = [[255, 255, 255], [0, 255, 255], [255, 255, 0], [0, 255, 0], [255, 0, 255], [0, 0, 255], [255, 0, 0]];
                BARS[x * 7 / w]
            } else {
                [30, 30, 30]
            };
            let i = (y * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[v[0], v[1], v[2], 255]);
        }
    }
    px
}
