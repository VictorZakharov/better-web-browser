//! Command-line parsing for interactive launch and hidden benchmark modes.

use super::super::*;
use super::{BenchmarkRun, diagnostics};

pub(in crate::windows_app) struct LaunchOptions {
    pub(in crate::windows_app) startup_url: Option<String>,
    pub(in crate::windows_app) open_task_manager: bool,
    pub(in crate::windows_app) benchmark: Option<BenchmarkRun>,
}

impl LaunchOptions {
    pub(in crate::windows_app) fn parse(process_started: Instant) -> Result<Self, String> {
        Self::parse_from(process_started, std::env::args().skip(1))
    }

    pub(in crate::windows_app) fn window_dimensions(&self) -> (i32, i32) {
        self.benchmark
            .as_ref()
            .map(|benchmark| (benchmark.window_width_dip, benchmark.window_height_dip))
            .unwrap_or((DEFAULT_WINDOW_WIDTH_DIP, DEFAULT_WINDOW_HEIGHT_DIP))
    }

    fn parse_from(
        process_started: Instant,
        arguments: impl IntoIterator<Item = String>,
    ) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let mut startup_url = None;
        let mut open_task_manager = false;
        let mut benchmark_url = None;
        let mut output = None;
        let mut screenshot = None;
        let mut settle_ms = 2_000_u64;
        let mut scroll_samples = 0_usize;
        let mut diagnostic_selectors = Vec::new();
        let mut window_width_dip = None;
        let mut window_height_dip = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--benchmark" => benchmark_url = Some(required(&mut arguments, &argument)?),
                "--output" => output = Some(PathBuf::from(required(&mut arguments, &argument)?)),
                "--screenshot" => {
                    screenshot = Some(PathBuf::from(required(&mut arguments, &argument)?));
                }
                "--settle-ms" => {
                    settle_ms = number::<u64>(&mut arguments, &argument)?.clamp(100, 60_000);
                }
                "--scroll-samples" => {
                    scroll_samples = number::<usize>(&mut arguments, &argument)?.clamp(1, 120);
                }
                "--window-width" => {
                    window_width_dip =
                        Some(number::<i32>(&mut arguments, &argument)?.clamp(320, 7680));
                }
                "--window-height" => {
                    window_height_dip =
                        Some(number::<i32>(&mut arguments, &argument)?.clamp(240, 4320));
                }
                "--diagnostic-selector" => {
                    let selector = required(&mut arguments, &argument)?;
                    if selector.trim().is_empty() {
                        return Err("--diagnostic-selector cannot be empty".to_string());
                    }
                    diagnostic_selectors.push(selector);
                    diagnostics::validate_selector_count(&diagnostic_selectors)?;
                }
                "--task-manager" => open_task_manager = true,
                option if option.starts_with('-') => {
                    return Err(format!("unknown option: {option}"));
                }
                url => startup_url = Some(url.to_string()),
            }
        }

        let benchmark = if let Some(url) = benchmark_url {
            let output = output
                .ok_or_else(|| "benchmark mode requires --output <result.json>".to_string())?;
            startup_url = Some(url.clone());
            Some(BenchmarkRun::new(
                url,
                output,
                screenshot,
                Duration::from_millis(settle_ms),
                scroll_samples,
                diagnostic_selectors,
                window_width_dip.unwrap_or(DEFAULT_WINDOW_WIDTH_DIP),
                window_height_dip.unwrap_or(DEFAULT_WINDOW_HEIGHT_DIP),
                process_started,
            ))
        } else {
            if screenshot.is_some() {
                return Err("--screenshot requires --benchmark".to_string());
            }
            if scroll_samples > 0 {
                return Err("--scroll-samples requires --benchmark".to_string());
            }
            if !diagnostic_selectors.is_empty() {
                return Err("--diagnostic-selector requires --benchmark".to_string());
            }
            if window_width_dip.is_some() || window_height_dip.is_some() {
                return Err("--window-width and --window-height require --benchmark".to_string());
            }
            None
        };

        Ok(Self {
            startup_url,
            open_task_manager,
            benchmark,
        })
    }
}

fn required(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn number<T>(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    required(arguments, option)?
        .parse()
        .map_err(|_| format!("{option} requires a number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reproducible_hidden_viewport_and_diagnostics() {
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark",
                "https://example.com",
                "--output",
                "result.json",
                "--window-width",
                "1920",
                "--window-height",
                "1080",
                "--diagnostic-selector",
                "#main",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let benchmark = options.benchmark.unwrap();
        assert_eq!(
            (benchmark.window_width_dip, benchmark.window_height_dip),
            (1920, 1080)
        );
        assert_eq!(benchmark.diagnostic_selectors, ["#main"]);
    }
}
