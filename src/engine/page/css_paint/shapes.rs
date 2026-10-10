//! Antialiased rounded backgrounds and translucent borders use the real scene backdrop.
//! Opaque square decorations retain the native fast path; these masks reuse tiny-skia.
use super::*;
use crate::engine::css::Color;

pub(super) fn descriptor(item: &DisplayItem) -> Option<String> {
    match item {
        DisplayItem::SolidRect {
            rect,
            color,
            radius,
        } if *radius > 0.0 && color.alpha > 0 => Some(format!(
            "solid:{:?}:{color:?}:{radius}",
            (rect.width, rect.height)
        )),
        DisplayItem::BorderRect {
            rect,
            widths,
            colors,
            radius,
        } if *radius > 0.0
            || colors
                .iter()
                .any(|color| color.alpha > 0 && color.alpha < 255) =>
        {
            Some(format!(
                "border:{:?}:{widths:?}:{colors:?}:{radius}",
                (rect.width, rect.height)
            ))
        }
        _ => None,
    }
}

pub(super) fn raster(item: &DisplayItem, width: u32, height: u32) -> Option<DecodedImage> {
    let mut pixels = Pixmap::new(width, height)?;
    match item {
        DisplayItem::SolidRect {
            rect,
            color,
            radius,
        } => {
            let transform =
                Transform::from_scale(width as f32 / rect.width, height as f32 / rect.height);
            pixels.fill_path(
                &rounded_rect(rect.width, rect.height, *radius)?,
                &paint(*color),
                skia::FillRule::Winding,
                transform,
                None,
            );
        }
        DisplayItem::BorderRect {
            rect,
            widths,
            colors,
            radius,
        } => {
            if widths
                .iter()
                .any(|width| !width.is_finite() || *width < 0.0)
            {
                return None;
            }
            let transform =
                Transform::from_scale(width as f32 / rect.width, height as f32 / rect.height);
            let r = radius.max(0.0).min(rect.width.min(rect.height) / 2.0);
            let [top, right, bottom, left] = *widths;
            let outer = rounded_rect(rect.width, rect.height, r)?;
            let mut ring = PathBuilder::new();
            ring.push_path(&outer);
            let inner_width = rect.width - left - right;
            let inner_height = rect.height - top - bottom;
            if inner_width > 0.0 && inner_height > 0.0 {
                let corners = [
                    [(r - left).max(0.0), (r - top).max(0.0)],
                    [(r - right).max(0.0), (r - top).max(0.0)],
                    [(r - right).max(0.0), (r - bottom).max(0.0)],
                    [(r - left).max(0.0), (r - bottom).max(0.0)],
                ];
                ring.push_path(&inner_path(left, top, inner_width, inner_height, corners)?);
            }
            let ring = ring.finish()?;
            if colors.iter().all(|color| *color == colors[0]) {
                pixels.fill_path(
                    &ring,
                    &paint(colors[0]),
                    skia::FillRule::EvenOdd,
                    transform,
                    None,
                );
            } else {
                let mut mask = skia::Mask::new(width, height)?;
                mask.fill_path(&ring, skia::FillRule::EvenOdd, true, transform);
                for (side, points) in sectors(rect.width, rect.height, *widths)
                    .into_iter()
                    .enumerate()
                {
                    if widths[side] == 0.0 || colors[side].alpha == 0 {
                        continue;
                    }
                    let mut path = PathBuilder::new();
                    path.move_to(points[0][0], points[0][1]);
                    for point in &points[1..] {
                        path.line_to(point[0], point[1]);
                    }
                    path.close();
                    pixels.fill_path(
                        &path.finish()?,
                        &paint(colors[side]),
                        skia::FillRule::Winding,
                        transform,
                        Some(&mask),
                    );
                }
            }
        }
        _ => return None,
    }
    let mut bgra = pixels.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(color.red, color.green, color.blue, color.alpha);
    paint
}

