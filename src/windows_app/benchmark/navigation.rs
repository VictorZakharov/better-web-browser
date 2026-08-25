//! Hidden in-process navigation sequences for lifecycle regression coverage.

use super::super::*;

impl BrowserState {
    pub(in crate::windows_app) fn schedule_benchmark_navigation(&mut self) -> bool {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return false;
        };
        if benchmark.navigation_targets.is_empty() || benchmark.navigation_scheduled {
            return false;
        }
        benchmark.navigation_scheduled = true;
        post_navigation(self.window, benchmark.navigation_delay);
        true
    }

    pub(in crate::windows_app) unsafe fn run_benchmark_navigation(&mut self) {
        let target = self.benchmark.as_mut().and_then(|benchmark| {
            benchmark.navigation_scheduled = false;
            (!benchmark.navigation_targets.is_empty())
                .then(|| benchmark.navigation_targets.remove(0))
        });
        if let Some(target) = target {
            self.begin_navigation(target, browser_navigation::HistoryMode::Push);
        }
    }
}

fn post_navigation(window: Hwnd, delay: Duration) {
    let window = window as usize;
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        unsafe {
            PostMessageW(window as Hwnd, WM_APP_BENCHMARK_NAVIGATE, 0, 0);
        }
    });
}
