use super::platform::*;
use super::wide_without_null;
use better_web_browser::engine::{DecodedImage, RectF};
use std::mem::size_of;

pub(super) fn screen_rect(rect: RectF, scroll_y: i32, content_top: i32, scale: f32) -> Rect {
    Rect {
        left: (rect.x * scale).round() as i32,
        top: content_top + (rect.y * scale).round() as i32 - scroll_y,
        right: (rect.right() * scale).ceil() as i32,
        bottom: content_top + (rect.bottom() * scale).ceil() as i32 - scroll_y,
    }
}

pub(super) fn bitmap_info(image: &DecodedImage) -> BitmapInfo {
    BitmapInfo {
        header: BitmapInfoHeader {
            size: size_of::<BitmapInfoHeader>() as u32,
            width: image.width as i32,
            height: -(image.height as i32),
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: image.bgra.len() as u32,
            x_pixels_per_meter: 0,
            y_pixels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        },
        colors: [0],
    }
}

pub(super) unsafe fn paint_alpha_image(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    rect: RectF,
    screen_y: i32,
    scale: f32,
) {
    let destination_rect = Rect {
        left: (rect.x * scale).round() as i32,
        top: screen_y,
        right: ((rect.x + rect.width) * scale).round() as i32,
        bottom: screen_y + (rect.height * scale).round().max(1.0) as i32,
    };
    paint_alpha_bitmap(destination, bitmap, image, &destination_rect);
}

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn paint_background_image(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    clip_rect: RectF,
    tile_rect: RectF,
    repeat_x: bool,
    repeat_y: bool,
    scroll_y: i32,
    content_top: i32,
    scale: f32,
) {
    if tile_rect.width <= 0.0 || tile_rect.height <= 0.0 {
        return;
    }
    let clip = screen_rect(clip_rect, scroll_y, content_top, scale);
    let saved = SaveDC(destination);
    IntersectClipRect(destination, clip.left, clip.top, clip.right, clip.bottom);

    let start_x = if repeat_x {
        tile_rect.x + ((clip_rect.x - tile_rect.x) / tile_rect.width).floor() * tile_rect.width
    } else {
        tile_rect.x
    };
    let start_y = if repeat_y {
        tile_rect.y + ((clip_rect.y - tile_rect.y) / tile_rect.height).floor() * tile_rect.height
    } else {
        tile_rect.y
    };
    let mut painted = 0_usize;
    let mut y = start_y;
    loop {
        let mut x = start_x;
        loop {
            let tile = RectF {
                x,
                y,
                width: tile_rect.width,
                height: tile_rect.height,
            };
            let destination_rect = screen_rect(tile, scroll_y, content_top, scale);
            paint_alpha_bitmap(destination, bitmap, image, &destination_rect);
            painted += 1;
            if !repeat_x || painted >= 4_096 {
                break;
            }
            x += tile_rect.width;
            if x >= clip_rect.right() {
                break;
            }
        }
        if !repeat_y || painted >= 4_096 {
            break;
        }
        y += tile_rect.height;
        if y >= clip_rect.bottom() {
            break;
        }
    }

    if saved != 0 {
        RestoreDC(destination, saved);
    }
}

pub(super) unsafe fn paint_alpha_bitmap(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    destination_rect: &Rect,
) {
    let source = CreateCompatibleDC(destination);
    if source.is_null() {
        return;
    }
    let previous = SelectObject(source, bitmap);
    AlphaBlend(
        destination,
        destination_rect.left,
        destination_rect.top,
        destination_rect.width().max(1),
        destination_rect.height().max(1),
        source,
        0,
        0,
        image.width as i32,
        image.height as i32,
        BlendFunction {
            operation: 0,
            flags: 0,
            source_constant_alpha: 255,
            alpha_format: 1,
        },
    );
    if !previous.is_null() {
        SelectObject(source, previous);
    }
    DeleteDC(source);
}

pub(super) fn intersects(left: &Rect, right: &Rect) -> bool {
    left.left < right.right
        && left.right > right.left
        && left.top < right.bottom
        && left.bottom > right.top
}

pub(super) unsafe fn fill_color_rect(dc: Hdc, rectangle: &Rect, color: u32) {
    if rectangle.right <= rectangle.left || rectangle.bottom <= rectangle.top {
        return;
    }
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        FillRect(dc, rectangle, brush);
        DeleteObject(brush);
    }
}

