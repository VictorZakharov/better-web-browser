use super::*;

#[test]
fn first_action_delay_is_bounded_and_zero_preserves_normal_spacing() {
    for (delay, expected) in [
        ("0", None),
        ("12000", Some(Duration::from_secs(12))),
        ("60000", Some(Duration::from_secs(60))),
    ] {
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark",
                "about:blank",
                "--output",
                "report.json",
                "--wheel-after-ready",
                "600,300,600",
                "--navigation-delay-ms",
                "1000",
                "--initial-action-delay-ms",
                delay,
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let benchmark = options.benchmark.unwrap();
        assert_eq!(benchmark.initial_action_delay, expected);
        assert_eq!(benchmark.navigation_delay, Duration::from_secs(1));
    }
    for delay in ["-1", "60001", "18446744073709551615", "NaN"] {
        assert!(
            LaunchOptions::parse_from(
                Instant::now(),
                [
                    "--benchmark",
                    "about:blank",
                    "--output",
                    "report.json",
                    "--initial-action-delay-ms",
                    delay
                ]
                .into_iter()
                .map(str::to_string)
            )
            .is_err()
        );
    }
    assert!(
        LaunchOptions::parse_from(
            Instant::now(),
            ["--initial-action-delay-ms", "0"]
                .into_iter()
                .map(str::to_string)
        )
        .is_err()
    );
}
