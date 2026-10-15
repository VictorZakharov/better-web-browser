use super::*;

#[test]
fn held_keys_keep_phase_order_and_require_hidden_benchmark_mode() {
    let parse = |arguments: &[&str]| {
        LaunchOptions::parse_from(Instant::now(), arguments.iter().map(|s| s.to_string()))
    };
    let options = parse(&[
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
        "--key-down-after-ready",
        "w,KeyW",
        "--pause-after-ready",
        "1000",
        "--key-up-after-ready",
        "w,KeyW",
    ])
    .unwrap();
    assert_eq!(
        options.benchmark.unwrap().navigation_targets,
        vec![
            BenchmarkNavigation::KeyState {
                key: "w".into(),
                code: "KeyW".into(),
                down: true
            },
            BenchmarkNavigation::Pause { milliseconds: 1000 },
            BenchmarkNavigation::KeyState {
                key: "w".into(),
                code: "KeyW".into(),
                down: false
            },
        ]
    );
    for option in ["--key-down-after-ready", "--key-up-after-ready"] {
        assert!(
            parse(&[option, "w,KeyW"])
                .err()
                .unwrap()
                .contains("require --benchmark")
        );
        for value in ["", "w", ",KeyW", "w,"] {
            assert!(
                parse(&[
                    "--benchmark",
                    "about:blank",
                    "--output",
                    "r.json",
                    option,
                    value
                ])
                .is_err()
            );
        }
    }
}
