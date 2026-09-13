//! Dirty-region backbuffer allocation and paint lifecycle.
use super::*;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn paint(&mut self) {
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
}
