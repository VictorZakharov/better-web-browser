//! Hidden offscreen rendering used by benchmark screenshots and scroll samples.

use super::paint_primitives::draw_text_in_rect;
use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScrollPaintMetrics {
    pub(super) samples: usize,
    pub(super) average: Duration,
    pub(super) maximum: Duration,
}

impl BrowserState {
    pub(super) unsafe fn prepare_benchmark_scroll_surface(&mut self) -> Result<(), String> {
        let mut surface = OffscreenSurface::new(self)?;
        self.paint_surface(surface.dc, &surface.client, &surface.client);
        surface.scroll_y = Some(self.scroll_y);
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.scroll_surface = Some(surface);
        }
        Ok(())
    }

    pub(super) unsafe fn paint_benchmark_frame(&mut self) -> Result<Duration, String> {
        let started = Instant::now();
        let mut surface = self
            .benchmark
            .as_mut()
            .and_then(|benchmark| benchmark.scroll_surface.take())
            .map(Ok)
            .unwrap_or_else(|| OffscreenSurface::new(self))?;
        let content = Rect {
            left: 0,
            top: self.toolbar_height(),
            right: surface.client.right,
            bottom: (surface.client.bottom - self.status_height()).max(self.toolbar_height()),
        };
        let dirty = surface
            .scroll_y
            .and_then(|previous| scroll_exposed_strip(content, previous - self.scroll_y));
        if let Some(dirty) = dirty {
            shift_scroll_pixels(
                surface.dc,
                content,
                previous_scroll_delta(&surface, self.scroll_y),
            );
            let saved = SaveDC(surface.dc);
            IntersectClipRect(surface.dc, dirty.left, dirty.top, dirty.right, dirty.bottom);
            self.paint_surface(surface.dc, &surface.client, &dirty);
            RestoreDC(surface.dc, saved);
        } else {
            self.paint_surface(surface.dc, &surface.client, &surface.client);
        }
        surface.scroll_y = Some(self.scroll_y);
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.scroll_surface = Some(surface);
        }
        let elapsed = started.elapsed();
        self.record_benchmark_paint(elapsed);
        Ok(elapsed)
    }

    pub(super) unsafe fn measure_scroll_paints(
        &mut self,
        sample_count: usize,
    ) -> Result<ScrollPaintMetrics, String> {
        if sample_count == 0 {
            return Ok(ScrollPaintMetrics::default());
        }
        let original_scroll = self.scroll_y;
        let maximum_scroll = (self.content_height - self.viewport_height()).max(0);
        let mut total = Duration::ZERO;
        let mut maximum = Duration::ZERO;
        for sample in 1..=sample_count {
            self.scroll_y =
                ((maximum_scroll as i64 * sample as i64) / (sample_count as i64 + 1)) as i32;
            let started = Instant::now();
            {
                let surface = OffscreenSurface::new(self)?;
                self.paint_surface(surface.dc, &surface.client, &surface.client);
            }
            let elapsed = started.elapsed();
            total += elapsed;
            maximum = maximum.max(elapsed);
        }
        self.scroll_y = original_scroll;
        Ok(ScrollPaintMetrics {
            samples: sample_count,
            average: total / sample_count as u32,
            maximum,
        })
    }

    pub(super) unsafe fn capture_screenshot(
        &mut self,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let surface = OffscreenSurface::new(self)?;
        self.paint_surface(surface.dc, &surface.client, &surface.client);
        if let Some(fonts) = self.fonts.as_ref() {
            SelectObject(surface.dc, fonts.ui);
            SetTextColor(surface.dc, CHROME_THEME.text);
            SetBkMode(surface.dc, TRANSPARENT);
            let mut address_rect = self
                .chrome
                .address_frame
                .inset(self.scale(16), self.scale(1));
            let address = window_text(self.controls.address);
            draw_text_in_rect(
                surface.dc,
                &address,
                &mut address_rect,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        let bgra = std::slice::from_raw_parts(surface.pixels.cast::<u8>(), surface.byte_len);
        let mut rgba = Vec::with_capacity(surface.byte_len);
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
        let width = surface.client.right.max(1) as u32;
        let height = surface.client.bottom.max(1) as u32;
        drop(surface);

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create screenshot directory: {error}"))?;
        }
        image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8)
            .map_err(|error| format!("write screenshot: {error}"))
    }
}

