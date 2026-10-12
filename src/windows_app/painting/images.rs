//! Image admission, sparse/opaque dispatch and item-local clipping.
use super::*;
use crate::windows_app::paint_primitives::{image_rect, paint_alpha_image};
use crate::windows_app::rendering_resources::{Fonts, ImageBitmaps};
use better_web_browser::engine::DecodedImage;
use std::collections::HashMap;
mod reflected;

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
    fonts: Option<&Fonts>,
) {
    let DisplayItem::Image {
        rect,
        clip,
        url,
        alt,
        tint,
    } = item
    else {
        return;
    };
    let visible = clip.unwrap_or(*rect);
    let screen_y = toolbar_height + (rect.y * scale).round() as i32 - scroll_y;
    if !intersects(
        &screen_rect(visible, scroll_y, toolbar_height, scale),
        content,
    ) {
        return;
    }
    let saved = clip.map(|bounds| {
        let saved = SaveDC(dc);
        if saved != 0 {
            let region = screen_rect(bounds, scroll_y, toolbar_height, scale);
            if !super::affine::clip_rect(dc, region) {
                IntersectClipRect(dc, 0, 0, 0, 0);
            }
        }
        saved
    });
    if saved == Some(0) {
        return;
    }
    if !super::visibility::drawable(dc, screen_rect(*rect, scroll_y, toolbar_height, scale)) {
        if let Some(saved) = saved {
            RestoreDC(dc, saved);
        }
        return;
    }
    if let Some(image) = images.get(url) {
        let bitmap = if let Some(color) = tint {
            bitmaps.get_or_create_tinted(
                url,
                image,
                [color.red, color.green, color.blue, color.alpha],
                dc,
            )
        } else {
            bitmaps.get_or_create(url, image, dc)
        };
        if !bitmap.is_null() {
            let opaque = tint.is_none() && bitmaps.is_opaque(url);
            let destination = image_rect(*rect, screen_y, scale);
            let _timing = profile::ImageTiming::native(
                [image.width, image.height],
                dc,
                destination,
                opaque,
                if url.starts_with("breeze-css-paint:") {
                    "css-paint"
                } else if url.starts_with("breeze-internal:canvas:") {
                    "canvas"
                } else {
                    "image"
                },
            );
            if tint.is_none()
                && !opaque
                && reflected::paint(dc, source_dc, bitmap, url, image, destination, bitmaps)
            {
                if let Some(saved) = saved {
                    RestoreDC(dc, saved);
                }
                return;
            }
            let device_sampling = (tint.is_none() && !opaque)
                .then(|| super::affine::ScaledDestination::new(dc, destination))
                .flatten();
            let destination = device_sampling
                .as_ref()
                .map_or(destination, |plan| plan.rect());
            let coverage = tint
                .is_none()
                .then(|| bitmaps.sparse_coverage(url, image, &destination))
                .flatten();
            let scaled = if tint.is_none() && !opaque {
                bitmaps.scaled_image(url, image, bitmap, &destination, dc)
            } else {
                None
            };
            if let Some(scaled) = scaled {
                let sparse = scaled.coverage.as_deref().is_some_and(|coverage| {
                    crate::windows_app::paint_primitives::paint_sparse_bitmap_from_dc(
                        dc,
                        source_dc,
                        scaled.handle,
                        &destination,
                        coverage,
                    )
                });
                if !sparse {
                    crate::windows_app::paint_primitives::paint_alpha_bitmap_from_dc(
                        dc,
                        source_dc,
                        scaled.handle,
                        destination.width() as u32,
                        destination.height() as u32,
                        &destination,
                    );
                }
            } else {
                paint_alpha_image(
                    dc,
                    source_dc,
                    bitmap,
                    image,
                    &destination,
                    opaque,
                    coverage.as_deref(),
                );
            }
        }
    } else if !alt.is_empty()
        && let Some(fonts) = fonts
    {
        SelectObject(dc, fonts.body);
        SetTextColor(dc, rgb(70, 70, 70));
        let alt = wide_without_null(alt);
        TextOutW(
            dc,
            (rect.x * scale).round() as i32,
            screen_y,
            alt.as_ptr(),
            alt.len() as i32,
        );
    }
    if let Some(saved) = saved
        && saved != 0
    {
        RestoreDC(dc, saved);
    }
}
