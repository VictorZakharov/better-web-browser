use super::paint_primitives::{
    draw_text_in_rect, fill_color_rect, fill_color_shape, intersects, paint_alpha_image,
    paint_background_image, paint_border, screen_rect,
};
use super::platform::*;
use super::{BrowserState, Surface, rgb, wide_without_null, window_text};
use better_web_browser::engine::{ControlKind, DisplayItem};
use std::ptr::null_mut;

impl BrowserState {
    pub(super) unsafe fn paint(&mut self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(self.window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, width, height)
        };

        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            self.paint_surface(memory_dc, &client);
            BitBlt(window_dc, 0, 0, width, height, memory_dc, 0, 0, SRCCOPY);
            if !previous.is_null() {
                SelectObject(memory_dc, previous);
            }
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_surface(window_dc, &client);
        }
        EndPaint(self.window, &paint);
    }

    pub(super) unsafe fn paint_surface(&mut self, dc: Hdc, client: &Rect) {
        let toolbar_height = self.toolbar_height();
        let scale = self.page_scale();
        let content = Rect {
            left: 0,
            top: toolbar_height,
            right: client.right,
            bottom: (client.bottom - self.status_height()).max(toolbar_height),
        };
        match self.surface {
            Surface::Page => {
                fill_color_rect(dc, &content, self.page_layout.background.to_colorref())
            }
            Surface::Reader => {
                FillRect(dc, &content, self.content_brush);
            }
        }
        SetBkMode(dc, TRANSPARENT);
        let saved_dc = SaveDC(dc);
        IntersectClipRect(dc, content.left, content.top, content.right, content.bottom);
        match self.surface {
            Surface::Page => {
                let visible_top = self.scroll_y as f32 / scale;
                let visible_bottom = (self.scroll_y + content.height()).max(0) as f32 / scale;
                for range in self.paint_index.visible_ranges(visible_top, visible_bottom) {
                    for item in &self.page_layout.items[range] {
                        match item {
                            DisplayItem::SolidRect {
                                rect,
                                color,
                                radius,
                            } => {
                                let rectangle =
                                    screen_rect(*rect, self.scroll_y, toolbar_height, scale);
                                if intersects(&rectangle, &content) {
                                    fill_color_shape(
                                        dc,
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
                                    screen_rect(*rect, self.scroll_y, toolbar_height, scale);
                                if intersects(&rectangle, &content) {
                                    paint_border(
                                        dc,
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
                                ..
                            } => {
                                let screen_y = toolbar_height + (rect.y * scale).round() as i32
                                    - self.scroll_y;
                                if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                    || screen_y > content.bottom
                                {
                                    continue;
                                }
                                let font_handle = self.dynamic_fonts.get_or_create(font, self.dpi);
                                SelectObject(dc, font_handle);
                                SetTextColor(dc, color.to_colorref());
                                let text = wide_without_null(text);
                                TextOutW(
                                    dc,
                                    (rect.x * scale).round() as i32,
                                    screen_y,
                                    text.as_ptr(),
                                    text.len() as i32,
                                );
                            }
                            DisplayItem::Image {
                                rect,
                                url,
                                alt,
                                tint,
                            } => {
                                let screen_y = toolbar_height + (rect.y * scale).round() as i32
                                    - self.scroll_y;
                                if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                    || screen_y > content.bottom
                                {
                                    continue;
                                }
                                if let Some(image) = self.page.images.get(url) {
                                    let bitmap = if let Some(color) = tint {
                                        self.image_bitmaps.get_or_create_tinted(
                                            url,
                                            image,
                                            [color.red, color.green, color.blue, color.alpha],
                                            dc,
                                        )
                                    } else {
                                        self.image_bitmaps.get_or_create(url, image, dc)
                                    };
                                    if !bitmap.is_null() {
                                        paint_alpha_image(
                                            dc, bitmap, image, *rect, screen_y, scale,
                                        );
                                    }
                                } else if !alt.is_empty()
                                    && let Some(fonts) = self.fonts.as_ref()
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
                            }
                            DisplayItem::BackgroundImage {
                                clip_rect,
                                tile_rect,
                                url,
                                repeat_x,
                                repeat_y,
                            } => {
                                let clip =
                                    screen_rect(*clip_rect, self.scroll_y, toolbar_height, scale);
                                if !intersects(&clip, &content)
                                    || tile_rect.width <= 0.0
                                    || tile_rect.height <= 0.0
                                {
                                    continue;
                                }
                                if let Some(image) = self.page.images.get(url) {
                                    let bitmap = self.image_bitmaps.get_or_create(url, image, dc);
                                    if !bitmap.is_null() {
                                        paint_background_image(
                                            dc,
                                            bitmap,
                                            image,
                                            *clip_rect,
                                            *tile_rect,
                                            *repeat_x,
                                            *repeat_y,
                                            self.scroll_y,
                                            toolbar_height,
                                            scale,
                                        );
                                    }
                                }
                            }
                            DisplayItem::Control(spec) => {
                                if self.benchmark.is_some() {
                                    let mut rectangle = screen_rect(
                                        spec.rect,
                                        self.scroll_y,
                                        toolbar_height,
                                        scale,
                                    );
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
                                    let font =
                                        self.dynamic_fonts.get_or_create(&spec.font, self.dpi);
                                    SelectObject(dc, font);
                                    SetTextColor(
                                        dc,
                                        if spec.text_color.alpha == 0 {
                                            CHROME_THEME.text
                                        } else {
                                            spec.text_color.to_colorref()
                                        },
                                    );
                                    let value = self
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
                                        dc,
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
                if let Some(fonts) = self.fonts.as_ref() {
                    for item in &self.draw_items {
                        let screen_y = toolbar_height + item.y - self.scroll_y;
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
        if saved_dc != 0 {
            RestoreDC(dc, saved_dc);
        }
        self.paint_chrome(dc, client);
    }
}