pub(super) struct OffscreenSurface {
    window: Hwnd,
    window_dc: Hdc,
    dc: Hdc,
    bitmap: Hbitmap,
    previous: Handle,
    pixels: *mut std::ffi::c_void,
    byte_len: usize,
    client: Rect,
    scroll_y: Option<i32>,
}

impl OffscreenSurface {
    unsafe fn new(state: &BrowserState) -> Result<Self, String> {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(state.window, &mut client) == 0 {
            return Err(last_error("measure benchmark capture"));
        }
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let byte_len = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "benchmark capture is too large".to_string())?;
        let window_dc = GetDC(state.window);
        if window_dc.is_null() {
            return Err(last_error("open benchmark capture surface"));
        }
        let dc = CreateCompatibleDC(window_dc);
        if dc.is_null() {
            ReleaseDC(state.window, window_dc);
            return Err(last_error("create benchmark capture surface"));
        }
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width,
                height: -height,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: byte_len.min(u32::MAX as usize) as u32,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                colors_important: 0,
            },
            colors: [0],
        };
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(window_dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if bitmap.is_null() || pixels.is_null() {
            DeleteDC(dc);
            ReleaseDC(state.window, window_dc);
            return Err(last_error("allocate benchmark capture bitmap"));
        }
        let previous = SelectObject(dc, bitmap);
        Ok(Self {
            window: state.window,
            window_dc,
            dc,
            bitmap,
            previous,
            pixels,
            byte_len,
            client,
            scroll_y: None,
        })
    }
}

fn previous_scroll_delta(surface: &OffscreenSurface, scroll_y: i32) -> i32 {
    surface.scroll_y.unwrap_or(scroll_y) - scroll_y
}

fn scroll_exposed_strip(content: Rect, delta: i32) -> Option<Rect> {
    if delta == 0 || delta.unsigned_abs() >= content.height() as u32 {
        return None;
    }
    if delta < 0 {
        Some(Rect {
            top: content.bottom + delta,
            ..content
        })
    } else {
        Some(Rect {
            bottom: content.top + delta,
            ..content
        })
    }
}

unsafe fn shift_scroll_pixels(dc: Hdc, content: Rect, delta: i32) {
    let height = content.height() - delta.unsigned_abs() as i32;
    let (destination_top, source_top) = if delta < 0 {
        (content.top, content.top - delta)
    } else {
        (content.top + delta, content.top)
    };
    BitBlt(
        dc,
        content.left,
        destination_top,
        content.width(),
        height,
        dc,
        content.left,
        source_top,
        SRCCOPY,
    );
}

impl Drop for OffscreenSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
            ReleaseDC(self.window, self.window_dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: Rect = Rect {
        left: 0,
        top: 80,
        right: 1280,
        bottom: 720,
    };

    #[test]
    fn downward_scroll_exposes_the_bottom_strip() {
        let strip = scroll_exposed_strip(CONTENT, -42).expect("exposed strip");
        assert_eq!(strip.left, CONTENT.left);
        assert_eq!(strip.top, 678);
        assert_eq!(strip.right, CONTENT.right);
        assert_eq!(strip.bottom, CONTENT.bottom);
    }

    #[test]
    fn upward_scroll_exposes_the_top_strip() {
        let strip = scroll_exposed_strip(CONTENT, 42).expect("exposed strip");
        assert_eq!(strip.left, CONTENT.left);
        assert_eq!(strip.top, CONTENT.top);
        assert_eq!(strip.right, CONTENT.right);
        assert_eq!(strip.bottom, 122);
    }

    #[test]
    fn unchanged_or_full_viewport_scroll_requires_a_full_paint() {
        assert!(scroll_exposed_strip(CONTENT, 0).is_none());
        assert!(scroll_exposed_strip(CONTENT, CONTENT.height()).is_none());
    }
}
