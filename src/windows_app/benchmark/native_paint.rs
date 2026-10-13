//! Replay the native backbuffer's drawing into an owned device-dependent bitmap.
//! Hidden benchmarks ordinarily do not process visible WM_PAINT messages. This
//! opt-in trace includes allocation, normal display-list painting and GDI flush,
//! but not DWM/window presentation. It is a diagnostic, not a gameplay FPS claim.
use super::*;
use crate::windows_app::painting::profile;
use std::collections::VecDeque;

const LIMIT: usize = 256;

#[derive(Default)]
pub(in crate::windows_app) struct NativePaintTrace {
    count: usize,
    samples: VecDeque<serde_json::Value>,
    installations: VecDeque<serde_json::Value>,
    installation_count: usize,
}

impl NativePaintTrace {
    pub(super) fn record_installation(&mut self, sample: serde_json::Value) {
        self.installation_count += 1;
        if self.installations.len() == LIMIT {
            self.installations.pop_front();
        }
        self.installations.push_back(sample);
    }

    fn record(&mut self, sample: serde_json::Value) {
        self.count += 1;
        if self.samples.len() == LIMIT {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub(super) fn to_json(&self) -> String {
        serde_json::json!({
            "scope": "offscreen GDI compatible bitmap; excludes active GPU page content and DWM presentation",
            "total_samples": self.count,
            "retained_samples": self.samples,
            "total_installations": self.installation_count,
            "installations": self.installations,
        })
        .to_string()
    }
}

struct Surface {
    window: Hwnd,
    window_dc: Hdc,
    dc: Hdc,
    bitmap: Hbitmap,
    previous: Hgdiobj,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.window_dc.is_null() {
                ReleaseDC(self.window, self.window_dc);
            }
        }
    }
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn paint_benchmark_native_presentation(&mut self) {
        let Some(benchmark) = self
            .benchmark
            .as_ref()
            .filter(|run| run.native_paint.is_some())
        else {
            return;
        };
        let elapsed_ms = benchmark.process_started.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let result = self.replay_native_paint();
        let paint_ms = start.elapsed().as_secs_f64() * 1000.0;
        let sample = match result {
            Ok((flush_ms, totals)) => serde_json::json!({
                "elapsed_ms": elapsed_ms, "paint_ms": paint_ms,
                "flush_ms": flush_ms, "items": totals.to_json(),
                "gpu_content_active":self.gpu_content_active(),
            }),
            Err(error) => serde_json::json!({
                "elapsed_ms": elapsed_ms, "paint_ms": paint_ms, "error": error,
            }),
        };
        if let Some(trace) = self
            .benchmark
            .as_mut()
            .and_then(|run| run.native_paint.as_mut())
        {
            trace.record(sample);
        }
    }

    unsafe fn replay_native_paint(&mut self) -> Result<(f64, profile::Totals), String> {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) == 0
            || client.width() <= 0
            || client.height() <= 0
            || i64::from(client.width()) * i64::from(client.height()) * 4 > 64 * 1024 * 1024
        {
            return Err("native paint trace surface exceeds the 64 MiB limit".into());
        }
        let mut surface = Surface {
            window: self.window,
            window_dc: GetDC(self.window),
            dc: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
        };
        if surface.window_dc.is_null() {
            return Err(last_error("native paint trace DC"));
        }
        surface.dc = CreateCompatibleDC(surface.window_dc);
        if surface.dc.is_null() {
            return Err(last_error("native paint trace memory DC"));
        }
        surface.bitmap = CreateCompatibleBitmap(surface.window_dc, client.width(), client.height());
        if surface.bitmap.is_null() {
            return Err(last_error("native paint trace bitmap"));
        }
        surface.previous = SelectObject(surface.dc, surface.bitmap);
        if surface.previous.is_null() || surface.previous as usize == usize::MAX {
            surface.previous = null_mut();
            return Err(last_error("native paint trace bitmap selection"));
        }
        let capture = profile::Capture::start();
        self.paint_surface(surface.dc, &client, &client);
        let flush_started = Instant::now();
        if GdiFlush() == 0 {
            return Err(last_error("native paint trace GDI flush"));
        }
        Ok((
            flush_started.elapsed().as_secs_f64() * 1000.0,
            capture.totals(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_retains_a_bounded_tail_without_losing_total_count() {
        let mut trace = NativePaintTrace::default();
        for index in 0..300 {
            trace.record(serde_json::json!({"index": index}));
            trace.record_installation(serde_json::json!({"index": index}));
        }
        let result: serde_json::Value = serde_json::from_str(&trace.to_json()).unwrap();
        assert_eq!(result["total_samples"], 300);
        let samples = result["retained_samples"].as_array().unwrap();
        assert_eq!(samples.len(), LIMIT);
        assert_eq!(samples[0]["index"], 44);
        assert_eq!(samples.last().unwrap()["index"], 299);
        assert_eq!(result["total_installations"], 300);
        let installs = result["installations"].as_array().unwrap();
        assert_eq!(installs.len(), LIMIT);
        assert_eq!(installs[0]["index"], 44);
    }
}
