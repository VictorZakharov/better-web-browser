//! Cache the same clipped, proportional border sectors as the native painter.
use super::*;
use better_web_browser::engine::css::Color;

pub(super) fn paint(
    compositor: &mut Compositor,
    bounds: D2D_RECT_F,
    colors: [Color; 4],
    widths: [f32; 4],
    radius: f32,
) -> Result<(), String> {
    let width = (bounds.right - bounds.left).max(0.0) as u32;
    let height = (bounds.bottom - bounds.top).max(0.0) as u32;
    let widths = widths.map(|value| value.max(0.0).round());
    if width == 0 || height == 0 || !widths.iter().any(|value| *value > 0.0) {
        return Ok(());
    }
    // The retained GDI border painter uses RGB FillRgn for every nontransparent
    // side. Keep that compatibility behavior, including transparent ownership.
    let colors = colors.map(|color| Color {
        alpha: if color.alpha == 0 { 0 } else { 255 },
        ..color
    });
    if colors.iter().all(|color| *color == colors[0]) {
        return shapes::paint(compositor, bounds, colors[0], radius, Some(widths));
    }
    if width > 7680
        || height > 4320
        || u64::from(width + 1) * u64::from(height + 1) * 4 > 64 * 1024 * 1024
    {
        return Err("native GPU border exceeds the bitmap budget".into());
    }
    let key = format!(
        "native-border:{width}:{height}:{}:{widths:?}:{colors:?}",
        radius.to_bits()
    );
    let rect = Rect {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    let ring = std::cell::OnceCell::new();
    let mut covered = coverage::large_shape(width, height);
    if covered {
        for (side, color) in colors.iter().enumerate() {
            if color.alpha == 0 || widths[side] == 0.0 {
                continue;
            }
            let geometry_key = format!(
                "native-border-coverage:{width}:{height}:{}:{widths:?}:{side}",
                radius.to_bits()
            );
            if !compositor.coverage.paint(
                &compositor.device,
                &geometry_key,
                bounds,
                *color,
                || {
                    let ring = ring
                        .get_or_init(|| {
                            let region = coverage::Region(unsafe {
                                super::super::paint_primitives::border_ring(&rect, widths, radius)
                            });
                            if region.0.is_null() {
                                Err("native GPU border ring is unavailable".to_string())
                            } else {
                                Ok(region)
                            }
                        })
                        .as_ref()
                        .map_err(Clone::clone)?;
                    let region = unsafe {
                        super::super::paint_primitives::border_side_region(
                            &rect, widths, side, ring.0,
                        )
                    };
                    if region.is_null() {
                        return Err("native GPU border side is unavailable".into());
                    }
                    Ok(coverage::Region(region))
                },
            )? {
                covered = false;
                break;
            }
        }
    }
    if covered {
        return Ok(());
    }
    let bitmap = compositor.images.generated(&compositor.device, &key, || {
        raster(width, height, widths, colors, radius)
    })?;
    primitives::draw(
        &compositor.device,
        &bitmap,
        D2D_RECT_F {
            right: bounds.right + 1.0,
            bottom: bounds.bottom + 1.0,
            ..bounds
        },
    );
    Ok(())
}

pub(super) fn raster(
    width: u32,
    height: u32,
    widths: [f32; 4],
    colors: [Color; 4],
    radius: f32,
) -> Result<DecodedImage, String> {
    let rect = Rect {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    let ring = unsafe { super::super::paint_primitives::border_ring(&rect, widths, radius) };
    if ring.is_null() {
        return Err("native GPU border ring is unavailable".into());
    }
    let result = (|| {
        let mut bgra = vec![0; (width + 1) as usize * (height + 1) as usize * 4];
        for (side, color) in colors.iter().enumerate() {
            if color.alpha == 0 || widths[side] == 0.0 {
                continue;
            }
            let region = unsafe {
                super::super::paint_primitives::border_side_region(&rect, widths, side, ring)
            };
            if region.is_null() {
                return Err("native GPU border side is unavailable".into());
            }
            let pixels = shapes::fill_pixels(region, width + 1, height + 1, *color, &mut bgra);
            unsafe {
                DeleteObject(region);
            }
            pixels?;
        }
        Ok(DecodedImage {
            width: width + 1,
            height: height + 1,
            bgra: bgra.into(),
        })
    })();
    unsafe {
        DeleteObject(ring);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::paint_primitives::bitmap_info;

    #[test]
    fn cached_colored_border_matches_every_native_pixel() {
        unsafe {
            let image = DecodedImage {
                width: 65,
                height: 65,
                bgra: vec![255; 65 * 65 * 4].into(),
            };
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            assert!(!dc.is_null());
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                dc,
                &bitmap_info(&image),
                0,
                &mut pixels,
                std::ptr::null_mut(),
                0,
            );
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap);
            let rect = Rect {
                left: 0,
                top: 0,
                right: 64,
                bottom: 64,
            };
            let colors = [
                Color::rgb(255, 0, 0),
                Color::rgb(0, 128, 0),
                Color::rgb(0, 0, 255),
                Color::TRANSPARENT,
            ];
            for radius in [0.0, 20.0] {
                for widths in [[8.0; 4], [3.0, 12.0, 5.0, 0.0], [2.0, 0.0, 0.0, 0.0]] {
                    std::ptr::write_bytes(pixels, 255, 65 * 65 * 4);
                    super::super::super::paint_primitives::paint_border_colors(
                        dc, &rect, widths, colors, radius,
                    );
                    let native = std::slice::from_raw_parts(pixels.cast::<u8>(), 65 * 65 * 4);
                    let cached = raster(64, 64, widths, colors, radius).unwrap();
                    for (actual, expected) in
                        cached.bgra.chunks_exact(4).zip(native.chunks_exact(4))
                    {
                        let rgb = if actual[3] == 0 {
                            [255; 3]
                        } else {
                            [actual[0], actual[1], actual[2]]
                        };
                        assert_eq!(rgb, expected[..3], "radius {radius}, widths {widths:?}");
                    }
                }
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
        }
    }
}
