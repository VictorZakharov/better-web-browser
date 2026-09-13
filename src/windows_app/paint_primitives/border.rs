//! Solid border sides share one clipped ring, with width-proportional corner joins.
use super::*;
use better_web_browser::engine::css::Color;

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreatePolygonRgn(points: *const Point, count: i32, mode: i32) -> Hrgn;
    fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32) -> Hrgn;
}

pub(in crate::windows_app) unsafe fn paint_border(
    dc: Hdc,
    rect: &Rect,
    widths: [f32; 4],
    color: u32,
    radius: f32,
) {
    let color = Color::rgb(color as u8, (color >> 8) as u8, (color >> 16) as u8);
    paint_border_colors(dc, rect, widths, [color; 4], radius);
}

pub(in crate::windows_app) unsafe fn paint_border_colors(
    dc: Hdc,
    rect: &Rect,
    widths: [f32; 4],
    colors: [Color; 4],
    radius: f32,
) {
    if rect.width() <= 0 || rect.height() <= 0 {
        return;
    }
    let widths = widths.map(|v| v.ceil().max(0.0));
    if !widths.iter().any(|v| *v > 0.0) {
        return;
    }
    let ring = border_ring(rect, widths, radius);
    if ring.is_null() {
        return;
    }
    if colors.iter().all(|color| *color == colors[0]) {
        fill_region(dc, ring, colors[0]);
        DeleteObject(ring);
        return;
    }
    for (side, points) in side_polygons(rect, widths).iter().enumerate() {
        if colors[side].alpha == 0 || widths[side] == 0.0 {
            continue;
        }
        let region = CreatePolygonRgn(points.as_ptr(), 4, 1 /* ALTERNATE */);
        if region.is_null() {
            continue;
        }
        CombineRgn(region, region, ring, 1 /* RGN_AND */);
        fill_region(dc, region, colors[side]);
        DeleteObject(region);
    }
    DeleteObject(ring);
}

unsafe fn fill_region(dc: Hdc, region: Hrgn, color: Color) {
    if color.alpha == 0 {
        return;
    }
    let brush = CreateSolidBrush(color.to_colorref());
    if !brush.is_null() {
        FillRgn(dc, region, brush);
        DeleteObject(brush);
    }
}

unsafe fn border_ring(rect: &Rect, widths: [f32; 4], radius: f32) -> Hrgn {
    let [top, right, bottom, left] = widths.map(|v| v as i32);
    let radius = radius
        .max(0.0)
        .min(rect.width().min(rect.height()) as f32 / 2.0);
    let diameter = (2.0 * radius).round() as i32;
    let outer = if radius == 0.0 {
        CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom)
    } else {
        CreateRoundRectRgn(
            rect.left,
            rect.top,
            rect.right + 1,
            rect.bottom + 1,
            diameter,
            diameter,
        )
    };
    if outer.is_null() {
        return outer;
    }
    let inner = Rect {
        left: rect.left + left,
        top: rect.top + top,
        right: rect.right - right,
        bottom: rect.bottom - bottom,
    };
    if inner.width() > 0 && inner.height() > 0 {
        let diameter = (2.0
            * (radius - *widths.iter().max_by(|a, b| a.total_cmp(b)).unwrap()).max(0.0))
        .round() as i32;
        let region = if diameter == 0 {
            CreateRectRgn(inner.left, inner.top, inner.right, inner.bottom)
        } else {
            CreateRoundRectRgn(
                inner.left,
                inner.top,
                inner.right + 1,
                inner.bottom + 1,
                diameter,
                diameter,
            )
        };
        if !region.is_null() {
            CombineRgn(outer, outer, region, RGN_DIFF);
            DeleteObject(region);
        }
    }
    outer
}

fn side_polygons(rect: &Rect, widths: [f32; 4]) -> [[Point; 4]; 4] {
    let [top, right, bottom, left] = widths;
    // Extend each corner's outer-to-inner line until the inset rectangle collapses.
    // The sectors cover the complete box, including rounded corners, without one
    // side painting over its neighbor. A transparent edge still owns its sector.
    let factor = (rect.width() as f32 / (left + right)).min(rect.height() as f32 / (top + bottom));
    let point = |x: f32, y: f32| Point {
        x: x.round() as i32,
        y: y.round() as i32,
    };
    let outer = [
        point(rect.left as f32, rect.top as f32),
        point(rect.right as f32, rect.top as f32),
        point(rect.right as f32, rect.bottom as f32),
        point(rect.left as f32, rect.bottom as f32),
    ];
    let inner = [
        point(
            rect.left as f32 + left * factor,
            rect.top as f32 + top * factor,
        ),
        point(
            rect.right as f32 - right * factor,
            rect.top as f32 + top * factor,
        ),
        point(
            rect.right as f32 - right * factor,
            rect.bottom as f32 - bottom * factor,
        ),
        point(
            rect.left as f32 + left * factor,
            rect.bottom as f32 - bottom * factor,
        ),
    ];
    std::array::from_fn(|side| {
        let next = (side + 1) % 4;
        [outer[side], outer[next], inner[next], inner[side]]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetPixel(dc: Hdc, x: i32, y: i32) -> u32;
    }

    #[test]
    fn offscreen_raster_keeps_each_color_and_transparent_side_in_its_own_sector() {
        unsafe {
            let image = DecodedImage {
                width: 64,
                height: 64,
                bgra: vec![255; 64 * 64 * 4].into(),
            };
            let info = bitmap_info(&image);
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            assert!(!dc.is_null());
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, 0, &mut pixels, std::ptr::null_mut(), 0);
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap);
            let rect = Rect {
                left: 0,
                top: 0,
                right: 64,
                bottom: 64,
            };
            for radius in [0.0, 20.0] {
                fill_color_rect(dc, &rect, Color::WHITE.to_colorref());
                paint_border_colors(
                    dc,
                    &rect,
                    [8.0; 4],
                    [
                        Color::rgb(255, 0, 0),
                        Color::rgb(0, 128, 0),
                        Color::rgb(0, 0, 255),
                        Color::TRANSPARENT,
                    ],
                    radius,
                );
                let actual = [(32, 0), (63, 32), (32, 63), (0, 32), (32, 32)]
                    .map(|(x, y)| GetPixel(dc, x, y));
                assert_eq!(
                    actual,
                    [0x0000ff, 0x008000, 0xff0000, 0xffffff, 0xffffff],
                    "radius {radius}"
                );
                if radius == 0.0 {
                    assert_eq!(GetPixel(dc, 6, 1), 0x0000ff);
                    assert_eq!(GetPixel(dc, 1, 6), 0xffffff);
                }
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
        }
    }
    #[test]
    fn unequal_borders_share_proportional_corner_join_lines() {
        let rect = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let sides = side_polygons(&rect, [10.0, 20.0, 10.0, 20.0]);
        assert_eq!((sides[0][2].x, sides[0][2].y), (50, 25));
        assert_eq!((sides[1][3].x, sides[1][3].y), (50, 25));
        assert_eq!((sides[1][2].x, sides[1][2].y), (50, 75));
    }
    #[test]
    fn absent_neighbor_width_does_not_steal_the_full_width_edge() {
        let rect = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 50,
        };
        let sides = side_polygons(&rect, [2.0, 0.0, 0.0, 0.0]);
        assert_eq!((sides[0][2].x, sides[0][2].y), (100, 50));
        assert_eq!((sides[0][3].x, sides[0][3].y), (0, 50));
    }
}
