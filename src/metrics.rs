use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct BrowserMetrics {
    bytes_downloaded: AtomicU64,
    pages_loaded: AtomicU64,
    failed_loads: AtomicU64,
    active_requests: AtomicU64,
    last_parse_micros: AtomicU64,
    retained_draw_items: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsSnapshot {
    pub bytes_downloaded: u64,
    pub pages_loaded: u64,
    pub failed_loads: u64,
    pub active_requests: u64,
    pub last_parse_micros: u64,
    pub retained_draw_items: u64,
}

impl BrowserMetrics {
    pub fn begin_request(self: &Arc<Self>) -> RequestGuard {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        RequestGuard {
            metrics: Arc::clone(self),
        }
    }

    pub fn record_success(&self, bytes: u64, parse_micros: u64) {
        self.bytes_downloaded.fetch_add(bytes, Ordering::Relaxed);
        self.pages_loaded.fetch_add(1, Ordering::Relaxed);
        self.last_parse_micros
            .store(parse_micros, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failed_loads.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_retained_draw_items(&self, count: usize) {
        self.retained_draw_items
            .store(count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            bytes_downloaded: self.bytes_downloaded.load(Ordering::Relaxed),
            pages_loaded: self.pages_loaded.load(Ordering::Relaxed),
            failed_loads: self.failed_loads.load(Ordering::Relaxed),
            active_requests: self.active_requests.load(Ordering::Relaxed),
            last_parse_micros: self.last_parse_micros.load(Ordering::Relaxed),
            retained_draw_items: self.retained_draw_items.load(Ordering::Relaxed),
        }
    }
}

pub struct RequestGuard {
    metrics: Arc<BrowserMetrics>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.metrics.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}
