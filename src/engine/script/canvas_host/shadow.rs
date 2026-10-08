//! Canvas shadows reuse image-rs's MIT/Apache-2.0 separable Gaussian filter.
//! HTML requires sigma = shadowBlur / 2 and permits an implementation sigma cap.
//! Transparent padding prevents the image library's edge extension from creating
//! a shadow halo at a clipped bitmap edge.

use super::*;

#[cfg(test)]
mod dense_oracle_tests;
mod kernel_cache;
mod sampling;
mod sampling_bounds;

pub(super) struct Layer {
    pub(super) bounds: [u32; 4],
    pub(super) pixels: Vec<u8>,
}

impl Layer {
    fn empty() -> Self {
        Self {
            bounds: [0; 4],
            pixels: Vec::new(),
        }
    }
}

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

pub(super) fn render_layer(
    source: &[u8],
    width: u32,
    height: u32,
    blur: f64,
    offset_x: f64,
    offset_y: f64,
    color: &[u8],
) -> Option<Vec<u8>> {
    let layer = render_sparse_layer(source, width, height, blur, offset_x, offset_y, color)?;
    let mut result = vec![0; source.len()];
    let [left, top, right, bottom] = layer.bounds;
    let row_bytes = (right - left) as usize * 4;
    if row_bytes == 0 {
        return Some(result);
    }
    for (y, row) in (top..bottom).zip(layer.pixels.chunks_exact(row_bytes)) {
        let start = (y as usize * width as usize + left as usize) * 4;
        result[start..start + row_bytes].copy_from_slice(row);
    }
    Some(result)
}

pub(super) fn render_sparse_layer(
    source: &[u8],
    width: u32,
    height: u32,
    blur: f64,
    offset_x: f64,
    offset_y: f64,
    color: &[u8],
) -> Option<Layer> {
    render_region_layer(
        source,
        [width, height],
        [0, 0, width, height],
        blur,
        [offset_x, offset_y],
        color,
    )
}

/// A source rectangle is storage only: omitted pixels are transparent. Its
/// origin remains in whole-bitmap coordinates throughout Gaussian sampling.
pub(super) fn render_region_layer(
    source: &[u8],
    [width, height]: [u32; 2],
    [source_left, source_top, source_right, source_bottom]: [u32; 4],
    blur: f64,
    [offset_x, offset_y]: [f64; 2],
    color: &[u8],
) -> Option<Layer> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    let source_width = source_right.checked_sub(source_left)?;
    let source_height = source_bottom.checked_sub(source_top)?;
    let source_pixels = (source_width as usize).checked_mul(source_height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || width > 16384
        || height > 16384
        || source_right > width
        || source_bottom > height
        || source.len() != source_pixels.checked_mul(4)?
        || color.len() != 4
        || !blur.is_finite()
        || blur < 0.0
        || !offset_x.is_finite()
        || !offset_y.is_finite()
    {
        return None;
    }
    if color[3] == 0 {
        return Some(Layer::empty());
    }
    let mut bounds = [width, height, 0, 0];
    for (index, pixel) in source.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index as u32 % source_width + source_left;
        let y = index as u32 / source_width + source_top;
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x + 1);
        bounds[3] = bounds[3].max(y + 1);
    }
    if bounds[0] >= bounds[2] {
        return Some(Layer::empty());
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
    let origin_x = f64::from(bounds[0]) - f64::from(padding);
    let origin_y = f64::from(bounds[1]) - f64::from(padding);
    let [left, top, right, bottom] = sampling_bounds::translated_support(
        [width, height],
        [mask_width, mask_height],
        [origin_x, origin_y],
        [offset_x, offset_y],
    );
    if left >= right || top >= bottom {
        return Some(Layer::empty());
    }
    let mut source_alpha =
        Vec::with_capacity((bounds[2] - bounds[0]) as usize * (bounds[3] - bounds[1]) as usize);
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            let index =
                (y - source_top) as usize * source_width as usize + (x - source_left) as usize;
            source_alpha.push(source[index * 4 + 3]);
        }
    }
    // Below this deviation every off-center contribution rounds to zero in the
    // 8-bit output. Avoid underflow into image-rs's special sigma=0 default.
    let alpha = kernel_cache::gaussian(
        source_alpha,
        bounds[2] - bounds[0],
        bounds[3] - bounds[1],
        padding,
        sigma,
    );
    // Sampling outside this translated support is exactly transparent. Keep
    // the same Gaussian/interpolation arithmetic inside, without scanning the
    // entire destination for a small shadow on a large texture.
    let result = sampling::paint(
        alpha.as_raw(),
        [mask_width, mask_height],
        [left, top, right, bottom],
        [origin_x, origin_y],
        [offset_x, offset_y],
        color,
    );
    Some(Layer {
        bounds: [left, top, right, bottom],
        pixels: result,
    })
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