fn inner_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    mut corners: [[f32; 2]; 4],
) -> Option<skia::Path> {
    let [tl, tr, br, bl] = corners;
    let scale = (width / (tl[0] + tr[0]))
        .min(width / (bl[0] + br[0]))
        .min(height / (tl[1] + bl[1]))
        .min(height / (tr[1] + br[1]))
        .min(1.0);
    for corner in &mut corners {
        for radius in corner {
            *radius *= scale;
        }
    }
    let [tl, tr, br, bl] = corners;
    let (right, bottom, k) = (x + width, y + height, 0.552_284_8);
    let mut path = PathBuilder::new();
    path.move_to(x + tl[0], y);
    path.line_to(right - tr[0], y);
    path.cubic_to(
        right - tr[0] + k * tr[0],
        y,
        right,
        y + tr[1] - k * tr[1],
        right,
        y + tr[1],
    );
    path.line_to(right, bottom - br[1]);
    path.cubic_to(
        right,
        bottom - br[1] + k * br[1],
        right - br[0] + k * br[0],
        bottom,
        right - br[0],
        bottom,
    );
    path.line_to(x + bl[0], bottom);
    path.cubic_to(
        x + bl[0] - k * bl[0],
        bottom,
        x,
        bottom - bl[1] + k * bl[1],
        x,
        bottom - bl[1],
    );
    path.line_to(x, y + tl[1]);
    path.cubic_to(
        x,
        y + tl[1] - k * tl[1],
        x + tl[0] - k * tl[0],
        y,
        x + tl[0],
        y,
    );
    path.close();
    path.finish()
}

fn sectors(width: f32, height: f32, widths: [f32; 4]) -> [[[f32; 2]; 4]; 4] {
    let [top, right, bottom, left] = widths;
    let factor = (width / (left + right)).min(height / (top + bottom));
    let outer = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]];
    let inner = [
        [left * factor, top * factor],
        [width - right * factor, top * factor],
        [width - right * factor, height - bottom * factor],
        [left * factor, height - bottom * factor],
    ];
    std::array::from_fn(|side| {
        let next = (side + 1) % 4;
        [outer[side], outer[next], inner[next], inner[side]]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect() -> RectF {
        RectF {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        }
    }
    fn px(image: &DecodedImage, x: usize, y: usize) -> &[u8] {
        &image.bgra[(y * image.width as usize + x) * 4..][..4]
    }
    #[test]
    fn rounded_background_has_clear_corners_and_partial_edge_coverage() {
        let item = DisplayItem::SolidRect {
            rect: rect(),
            color: Color {
                alpha: 128,
                ..Color::rgb(255, 0, 0)
            },
            radius: 8.0,
        };
        let image = raster(&item, 20, 20).unwrap();
        assert_eq!(px(&image, 0, 0), [0, 0, 0, 0]);
        assert_eq!(px(&image, 10, 10), [0, 0, 128, 128]);
        assert!(
            image
                .bgra
                .chunks_exact(4)
                .any(|px| px[3] > 0 && px[3] < 128)
        );
    }
    #[test]
    fn translucent_border_knocks_out_center_and_keeps_original_alpha() {
        let item = DisplayItem::BorderRect {
            rect: rect(),
            widths: [2.0; 4],
            colors: [Color {
                alpha: 128,
                ..Color::rgb(255, 255, 0)
            }; 4],
            radius: 5.0,
        };
        let image = raster(&item, 40, 40).unwrap();
        assert_eq!(px(&image, 20, 20), [0, 0, 0, 0]);
        assert_eq!(px(&image, 20, 0), [0, 128, 128, 128]);
        assert_eq!(px(&image, 0, 0), [0, 0, 0, 0]);
    }
    #[test]
    fn asymmetric_sides_preserve_transparent_sector_and_width() {
        let item = DisplayItem::BorderRect {
            rect: rect(),
            widths: [2.0, 3.0, 4.0, 1.0],
            colors: [
                Color::rgb(255, 0, 0),
                Color::TRANSPARENT,
                Color::rgb(0, 0, 255),
                Color::rgb(0, 255, 0),
            ],
            radius: 0.0,
        };
        let image = raster(&item, 20, 20).unwrap();
        assert_eq!(px(&image, 10, 0), [0, 0, 255, 255]);
        assert_eq!(px(&image, 19, 10), [0, 0, 0, 0]);
        assert_eq!(px(&image, 10, 19), [255, 0, 0, 255]);
        assert_eq!(px(&image, 0, 10), [0, 255, 0, 255]);
    }
}
