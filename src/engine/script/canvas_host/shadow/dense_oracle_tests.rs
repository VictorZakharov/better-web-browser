//! Frozen pre-optimization full-surface algorithm: an independent pixel oracle
//! for sparse storage and translated support, not a production fallback.
use super::*;
use image::{ImageBuffer, Luma};

fn dense_reference(
    source: &[u8],
    width: u32,
    height: u32,
    blur: f64,
    offset_x: f64,
    offset_y: f64,
    color: &[u8],
) -> Option<Vec<u8>> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || width > 16384
        || height > 16384
        || source.len() != pixels * 4
        || color.len() != 4
        || !blur.is_finite()
        || blur < 0.0
        || !offset_x.is_finite()
        || !offset_y.is_finite()
    {
        return None;
    }
    let mut result = vec![0; source.len()];
    if color[3] == 0 {
        return Some(result);
    }
    let mut bounds = [width, height, 0, 0];
    for (index, pixel) in source.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index as u32 % width;
        let y = index as u32 / width;
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x + 1);
        bounds[3] = bounds[3].max(y + 1);
    }
    if bounds[0] >= bounds[2] {
        return Some(result);
    }
    let sigma = blur.min(MAX_SIGMA * 2.0) / 2.0;
    // The Gaussian kernel is truncated at three standard deviations. Retain
    // transparent border pixels so edge extension cannot feed alpha back in.
    let padding = (sigma * 3.0).ceil() as u32 + 3;
    let mask_width = bounds[2] - bounds[0] + padding * 2;
    let mask_height = bounds[3] - bounds[1] + padding * 2;
    let mask_pixels = (mask_width as usize).checked_mul(mask_height as usize)?;
    if mask_pixels > MAX_WORKING_PIXELS {
        return None;
    }
    let mut alpha = ImageBuffer::<Luma<f32>, Vec<f32>>::new(mask_width, mask_height);
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            alpha.put_pixel(
                x - bounds[0] + padding,
                y - bounds[1] + padding,
                Luma([
                    f32::from(source[(y as usize * width as usize + x as usize) * 4 + 3]) / 255.0,
                ]),
            );
        }
    }
    // Below this deviation every off-center contribution rounds to zero in the
    // 8-bit output. Avoid underflow into image-rs's special sigma=0 default.
    let alpha = if sigma < 0.01 {
        alpha
    } else {
        image::imageops::blur(&alpha, sigma as f32)
    };
    let origin_x = f64::from(bounds[0]) - f64::from(padding);
    let origin_y = f64::from(bounds[1]) - f64::from(padding);
    for y in 0..height {
        for x in 0..width {
            let sx = f64::from(x) - offset_x - origin_x;
            let sy = f64::from(y) - offset_y - origin_y;
            if sx < -1.0 || sy < -1.0 || sx >= f64::from(mask_width) || sy >= f64::from(mask_height)
            {
                continue;
            }
            let left = sx.floor() as i64;
            let top = sy.floor() as i64;
            let fx = sx - left as f64;
            let fy = sy - top as f64;
            let mut coverage = 0.0;
            for dy in 0..=1 {
                for dx in 0..=1 {
                    let px = left + dx;
                    let py = top + dy;
                    if px < 0
                        || py < 0
                        || px >= i64::from(mask_width)
                        || py >= i64::from(mask_height)
                    {
                        continue;
                    }
                    let weight =
                        if dx == 0 { 1.0 - fx } else { fx } * if dy == 0 { 1.0 - fy } else { fy };
                    coverage += f64::from(alpha.get_pixel(px as u32, py as u32)[0]) * weight;
                }
            }
            let index = (y as usize * width as usize + x as usize) * 4;
            let opacity = (coverage * f64::from(color[3])).clamp(0.0, 255.0);
            // Uint8ClampedArray's round-to-even conversion, including half ties.
            result[index + 3] = opacity.round_ties_even() as u8;
            if result[index + 3] != 0 {
                result[index..index + 3].copy_from_slice(&color[..3]);
            }
        }
    }
    Some(result)
}

#[test]
fn sparse_sampling_and_storage_equal_original_dense_pixels_across_fractional_offsets() {
    for (width, height) in [(1_u32, 1_u32), (7, 9), (31, 17), (67, 35)] {
        for placement in [0, 1, 2] {
            let mut source = vec![0; width as usize * height as usize * 4];
            for y in 0..height {
                for x in 0..width {
                    let visible = match placement {
                        0 => x == 0 && y == 0,
                        1 => x.abs_diff(width / 2) <= 1 && y.abs_diff(height / 2) <= 1,
                        _ => (x * 7 + y * 11) % 5 == 0,
                    };
                    if visible {
                        source[((y * width + x) * 4 + 3) as usize] =
                            ((x * 19 + y * 31) % 254 + 1) as u8;
                    }
                }
            }
            for (blur, x, y) in [
                (0.0, 0.0, 0.0),
                (2.5, 0.5, -0.25),
                (7.0, -4.125, 3.75),
                (3.0, 200.0, -100.0),
                (0.01, -0.5, -0.5),
            ] {
                let color = [13, 71, 209, 160];
                assert_eq!(
                    render_layer(&source, width, height, blur, x, y, &color).unwrap(),
                    dense_reference(&source, width, height, blur, x, y, &color).unwrap(),
                    "{width}x{height} placement {placement} blur {blur} offset {x},{y}"
                );
            }
        }
    }
}
