//! Bounded GDI backbuffers for CSS group opacity.

use super::super::paint_primitives::screen_rect;
use super::super::platform::*;
use std::ptr::null_mut;

pub(super) enum OpacityLayerStart {
    Hidden,
    Layer(OpacityLayer),
}

pub(super) struct OpacityLayer {
    destination: Hdc,
    source: Hdc,
    bitmap: Hbitmap,
    previous: Hgdiobj,
    rect: Rect,
    opacity: u8,
}

impl OpacityLayer {
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn begin(
        destination: Hdc,
        bounds: better_web_browser::engine::RectF,
        opacity: f32,
        scroll_y: i32,
        toolbar_height: i32,
        scale: f32,
        content: &Rect,
        dirty: &Rect,
    ) -> OpacityLayerStart {
        let Some(rect) = intersection(
            screen_rect(bounds, scroll_y, toolbar_height, scale),
            *content,
        )
        .and_then(|rect| intersection(rect, *dirty)) else {
            return OpacityLayerStart::Hidden;
        };
        let source = CreateCompatibleDC(destination);
        if source.is_null() {
            return OpacityLayerStart::Layer(Self::passthrough(destination));
        }
        let bitmap = CreateCompatibleBitmap(destination, rect.width().max(1), rect.height().max(1));
        if bitmap.is_null() {
            DeleteDC(source);
            return OpacityLayerStart::Layer(Self::passthrough(destination));
        }
        let previous = SelectObject(source, bitmap);
        SetViewportOrgEx(source, -rect.left, -rect.top, null_mut());
        IntersectClipRect(source, rect.left, rect.top, rect.right, rect.bottom);
        BitBlt(
            source,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            destination,
            rect.left,
            rect.top,
            SRCCOPY,
        );
        OpacityLayerStart::Layer(Self {
            destination,
            source,
            bitmap,
            previous,
            rect,
            opacity: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        })
    }

    fn passthrough(destination: Hdc) -> Self {
        Self {
            destination,
            source: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
            rect: Rect::default(),
            opacity: 255,
        }
    }

    pub(super) fn dc(&self) -> Option<Hdc> {
        (!self.source.is_null()).then_some(self.source)
    }

    pub(super) unsafe fn finish(self) {
        if self.source.is_null() {
            return;
        }
        AlphaBlend(
            self.destination,
            self.rect.left,
            self.rect.top,
            self.rect.width(),
            self.rect.height(),
            self.source,
            self.rect.left,
            self.rect.top,
            self.rect.width(),
            self.rect.height(),
            BlendFunction {
                operation: 0,
                flags: 0,
                source_constant_alpha: self.opacity,
                alpha_format: 0,
            },
        );
        if !self.previous.is_null() {
            SelectObject(self.source, self.previous);
        }
        DeleteObject(self.bitmap);
        DeleteDC(self.source);
    }
}

fn intersection(left: Rect, right: Rect) -> Option<Rect> {
    let rect = Rect {
        left: left.left.max(right.left),
        top: left.top.max(right.top),
        right: left.right.min(right.right),
        bottom: left.bottom.min(right.bottom),
    };
    (rect.width() > 0 && rect.height() > 0).then_some(rect)
}
