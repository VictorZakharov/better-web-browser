pub(in crate::windows_app) mod affine;
mod backbuffer;
mod background;
mod clip;
mod controls;
mod crash;
mod groups;
mod images;
mod masking;
mod opacity;
pub(in crate::windows_app) mod profile;
mod solid;
mod visibility;

use super::paint_primitives::{
    draw_text_in_rect, fill_color_rect, fill_color_shape, intersects, paint_alpha_bitmap,
    paint_alpha_bitmap_from_dc, paint_alpha_bitmap_size, paint_background_image,
    paint_border_colors, screen_rect,
};
use super::platform::*;
use super::{BrowserState, Surface, rgb, wide_without_null, window_text};
use better_web_browser::engine::{ControlKind, DisplayItem};
use clip::ClipStack;
use crash::paint_crash_page;
use groups::PaintGroups;
use std::ptr::null_mut;
use std::time::Instant;

impl BrowserState {
    pub(super) unsafe fn paint_surface(&mut self, dc: Hdc, client: &Rect, dirty: &Rect) {
        let toolbar_height = self.toolbar_height();
        let scale = self.page_scale();
        let dpi = self.dpi;
        let benchmark_mode = self.benchmark.is_some();
        let content_brush = self.content_brush;
        let fonts = self.fonts.as_ref();
        let content = Rect {
            left: 0,
            top: toolbar_height,
            right: client.right,
            bottom: (client.bottom - self.status_height()).max(toolbar_height),
        };
        let tab = self.tabs.active_mut();
        if tab.crashed {
            paint_crash_page(dc, &content, content_brush, fonts, &tab.status_text, scale);
            self.paint_chrome(dc, client);
            return;
        }
        match tab.surface {
            Surface::Page if tab.page_layout.background.alpha != 0 => {
                fill_color_rect(dc, &content, tab.page_layout.background.to_colorref())
            }
            _ => {
                FillRect(dc, &content, content_brush);
            }
        }
        SetBkMode(dc, TRANSPARENT);
        let saved_dc = SaveDC(dc);
        IntersectClipRect(dc, content.left, content.top, content.right, content.bottom);
        let glyph_source_dc = CreateCompatibleDC(dc);
        match tab.surface {
            Surface::Page => {
                let dirty_content_top = (dirty.top - toolbar_height).max(0);
                let dirty_content_bottom = (dirty.bottom - toolbar_height).max(0);
                let visible_top = (tab.scroll_y + dirty_content_top) as f32 / scale;
                let visible_bottom = (tab.scroll_y + dirty_content_bottom) as f32 / scale;
                let mut paint_groups = PaintGroups::default();
                let mut clip_stack = ClipStack::default();
                for range in tab.paint_index.visible_ranges(visible_top, visible_bottom) {
                    for item in &tab.page_layout.items[range] {
                        let _timing = profile::ItemTiming::start(item);
                        if paint_groups.handle(
                            item,
                            dc,
                            &tab.presented_images,
                            tab.scroll_y,
                            toolbar_height,
                            scale,
                            &content,
                            dirty,
                        ) {
                            continue;
                        }
                        let item_dc = paint_groups.dc(dc);
                        match item {
                            DisplayItem::NodeBoundary { .. }
                            | DisplayItem::PaintBoundary { .. }
                            | DisplayItem::CssGradient { .. }
                            | DisplayItem::CssBoxShadow { .. }
                            | DisplayItem::BeginCssMask { .. }
                            | DisplayItem::EmbeddedFrame { .. } => continue,
                            DisplayItem::BeginOpacity { .. }
                            | DisplayItem::BeginTransform { .. }
                            | DisplayItem::EndTransform { .. }
                            | DisplayItem::EndOpacity { .. }
                            | DisplayItem::BeginMask { .. }
                            | DisplayItem::EndMask { .. } => unreachable!("handled paint group"),
                            DisplayItem::BeginClip { .. } | DisplayItem::EndClip { .. } => {
                                clip_stack.handle(
                                    item,
                                    item_dc,
                                    tab.scroll_y,
                                    toolbar_height,
                                    scale,
                                );
                            }
                            DisplayItem::SolidRect {
                                rect,
                                color,
                                radius,
                            } => {
                                solid::paint(
                                    item_dc,
                                    *rect,
                                    *color,
                                    *radius,
                                    tab.scroll_y,
                                    toolbar_height,
                                    scale,
                                    &content,
                                    dirty,
                                );
                            }
                            DisplayItem::BorderRect {
                                rect,
                                widths,
                                colors,
                                radius,
                            } => {
                                let rectangle =
                                    screen_rect(*rect, tab.scroll_y, toolbar_height, scale);
                                if intersects(&affine::device_bounds(item_dc, rectangle), &content)
                                {
                                    paint_border_colors(
                                        item_dc,
                                        &rectangle,
                                        widths.map(|width| width * scale),
                                        *colors,
                                        *radius * scale,
                                    );
                                }
                            }
                            DisplayItem::Text {
                                rect,
                                text,
                                font,
                                color,
                                raster_run_id,
                                glyphs,
                                ..
                            } => {
                                let screen_y =
                                    toolbar_height + (rect.y * scale).round() as i32 - tab.scroll_y;
                                if !intersects(
                                    &affine::device_bounds(
                                        item_dc,
                                        screen_rect(*rect, tab.scroll_y, toolbar_height, scale),
                                    ),
                                    &content,
                                ) {
                                    continue;
                                }
                                // The built-in `browser.local` document is trusted browser UI and
                                // still uses the native UI fallback. Remote documents always have
                                // an active renderer identity and can only paint validated glyphs.
                                if glyphs.is_empty() && tab.navigation.active_document().is_none() {
                                    let font_handle = tab.dynamic_fonts.get_or_create(font, dpi);
                                    SelectObject(item_dc, font_handle);
                                    SetTextColor(item_dc, color.to_colorref());
                                    let text = wide_without_null(text);
                                    TextOutW(
                                        item_dc,
                                        (rect.x * scale).round() as i32,
                                        screen_y,
                                        text.as_ptr(),
                                        text.len() as i32,
                                    );
                                    continue;
                                }
                                let tint = [color.red, color.green, color.blue, color.alpha];
                                if let Some(run) = tab.glyph_bitmaps.get_or_create_run(
                                    *raster_run_id,
                                    glyphs,
                                    &tab.presented_glyphs,
                                    tint,
                                    scale,
                                    item_dc,
                                ) {
                                    let destination = run.destination_rect(
                                        *rect,
                                        tab.scroll_y,
                                        toolbar_height,
                                        scale,
                                    );
                                    if intersects(
                                        &affine::device_bounds(item_dc, destination),
                                        &content,
                                    ) {
                                        if glyph_source_dc.is_null() {
                                            paint_alpha_bitmap_size(
                                                item_dc,
                                                run.bitmap,
                                                run.source_width,
                                                run.source_height,
                                                &destination,
                                            );
                                        } else {
                                            paint_alpha_bitmap_from_dc(
                                                item_dc,
                                                glyph_source_dc,
                                                run.bitmap,
                                                run.source_width,
                                                run.source_height,
                                                &destination,
                                            );
                                        }
                                    }
                                    continue;
                                }
                                for glyph in glyphs.iter() {
                                    let Some(resource) = tab.presented_glyphs.get(&glyph.raster_id)
                                    else {
                                        continue;
                                    };
                                    if resource.color != glyph.color {
                                        continue;
                                    }
                                    let glyph_rect = better_web_browser::engine::RectF {
                                        x: rect.x + glyph.x,
                                        y: rect.y + glyph.y,
                                        width: glyph.width,
                                        height: glyph.height,
                                    };
                                    let destination = screen_rect(
                                        glyph_rect,
                                        tab.scroll_y,
                                        toolbar_height,
                                        scale,
                                    );
                                    if !intersects(
                                        &affine::device_bounds(item_dc, destination),
                                        &content,
                                    ) {
                                        continue;
                                    }
                                    let tint = (!resource.color).then_some(tint);
                                    let bitmap = tab.glyph_bitmaps.get_or_create(
                                        resource.id,
                                        &resource.image,
                                        tint,
                                        item_dc,
                                    );
                                    if !bitmap.is_null() {
                                        if glyph_source_dc.is_null() {
                                            paint_alpha_bitmap(
                                                item_dc,
                                                bitmap,
                                                &resource.image,
                                                &destination,
                                            );
                                        } else {
                                            paint_alpha_bitmap_from_dc(
                                                item_dc,
                                                glyph_source_dc,
                                                bitmap,
                                                resource.image.width,
                                                resource.image.height,
                                                &destination,
                                            );
                                        }
                                    }
                                }
                            }
                            DisplayItem::Image { .. } => images::paint(
                                item,
                                item_dc,
                                glyph_source_dc,
                                &tab.presented_images,
                                &mut tab.image_bitmaps,
                                tab.scroll_y,
                                toolbar_height,
                                scale,
                                &affine::local_content(item_dc, content),
                                fonts,
                            ),
                            DisplayItem::BackgroundImage { .. } => background::paint(
                                item,
                                item_dc,
                                glyph_source_dc,
                                &tab.presented_images,
                                &mut tab.image_bitmaps,
                                tab.scroll_y,
                                toolbar_height,
                                scale,
                                &affine::local_content(item_dc, content),
                            ),
                            DisplayItem::Control(spec) => controls::paint(
                                spec,
                                &mut tab.dynamic_fonts,
                                &tab.page_controls,
                                tab.scroll_y,
                                item_dc,
                                dpi,
                                toolbar_height,
                                scale,
                                &affine::local_content(item_dc, content),
                                benchmark_mode,
                            ),
                        }
                    }
                }
            }
            Surface::Reader => {
                if let Some(fonts) = fonts {
                    for item in &tab.draw_items {
                        let screen_y = toolbar_height + item.y - tab.scroll_y;
                        if screen_y + item.height < content.top || screen_y > content.bottom {
                            continue;
                        }
                        SelectObject(dc, fonts.get(item.font));
                        SetTextColor(dc, item.color);
                        let text = wide_without_null(&item.text);
                        TextOutW(dc, item.x, screen_y, text.as_ptr(), text.len() as i32);
                    }
                }
            }
        }
        if !glyph_source_dc.is_null() {
            DeleteDC(glyph_source_dc);
        }
        if saved_dc != 0 {
            RestoreDC(dc, saved_dc);
        }
        self.paint_chrome(dc, client);
    }
}
