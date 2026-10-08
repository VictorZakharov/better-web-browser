//! Independent scalar interpolation guards exact offsets and clamped-byte ties.
use super::*;

fn scalar(
    alpha: &[f32],
    [width, height]: [u32; 2],
    [left, top, right, bottom]: [u32; 4],
    [origin_x, origin_y]: [f64; 2],
    [offset_x, offset_y]: [f64; 2],
    color: &[u8],
) -> Vec<u8> {
    let mut result = vec![0; (right - left) as usize * (bottom - top) as usize * 4];
    for y in top..bottom {
        for x in left..right {
            let sx = f64::from(x) - offset_x - origin_x;
            let sy = f64::from(y) - offset_y - origin_y;
            if sx < -1.0 || sy < -1.0 || sx >= f64::from(width) || sy >= f64::from(height) {
                continue;
            }
            let first_x = sx.floor() as i64;
            let first_y = sy.floor() as i64;
            let fx = sx - first_x as f64;
            let fy = sy - first_y as f64;
            let mut coverage = 0.0;
            for dy in 0..=1 {
                for dx in 0..=1 {
                    let px = first_x + dx;
                    let py = first_y + dy;
                    if px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
                        continue;
                    }
                    let weight =
                        if dx == 0 { 1.0 - fx } else { fx } * if dy == 0 { 1.0 - fy } else { fy };
                    coverage += f64::from(alpha[(py as u32 * width + px as u32) as usize]) * weight;
                }
            }
            let index = (((y - top) * (right - left) + x - left) * 4) as usize;
            result[index + 3] = (coverage * f64::from(color[3]))
                .clamp(0.0, 255.0)
                .round_ties_even() as u8;
            if result[index + 3] != 0 {
                result[index..index + 3].copy_from_slice(&color[..3]);
            }
        }
    }
    result
}

#[test]
fn prepared_axes_equal_scalar_at_edges_fractional_origins_and_large_offsets() {
    for dimensions in [[1, 1], [2, 3], [17, 11], [67, 35]] {
        let [width, height] = dimensions;
        let alpha: Vec<_> = (0..width * height)
            .map(|index| ((index * 71 + 19) % 256) as f32 / 255.0)
            .collect();
        for bounds in [[0, 0, width + 3, height + 3], [2, 1, width + 5, height + 4]] {
            for origin in [[0.0, 0.0], [-3.0, -4.0], [1.25, -2.375]] {
                for offset in [
                    [0.0, 0.0],
                    [0.5, -0.5],
                    [-1.0, 1.0],
                    [0.49999999999999994, -0.5000000000000001],
                    [13.75, -6.125],
                    [1e20, -1e20],
                ] {
                    for color in [[0; 4], [19, 71, 211, 1], [19, 71, 211, 127], [255; 4]] {
                        assert_eq!(
                            paint(&alpha, dimensions, bounds, origin, offset, &color),
                            scalar(&alpha, dimensions, bounds, origin, offset, &color),
                            "{dimensions:?}, {bounds:?}, {origin:?}, {offset:?}, {color:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn half_ties_remain_even_and_transparent_pixels_have_no_paint_rgb() {
    let alpha = [0.5, 0.5, 0.0];
    for (opacity, expected) in [(1, 0), (3, 2), (5, 2), (255, 128)] {
        let output = paint(
            &alpha,
            [3, 1],
            [0, 0, 3, 1],
            [0.0; 2],
            [0.0; 2],
            &[10, 20, 30, opacity],
        );
        assert_eq!(output[3], expected);
        assert_eq!(output[7], expected);
        assert_eq!(&output[8..], &[0; 4]);
        if expected == 0 {
            assert_eq!(output, [0; 12]);
        }
    }
}

#[test]
fn empty_rectangles_do_not_access_samples() {
    for bounds in [[2, 1, 2, 3], [1, 2, 3, 2]] {
        assert!(paint(&[1.0], [1, 1], bounds, [0.0; 2], [0.0; 2], &[255; 4]).is_empty());
    }
}
