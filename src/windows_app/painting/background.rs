//! Background tile painting owns bitmap admission; the main painter owns ordering.
use super::super::rendering_resources::ImageBitmaps;
use super::*;
use better_web_browser::engine::DecodedImage;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn paint(
    item: &DisplayItem,
    dc: Hdc,
    source_dc: Hdc,
    images: &HashMap<String, DecodedImage>,
    bitmaps: &mut ImageBitmaps,
    scroll_y: i32,
    toolbar_height: i32,
    scale: f32,
    content: &Rect,
) {
    let DisplayItem::BackgroundImage {
        clip_rect,
        tile_rect,
        url,
        repeat_x,
        repeat_y,
    } = item
    else {
        return;
    };
    let clip = screen_rect(*clip_rect, scroll_y, toolbar_height, scale);
    if !intersects(&clip, content)
        || tile_rect.width <= 0.0
        || tile_rect.height <= 0.0
        || !super::visibility::drawable(dc, clip)
    {
        return;
    }
    if let Some(image) = images.get(url) {
        let bitmap = bitmaps.get_or_create(url, image, dc);
        if !bitmap.is_null() {
            paint_background_image(
                dc,
                source_dc,
                bitmap,
                image,
                *clip_rect,
                *tile_rect,
                *repeat_x,
                *repeat_y,
                scroll_y,
                toolbar_height,
                scale,
                bitmaps.is_opaque(url),
            );
        }
    }
}
