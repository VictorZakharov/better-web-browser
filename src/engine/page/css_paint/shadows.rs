//! CSS box-shadow masks reuse tiny-skia coverage and image's Gaussian filter.
//! Sigma is half the blur radius (CSS Backgrounds 3 §6.1.2), not a custom kernel.
use super::*;
use image::{ImageBuffer, Luma};
type Alpha = ImageBuffer<Luma<f32>, Vec<f32>>;

fn mask(width: u32, height: u32, rect: RectF, radius: f32) -> Option<skia::Mask> {
    let mut mask = skia::Mask::new(width, height)?;
    if rect.width > 0.0 && rect.height > 0.0 {
        mask.fill_path(
            &rounded_rect(rect.width, rect.height, radius)?,
            skia::FillRule::Winding,
            true,
            Transform::from_translate(rect.x, rect.y),
        );
    }
    Some(mask)
}

pub(super) fn raster(item: &DisplayItem, width: u32, height: u32) -> Option<DecodedImage> {
    let DisplayItem::CssBoxShadow {
        bounds,
        edge,
        offset,
        blur,
        spread,
        radius,
        color,
        inset,
    } = item
    else {
        return None;
    };
    let scale = width as f32 / bounds.width;
    let (blur, spread, radius) = (blur * scale, spread * scale, radius * scale);
    let offset = offset.map(|value| value * scale);
    // Three float alpha planes plus masks/output are temporary. Bound that cost
    // independently of the retained BGRA cache and reject pathological kernels.
    if blur > 1024.0 {
        return None;
    }
    let padding = if *inset {
        (blur * 1.5).ceil() as u32
    } else {
        0
    };
    let work_width = width.checked_add(padding.checked_mul(2)?)?;
    let work_height = height.checked_add(padding.checked_mul(2)?)?;
    if work_width > MAX_DECODED_IMAGE_DIMENSION
        || work_height > MAX_DECODED_IMAGE_DIMENSION
        || u64::from(work_width) * u64::from(work_height) * 20 > MAX_PAINT_BYTES as u64
    {
        return None;
    }
    let local = RectF {
        x: (edge.x - bounds.x) * scale + padding as f32,
        y: (edge.y - bounds.y) * scale + padding as f32,
        width: edge.width * scale,
        height: edge.height * scale,
    };
    let growth = if *inset { -spread } else { spread };
    let shape = RectF {
        x: local.x + offset[0] - growth,
        y: local.y + offset[1] - growth,
        width: (local.width + 2.0 * growth).max(0.0),
        height: (local.height + 2.0 * growth).max(0.0),
    };
    let shape_radius = spread_radius(radius, growth);
    let source = mask(work_width, work_height, shape, shape_radius)?;
    let alpha = source
        .data()
        .iter()
        .map(|value| {
            let coverage = f32::from(*value) / 255.0;
            if *inset { 1.0 - coverage } else { coverage }
        })
        .collect();
    let alpha = Alpha::from_raw(work_width, work_height, alpha)?;
    let alpha = if blur > 0.0 {
        image::imageops::blur(&alpha, blur / 2.0)
    } else {
        alpha
    };
    let clip = mask(work_width, work_height, local, radius)?;
    let bytes = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
    let mut bgra = vec![0u8; bytes];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let source_at = (y + padding as usize) * work_width as usize + x + padding as usize;
            let coverage = f32::from(clip.data()[source_at]) / 255.0;
            let coverage = if *inset { coverage } else { 1.0 - coverage };
            let opacity =
                (alpha.as_raw()[source_at] * coverage).clamp(0.0, 1.0) * f32::from(color.alpha);
            let target = &mut bgra[(y * width as usize + x) * 4..][..4];
            target[0] = (f32::from(color.blue) * opacity / 255.0).round() as u8;
            target[1] = (f32::from(color.green) * opacity / 255.0).round() as u8;
            target[2] = (f32::from(color.red) * opacity / 255.0).round() as u8;
            target[3] = opacity.round() as u8;
        }
    }
    Some(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}

fn spread_radius(radius: f32, spread: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    if spread > radius {
        radius + spread * (1.0 - (1.0 - radius / spread).powi(3))
    } else {
        (radius + spread).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::Color;
    fn item(inset: bool, blur: f32, spread: f32) -> DisplayItem {
        let bounds = if inset {
            RectF {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            }
        } else {
            RectF {
                x: -10.0,
                y: -10.0,
                width: 40.0,
                height: 40.0,
            }
        };
        DisplayItem::CssBoxShadow {
            bounds,
            edge: RectF {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
            offset: [0.0; 2],
            blur,
            spread,
            radius: 0.0,
            color: Color::rgb(255, 0, 0),
            inset,
        }
    }
    fn px(image: &DecodedImage, x: usize, y: usize) -> &[u8] {
        &image.bgra[(y * image.width as usize + x) * 4..][..4]
    }
    #[test]
    fn inset_spread_paints_inside_padding_edge_and_keeps_center_clear() {
        let image = raster(&item(true, 0.0, 2.0), 20, 20).unwrap();
        assert_eq!(px(&image, 0, 10), &[0, 0, 255, 255]);
        assert_eq!(px(&image, 2, 10), &[0, 0, 0, 0]);
        assert_eq!(px(&image, 10, 10), &[0, 0, 0, 0]);
    }
    #[test]
    fn outer_spread_knocks_out_original_border_box() {
        let image = raster(&item(false, 0.0, 3.0), 40, 40).unwrap();
        assert_eq!(px(&image, 8, 20), &[0, 0, 255, 255]);
        assert_eq!(px(&image, 10, 20), &[0, 0, 0, 0]);
        assert_eq!(px(&image, 6, 20), &[0, 0, 0, 0]);
    }
    #[test]
    fn gaussian_shadow_is_soft_and_premultiplied() {
        let image = raster(&item(true, 6.0, 0.0), 20, 20).unwrap();
        assert!(px(&image, 0, 10)[3] > px(&image, 4, 10)[3]);
        assert!(px(&image, 4, 10)[3] > px(&image, 10, 10)[3]);
        assert!(image.bgra.chunks_exact(4).all(|pixel| pixel[2] <= pixel[3]));
    }
    #[test]
    fn extreme_filter_or_working_set_is_omitted_before_allocation() {
        assert!(raster(&item(true, 2048.0, 0.0), 20, 20).is_none());
        assert!(raster(&item(true, 10.0, 0.0), 8000, 8000).is_none());
    }
}
