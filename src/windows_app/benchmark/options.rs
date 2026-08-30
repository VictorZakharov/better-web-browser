//! Command-line parsing for interactive launch and hidden benchmark modes.

use super::super::*;
use super::{BenchmarkRun, diagnostics, navigation::BenchmarkNavigation};

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
        let mut filmstrip_directory = None;
        let mut filmstrip_interval_ms = None;
        let mut filmstrip_duration_ms = None;
        let mut settle_ms = 2_000_u64;
        let mut completion_marker = None;
        let mut scroll_samples = 0_usize;
        let mut early_scroll = false;
        let mut diagnostic_selectors = Vec::new();
        let mut navigation_targets = Vec::new();
        let mut navigation_delay_ms = 0_u64;
        let mut window_width_dip = None;
        let mut window_height_dip = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--benchmark" => benchmark_url = Some(required(&mut arguments, &argument)?),
                "--output" => output = Some(PathBuf::from(required(&mut arguments, &argument)?)),
                "--screenshot" => {
                    screenshot = Some(PathBuf::from(required(&mut arguments, &argument)?));
                }
                "--filmstrip-directory" => {
                    filmstrip_directory = Some(PathBuf::from(required(&mut arguments, &argument)?));
                }
                "--filmstrip-interval-ms" => {
                    filmstrip_interval_ms =
                        Some(number::<u64>(&mut arguments, &argument)?.clamp(100, 10_000));
                }
                "--filmstrip-duration-ms" => {
                    filmstrip_duration_ms =
                        Some(number::<u64>(&mut arguments, &argument)?.clamp(100, 60_000));
                }
                "--settle-ms" => {
                    settle_ms = number::<u64>(&mut arguments, &argument)?.clamp(100, 60_000);
                }
                "--completion-marker" => {
                    let marker = required(&mut arguments, &argument)?;
                    if marker.is_empty() {
                        return Err("--completion-marker cannot be empty".to_string());
                    }
                    completion_marker = Some(marker);
                }
                "--scroll-samples" => {
                    scroll_samples = number::<usize>(&mut arguments, &argument)?.clamp(1, 120);
                }
                "--early-scroll-trace" => early_scroll = true,
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
                "--navigate-after-ready" => {
                    navigation_targets.push(BenchmarkNavigation::Address(required(
                        &mut arguments,
                        &argument,
                    )?));
                }
                "--activate-link-after-ready" => {
                    navigation_targets.push(BenchmarkNavigation::ActivateLink(required(
                        &mut arguments,
                        &argument,
                    )?));
                }
                "--activate-selector-after-ready" => {
                    let selector = required(&mut arguments, &argument)?;
                    if selector.trim().is_empty() {
                        return Err("--activate-selector-after-ready cannot be empty".to_string());
                    }
                    if !diagnostic_selectors.contains(&selector) {
                        diagnostic_selectors.push(selector.clone());
                        diagnostics::validate_selector_count(&diagnostic_selectors)?;
                    }
                    navigation_targets.push(BenchmarkNavigation::ActivateSelector(selector));
                }
                "--click-after-ready" => {
                    navigation_targets.push(click_point(&required(&mut arguments, &argument)?)?);
                }
                "--key-after-ready" => {
                    navigation_targets.push(key_input(&required(&mut arguments, &argument)?)?);
                }
                "--navigation-delay-ms" => {
                    navigation_delay_ms =
                        number::<u64>(&mut arguments, &argument)?.clamp(0, 60_000);
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
            let mut benchmark = BenchmarkRun::new(
                url,
                output,
                screenshot,
                Duration::from_millis(settle_ms),
                completion_marker,
                scroll_samples,
                early_scroll,
                diagnostic_selectors,
                window_width_dip.unwrap_or(DEFAULT_WINDOW_WIDTH_DIP),
                window_height_dip.unwrap_or(DEFAULT_WINDOW_HEIGHT_DIP),
                process_started,
            );
            benchmark.navigation_targets = navigation_targets;
            benchmark.navigation_delay = Duration::from_millis(navigation_delay_ms);
            benchmark.filmstrip = filmstrip_directory
                .map(|directory| {
                    super::filmstrip::Filmstrip::new(
                        directory,
                        Duration::from_millis(filmstrip_interval_ms.unwrap_or(500)),
                        Duration::from_millis(filmstrip_duration_ms.unwrap_or(10_000)),
                    )
                })
                .transpose()?;
            if benchmark.filmstrip.is_none()
                && (filmstrip_interval_ms.is_some() || filmstrip_duration_ms.is_some())
            {
                return Err("filmstrip timing requires --filmstrip-directory".into());
            }
            Some(benchmark)
        } else {
            if screenshot.is_some() {
                return Err("--screenshot requires --benchmark".to_string());
            }
            if filmstrip_directory.is_some()
                || filmstrip_interval_ms.is_some()
                || filmstrip_duration_ms.is_some()
            {
                return Err("filmstrip options require --benchmark".to_string());
            }
            if scroll_samples > 0 {
                return Err("--scroll-samples requires --benchmark".to_string());
            }
            if completion_marker.is_some() {
                return Err("--completion-marker requires --benchmark".to_string());
            }
            if early_scroll {
                return Err("--early-scroll-trace requires --benchmark".to_string());
            }
            if !diagnostic_selectors.is_empty() {
                return Err("--diagnostic-selector requires --benchmark".to_string());
            }
            if !navigation_targets.is_empty() || navigation_delay_ms > 0 {
                return Err("benchmark navigation options require --benchmark".to_string());
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

fn click_point(value: &str) -> Result<BenchmarkNavigation, String> {
    let Some((x, y)) = value.split_once(',') else {
        return Err("--click-after-ready requires x,y".into());
    };
    let x = x
        .trim()
        .parse::<i32>()
        .map_err(|_| "--click-after-ready requires integer x,y".to_string())?;
    let y = y
        .trim()
        .parse::<i32>()
        .map_err(|_| "--click-after-ready requires integer x,y".to_string())?;
    if x < 0 || y < 0 {
        return Err("--click-after-ready coordinates cannot be negative".into());
    }
    Ok(BenchmarkNavigation::ClickPoint { x, y })
}

fn key_input(value: &str) -> Result<BenchmarkNavigation, String> {
    let Some((key, code)) = value.split_once(',') else {
        return Err("--key-after-ready requires key,code".into());
    };
    let key = key.trim();
    let code = code.trim();
    if key.is_empty() || code.is_empty() || key.len() > 64 || code.len() > 64 {
        return Err("--key-after-ready requires non-empty key,code values up to 64 bytes".into());
    }
    Ok(BenchmarkNavigation::Key {
        key: key.to_string(),
        code: code.to_string(),
    })
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
                "--early-scroll-trace",
                "--diagnostic-selector",
                "#main",
                "--completion-marker",
                "__DONE__",
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
        assert!(benchmark.early_scroll.is_some());
        assert_eq!(benchmark.diagnostic_selectors, ["#main"]);
        assert_eq!(benchmark.completion_marker.as_deref(), Some("__DONE__"));
    }

    #[test]
    fn parses_navigation_anchored_filmstrip_options() {
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark",
                "https://example.test",
                "--output",
                "report.json",
                "--filmstrip-directory",
                "frames",
                "--filmstrip-interval-ms",
                "500",
                "--filmstrip-duration-ms",
                "5000",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let filmstrip = options.benchmark.unwrap().filmstrip.unwrap();
        assert_eq!(filmstrip.interval, Duration::from_millis(500));
        assert_eq!(filmstrip.duration, Duration::from_secs(5));
        assert_eq!(filmstrip.frame_count, 10);
    }

    #[test]
    fn parses_ordered_hidden_navigation_sequence() {
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark",
                "https://example.test/first",
                "--output",
                "result.json",
                "--navigate-after-ready",
                "https://example.test/second",
                "--navigate-after-ready",
                "https://example.test/final",
                "--activate-link-after-ready",
                "https://example.test/clicked",
                "--activate-selector-after-ready",
                "button.play",
                "--click-after-ready",
                "320,180",
                "--key-after-ready",
                "k,KeyK",
                "--navigation-delay-ms",
                "750",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let benchmark = options.benchmark.unwrap();
        assert_eq!(
            benchmark.navigation_targets,
            [
                BenchmarkNavigation::Address("https://example.test/second".to_string()),
                BenchmarkNavigation::Address("https://example.test/final".to_string()),
                BenchmarkNavigation::ActivateLink("https://example.test/clicked".to_string()),
                BenchmarkNavigation::ActivateSelector("button.play".to_string()),
                BenchmarkNavigation::ClickPoint { x: 320, y: 180 },
                BenchmarkNavigation::Key {
                    key: "k".to_string(),
                    code: "KeyK".to_string(),
                },
            ]
        );
        assert_eq!(benchmark.diagnostic_selectors, ["button.play"]);
        assert_eq!(benchmark.navigation_delay, Duration::from_millis(750));
    }

    #[test]
    fn rejects_navigation_sequence_outside_hidden_benchmark_mode() {
        let error = LaunchOptions::parse_from(
            Instant::now(),
            ["--navigate-after-ready", "https://example.test/second"]
                .into_iter()
                .map(str::to_string),
        )
        .err()
        .expect("interactive navigation sequence is rejected");
        assert!(error.contains("require --benchmark"));
    }
}
