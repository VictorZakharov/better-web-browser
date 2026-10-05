//! Canvas shadows reuse image-rs's MIT/Apache-2.0 separable Gaussian filter.
//! HTML requires sigma = shadowBlur / 2 and permits an implementation sigma cap.
//! Transparent padding prevents the image library's edge extension from creating
//! a shadow halo at a clipped bitmap edge.

use super::*;
use image::{ImageBuffer, Luma};

const MAX_SIGMA: f64 = 64.0;
const MAX_WORKING_PIXELS: usize = 8 * 1024 * 1024;

pub(super) fn render(args: &[JsValue]) -> JsValue {
    let width = argument_id(args, 2);
    let height = argument_id(args, 3);
    let Some(source) = args.get(1).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let Some(blur) = args.get(4).and_then(JsValue::as_number) else {
        return JsValue::Null;
    };
    let Some(offset_x) = args.get(5).and_then(JsValue::as_number) else {
        return JsValue::Null;
    };
    let Some(offset_y) = args.get(6).and_then(JsValue::as_number) else {
        return JsValue::Null;
    };
    let Some(color) = args.get(7).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    render_layer(source, width, height, blur, offset_x, offset_y, color)
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn render_layer(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unblurred_shadow_moves_alpha_and_preserves_fractional_offsets() {
        let mut source = vec![0; 4 * 4 * 4];
        source[3] = 255;
        let shadow = render_layer(&source, 4, 4, 0.0, 1.5, 1.0, &[0, 0, 255, 255]).unwrap();
        assert_eq!(&shadow[(4 + 1) * 4..(4 + 1) * 4 + 4], &[0, 0, 255, 128]);
        assert_eq!(&shadow[(4 + 2) * 4..(4 + 2) * 4 + 4], &[0, 0, 255, 128]);
        assert_eq!(shadow[3], 0);
    }

    #[test]
    fn gaussian_shadow_has_symmetric_falloff_and_transparent_padding() {
        let mut source = vec![0; 17 * 17 * 4];
        source[(8 * 17 + 8) * 4 + 3] = 255;
        let shadow = render_layer(&source, 17, 17, 4.0, 0.0, 0.0, &[9, 44, 200, 255]).unwrap();
        let at = |x: usize, y: usize| shadow[(y * 17 + x) * 4 + 3];
        assert!(at(8, 8) > at(10, 8));
        assert!(at(10, 8) > 0);
        assert_eq!(at(6, 8), at(10, 8));
        assert_eq!(at(8, 6), at(8, 10));
        let edge = render_layer(&[255; 4], 1, 1, 4.0, 0.0, 0.0, &[255; 4]).unwrap();
        assert!(
            edge[3] < 30,
            "edge extension must not create an opaque halo"
        );
    }

    #[test]
    fn rejects_invalid_storage_and_bounds_before_image_allocation() {
        assert!(render_layer(&[0; 4], u32::MAX, 1, 0.0, 0.0, 0.0, &[255; 4]).is_none());
        assert!(render_layer(&[0; 3], 1, 1, 0.0, 0.0, 0.0, &[255; 4]).is_none());
        assert!(render_layer(&[0; 4], 1, 1, f64::NAN, 0.0, 0.0, &[255; 4]).is_none());
        assert!(render_layer(&[0; 4], 1, 1, 0.0, f64::INFINITY, 0.0, &[255; 4]).is_none());
    }

    #[test]
    fn subnormal_blur_does_not_select_library_default_and_sigma_is_bounded() {
        let exact = render_layer(&[255; 4], 1, 1, 0.0, 0.0, 0.0, &[255; 4]).unwrap();
        assert_eq!(
            render_layer(&[255; 4], 1, 1, f64::MIN_POSITIVE, 0.0, 0.0, &[255; 4]).unwrap(),
            exact
        );
        let capped = render_layer(&[255; 4], 1, 1, MAX_SIGMA * 2.0, 0.0, 0.0, &[255; 4]).unwrap();
        assert_eq!(
            render_layer(&[255; 4], 1, 1, f64::MAX, 0.0, 0.0, &[255; 4]).unwrap(),
            capped
        );
    }
}
