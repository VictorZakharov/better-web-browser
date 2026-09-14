use super::*;

#[test]
fn benchmark_scale_is_explicit_and_interactive_scale_stays_native() {
    let parse = |arguments: Vec<&str>| {
        LaunchOptions::parse_from(Instant::now(), arguments.into_iter().map(str::to_string))
    };
    assert!(parse(vec![]).unwrap().dpi_override.is_none());
    for (scale, expected) in [("1", 96), ("1.25", 120), ("2", 192)] {
        let options = parse(vec![
            "--benchmark",
            "about:blank",
            "--output",
            "result.json",
            "--device-scale-factor",
            scale,
        ])
        .unwrap();
        assert_eq!(options.dpi_override, Some(expected));
        assert_eq!(unsafe { options.initial_dpi() }, expected);
    }
    for scale in ["NaN", "inf", "0", "0.5", "4.1"] {
        assert!(
            parse(vec![
                "--benchmark",
                "about:blank",
                "--output",
                "result.json",
                "--device-scale-factor",
                scale
            ])
            .is_err()
        );
    }
    assert!(
        parse(vec!["--device-scale-factor", "1"])
            .err()
            .unwrap()
            .contains("requires --benchmark")
    );
}

#[test]
fn endurance_settle_is_bounded_but_can_outlive_a_short_video() {
    for (requested, expected) in [(245_000_u64, 245_000_u64), (900_000, 600_000)] {
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark".to_string(),
                "https://example.test".to_string(),
                "--output".to_string(),
                "report.json".to_string(),
                "--settle-ms".to_string(),
                requested.to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            options.benchmark.unwrap().settle,
            Duration::from_millis(expected)
        );
    }
}

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
            "--move-after-ready",
            "320,180",
            "--click-after-ready",
            "320,180",
            "--scroll-after-ready",
            "800",
            "--key-after-ready",
            "k,KeyK",
            "--scroll-after-ready",
            "0",
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
            BenchmarkNavigation::MovePoint { x: 320, y: 180 },
            BenchmarkNavigation::ClickPoint { x: 320, y: 180 },
            BenchmarkNavigation::ScrollTo { y: 800 },
            BenchmarkNavigation::Key {
                key: "k".to_string(),
                code: "KeyK".to_string(),
            },
            BenchmarkNavigation::ScrollTo { y: 0 },
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
