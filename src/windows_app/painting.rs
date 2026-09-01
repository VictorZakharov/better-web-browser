mod clip;
mod crash;
mod opacity;

use super::paint_primitives::{
    draw_text_in_rect, fill_color_rect, fill_color_shape, intersects, paint_alpha_bitmap,
    paint_alpha_bitmap_from_dc, paint_alpha_bitmap_size, paint_alpha_image, paint_background_image,
    paint_border, screen_rect,
};
use super::platform::*;
use super::{BrowserState, Surface, rgb, wide_without_null, window_text};
use better_web_browser::engine::{ControlKind, DisplayItem};
use clip::ClipStack;
use crash::paint_crash_page;
use opacity::{OpacityLayer, OpacityLayerStart};
use std::ptr::null_mut;
use std::time::Instant;

impl BrowserState {
    pub(super) unsafe fn paint(&mut self) {
        let paint_started = Instant::now();
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(self.window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let dirty = if paint.paint.width() > 0 && paint.paint.height() > 0 {
            paint.paint
        } else {
            client
        };
        let content = Rect {
            left: 0,
            top: self.toolbar_height(),
            right: client.right,
            bottom: (client.bottom - self.status_height()).max(self.toolbar_height()),
        };
        let painted_content = intersects(&dirty, &content);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, dirty.width().max(1), dirty.height().max(1))
        };

        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            // Map client coordinates into a backbuffer sized to the invalidated region.
            SetViewportOrgEx(memory_dc, -dirty.left, -dirty.top, null_mut());
            IntersectClipRect(memory_dc, dirty.left, dirty.top, dirty.right, dirty.bottom);
            self.paint_surface(memory_dc, &client, &dirty);
            BitBlt(
                window_dc,
                dirty.left,
                dirty.top,
                dirty.width(),
                dirty.height(),
                memory_dc,
                dirty.left,
                dirty.top,
                SRCCOPY,
            );
            if !previous.is_null() {
                SelectObject(memory_dc, previous);
            }
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_surface(window_dc, &client, &dirty);
        }
        EndPaint(self.window, &paint);
        let paint_time = paint_started.elapsed();
        if painted_content {
            self.record_visible_paint(paint_time);
        }
        self.record_benchmark_paint(paint_time);
    }

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
            Surface::Page => {
                fill_color_rect(dc, &content, tab.page_layout.background.to_colorref())
            }
            Surface::Reader => {
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
                let mut opacity_layers = Vec::new();
                let mut clip_stack = ClipStack::default();
                let mut skipped_opacity_depth = 0_usize;
                for range in tab.paint_index.visible_ranges(visible_top, visible_bottom) {
                    for item in &tab.page_layout.items[range] {
                        if skipped_opacity_depth > 0 {
                            match item {
                                DisplayItem::BeginOpacity { .. } => skipped_opacity_depth += 1,
                                DisplayItem::EndOpacity { .. } => skipped_opacity_depth -= 1,
                                _ => {}
                            }
                            continue;
                        }
                        let item_dc = opacity_layers
                            .iter()
                            .rev()
                            .find_map(OpacityLayer::dc)
                            .unwrap_or(dc);
                        match item {
                            DisplayItem::BeginOpacity { bounds, opacity } => {
                                if *opacity <= 0.0 {
                                    skipped_opacity_depth = 1;
                                    continue;
                                }
                                match OpacityLayer::begin(
                                    item_dc,
                                    *bounds,
                                    *opacity,
                                    tab.scroll_y,
                                    toolbar_height,
                                    scale,
                                    &content,
                                    dirty,
                                ) {
                                    OpacityLayerStart::Hidden => skipped_opacity_depth = 1,
                                    OpacityLayerStart::Layer(layer) => opacity_layers.push(layer),
                                }
                            }
                            DisplayItem::EndOpacity { .. } => {
                                if let Some(layer) = opacity_layers.pop() {
                                    layer.finish();
                                }
                            }
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
                                let rectangle =
                                    screen_rect(*rect, tab.scroll_y, toolbar_height, scale);
                                if intersects(&rectangle, &content) {
                                    fill_color_shape(
                                        item_dc,
                                        &rectangle,
                                        color.to_colorref(),
                                        *radius * scale,
                                    );
                                }
                            }
                            DisplayItem::BorderRect {
                                rect,
                                widths,
                                color,
                                radius,
                            } => {
                                let rectangle =
                                    screen_rect(*rect, tab.scroll_y, toolbar_height, scale);
                                if intersects(&rectangle, &content) {
                                    paint_border(
                                        item_dc,
                                        &rectangle,
                                        widths.map(|width| width * scale),
                                        color.to_colorref(),
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
                                if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                    || screen_y > content.bottom
                                {
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
                                    if intersects(&destination, &content) {
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
                                    if !intersects(&destination, &content) {
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
                            DisplayItem::Image {
                                rect,
                                url,
                                alt,
                                tint,
                            } => {
                                let screen_y =
                                    toolbar_height + (rect.y * scale).round() as i32 - tab.scroll_y;
                                if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                    || screen_y > content.bottom
                                {
                                    continue;
                                }
                                if let Some(image) = tab.presented_images.get(url) {
                                    let bitmap = if let Some(color) = tint {
                                        tab.image_bitmaps.get_or_create_tinted(
                                            url,
                                            image,
                                            [color.red, color.green, color.blue, color.alpha],
                                            item_dc,
                                        )
                                    } else {
                                        tab.image_bitmaps.get_or_create(url, image, item_dc)
                                    };
                                    if !bitmap.is_null() {
                                        paint_alpha_image(
                                            item_dc, bitmap, image, *rect, screen_y, scale,
                                        );
                                    }
                                } else if !alt.is_empty()
                                    && let Some(fonts) = fonts
                                {
                                    SelectObject(item_dc, fonts.body);
                                    SetTextColor(item_dc, rgb(70, 70, 70));
                                    let alt = wide_without_null(alt);
                                    TextOutW(
                                        item_dc,
                                        (rect.x * scale).round() as i32,
                                        screen_y,
                                        alt.as_ptr(),
                                        alt.len() as i32,
                                    );
                                }
                            }
                            DisplayItem::BackgroundImage {
                                clip_rect,
                                tile_rect,
                                url,
                                repeat_x,
                                repeat_y,
                            } => {
                                let clip =
                                    screen_rect(*clip_rect, tab.scroll_y, toolbar_height, scale);
                                if !intersects(&clip, &content)
                                    || tile_rect.width <= 0.0
                                    || tile_rect.height <= 0.0
                                {
                                    continue;
                                }
                                if let Some(image) = tab.presented_images.get(url) {
                                    let bitmap =
                                        tab.image_bitmaps.get_or_create(url, image, item_dc);
                                    if !bitmap.is_null() {
                                        paint_background_image(
                                            item_dc,
                                            bitmap,
                                            image,
                                            *clip_rect,
                                            *tile_rect,
                                            *repeat_x,
                                            *repeat_y,
                                            tab.scroll_y,
                                            toolbar_height,
                                            scale,
                                        );
                                    }
                                }
                            }
                            DisplayItem::Control(spec) => {
                                if benchmark_mode {
                                    let mut rectangle =
                                        screen_rect(spec.rect, tab.scroll_y, toolbar_height, scale);
                                    if !intersects(&rectangle, &content) {
                                        continue;
                                    }
                                    let is_button = matches!(
                                        spec.kind,
                                        ControlKind::Submit
                                            | ControlKind::Button
                                            | ControlKind::Reset
                                    );
                                    if !is_button {
                                        let [border_top, border_right, border_bottom, border_left] =
                                            spec.border_width
                                                .map(|width| (width * scale).ceil() as i32);
                                        let [
                                            padding_top,
                                            padding_right,
                                            padding_bottom,
                                            padding_left,
                                        ] = spec.padding.map(|width| (width * scale).ceil() as i32);
                                        rectangle.left += border_left + padding_left;
                                        rectangle.top += border_top + padding_top;
                                        rectangle.right -= border_right + padding_right;
                                        rectangle.bottom -= border_bottom + padding_bottom;
                                    }
                                    let font = tab.dynamic_fonts.get_or_create(&spec.font, dpi);
                                    SelectObject(item_dc, font);
                                    SetTextColor(
                                        item_dc,
                                        if spec.text_color.alpha == 0 {
                                            CHROME_THEME.text
                                        } else {
                                            spec.text_color.to_colorref()
                                        },
                                    );
                                    let value = tab
                                        .page_controls
                                        .iter()
                                        .find(|control| control.spec.node_id == spec.node_id)
                                        .map(|control| window_text(control.window))
                                        .unwrap_or_else(|| spec.value.clone());
                                    let text = if spec.kind == ControlKind::Password {
                                        "•".repeat(value.chars().count())
                                    } else if value.is_empty() {
                                        if spec.kind == ControlKind::Select || is_button {
                                            spec.label.clone()
                                        } else {
                                            spec.placeholder.clone()
                                        }
                                    } else {
                                        value
                                    };
                                    draw_text_in_rect(
                                        item_dc,
                                        &text,
                                        &mut rectangle,
                                        DT_VCENTER
                                            | DT_SINGLELINE
                                            | DT_END_ELLIPSIS
                                            | DT_NOPREFIX
                                            | if is_button { DT_CENTER } else { 0 },
                                    );
                                }
                            }
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