pub(super) unsafe fn fill_color_shape(dc: Hdc, rectangle: &Rect, color: u32, radius: f32) {
    if radius <= 0.0 {
        fill_color_rect(dc, rectangle, color);
        return;
    }
    let brush = CreateSolidBrush(color);
    if brush.is_null() {
        return;
    }
    let diameter = (radius * 2.0).round().max(1.0) as i32;
    let region = CreateRoundRectRgn(
        rectangle.left,
        rectangle.top,
        rectangle.right + 1,
        rectangle.bottom + 1,
        diameter,
        diameter,
    );
    if !region.is_null() {
        FillRgn(dc, region, brush);
        DeleteObject(region);
    }
    DeleteObject(brush);
}

pub(super) unsafe fn paint_rounded_panel(
    dc: Hdc,
    rectangle: &Rect,
    fill: u32,
    border: u32,
    radius: f32,
    border_width: i32,
) {
    if rectangle.width() <= 0 || rectangle.height() <= 0 {
        return;
    }
    let border_width = border_width.max(0);
    if border_width == 0 {
        fill_color_shape(dc, rectangle, fill, radius);
        return;
    }
    fill_color_shape(dc, rectangle, border, radius);
    let inner = rectangle.inset(border_width, border_width);
    if inner.width() > 0 && inner.height() > 0 {
        fill_color_shape(dc, &inner, fill, (radius - border_width as f32).max(0.0));
    }
}

pub(super) unsafe fn draw_text_in_rect(
    dc: Hdc,
    text: &str,
    rectangle: &mut Rect,
    format: u32,
) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let text = wide_without_null(text);
    DrawTextW(dc, text.as_ptr(), text.len() as i32, rectangle, format)
}

pub(super) unsafe fn paint_text(
    dc: Hdc,
    font: Hfont,
    color: u32,
    text: &str,
    mut rectangle: Rect,
    format: u32,
) {
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT);
    draw_text_in_rect(dc, text, &mut rectangle, format);
}

pub(super) unsafe fn paint_border(
    dc: Hdc,
    rectangle: &Rect,
    widths: [f32; 4],
    color: u32,
    radius: f32,
) {
    let [top, right, bottom, left] = widths.map(|width| width.ceil().max(0.0) as i32);
    if radius > 0.0 {
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            return;
        }
        let diameter = (radius * 2.0).round().max(1.0) as i32;
        let outer = CreateRoundRectRgn(
            rectangle.left,
            rectangle.top,
            rectangle.right + 1,
            rectangle.bottom + 1,
            diameter,
            diameter,
        );
        let inner_rect = Rect {
            left: rectangle.left + left,
            top: rectangle.top + top,
            right: rectangle.right - right,
            bottom: rectangle.bottom - bottom,
        };
        if !outer.is_null() {
            if inner_rect.width() > 0 && inner_rect.height() > 0 {
                let border_width = top.max(right).max(bottom).max(left) as f32;
                let inner_radius = (radius - border_width).max(0.0);
                let inner_diameter = (inner_radius * 2.0).round().max(1.0) as i32;
                let inner = CreateRoundRectRgn(
                    inner_rect.left,
                    inner_rect.top,
                    inner_rect.right + 1,
                    inner_rect.bottom + 1,
                    inner_diameter,
                    inner_diameter,
                );
                if !inner.is_null() {
                    CombineRgn(outer, outer, inner, RGN_DIFF);
                    DeleteObject(inner);
                }
            }
            FillRgn(dc, outer, brush);
            DeleteObject(outer);
        }
        DeleteObject(brush);
        return;
    }
    if top > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: rectangle.top,
                right: rectangle.right,
                bottom: (rectangle.top + top).min(rectangle.bottom),
            },
            color,
        );
    }
    if right > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: (rectangle.right - right).max(rectangle.left),
                top: rectangle.top,
                right: rectangle.right,
                bottom: rectangle.bottom,
            },
            color,
        );
    }
    if bottom > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: (rectangle.bottom - bottom).max(rectangle.top),
                right: rectangle.right,
                bottom: rectangle.bottom,
            },
            color,
        );
    }
    if left > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: rectangle.top,
                right: (rectangle.left + left).min(rectangle.right),
                bottom: rectangle.bottom,
            },
            color,
        );
    }
}
