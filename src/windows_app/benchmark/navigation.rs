//! Hidden in-process navigation sequences for lifecycle regression coverage.

use super::super::*;
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum BenchmarkNavigation {
    Address(String),
    ActivateLink(String),
}

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
            benchmark.navigation_started = Some(Instant::now());
            (!benchmark.navigation_targets.is_empty())
                .then(|| benchmark.navigation_targets.remove(0))
        });
        if let Some(target) = target {
            let result = match target {
                BenchmarkNavigation::Address(url) => {
                    self.begin_navigation(url, browser_navigation::HistoryMode::Push);
                    Ok(())
                }
                BenchmarkNavigation::ActivateLink(url) => self.activate_benchmark_link(&url),
            };
            if let Err(error) = result {
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.error = Some(error);
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
            }
        }
    }

    fn activate_benchmark_link(&mut self, expected_url: &str) -> Result<(), String> {
        let (x, y, target) = self
            .page_layout
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text {
                    rect,
                    link: Some(url),
                    node_id: Some(node),
                    ..
                } if url == expected_url => renderer_input::wire_node(*node).map(|target| {
                    (
                        rect.x + rect.width / 2.0,
                        rect.y + rect.height / 2.0,
                        target,
                    )
                }),
                _ => None,
            })
            .ok_or_else(|| format!("benchmark link was not presented: {expected_url}"))?;
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            let (document, sequence) = self
                .next_renderer_input()
                .ok_or_else(|| "benchmark link has no active renderer document".to_string())?;
            if !self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
                document,
                sequence,
                phase,
                button: PointerButton::Primary,
                x,
                y,
                modifiers: InputModifiers::default(),
                target: Some(target),
            })) {
                return Err("benchmark link activation was rejected by the renderer".into());
            }
            if phase == PointerPhase::Up {
                // Hidden activation models the same trusted gesture and document ownership as UI.
                self.transient_activation = Some((document, Instant::now()));
            }
        }
        Ok(())
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
