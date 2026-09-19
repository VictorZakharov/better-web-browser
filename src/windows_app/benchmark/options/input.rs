//! Validated hidden pointer, keyboard, and scroll action arguments.
use super::BenchmarkNavigation;

pub(super) fn control_value_input(
    selector: &str,
    value: &str,
) -> Result<BenchmarkNavigation, String> {
    if selector.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err("--set-control-value-after-ready requires a selector and a value up to 4096 bytes without NUL".into());
    }
    Ok(BenchmarkNavigation::SetControlValue {
        selector: selector.to_string(),
        value: value.to_string(),
    })
}

pub(super) fn wheel_input(value: &str) -> Result<BenchmarkNavigation, String> {
    let error = "--wheel-after-ready requires x,y,delta (CSS viewport coordinates and pixel delta)";
    let values = value
        .split(',')
        .map(|v| v.trim().parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error.to_string())?;
    let [x, y, delta] = values.as_slice() else {
        return Err(error.into());
    };
    if !(0..=7680).contains(x) || !(0..=4320).contains(y) || !(-10000..=10000).contains(delta) {
        return Err(error.into());
    }
    Ok(BenchmarkNavigation::Wheel {
        x: *x,
        y: *y,
        delta: *delta,
    })
}

pub(super) fn scroll_input(value: &str) -> Result<BenchmarkNavigation, String> {
    let value = value.trim();
    let error = || "--scroll-after-ready requires an integer CSS y offset from 0 to 2147483647";
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error().into());
    }
    let y = value.parse::<i32>().map_err(|_| error().to_string())?;
    Ok(BenchmarkNavigation::ScrollTo { y })
}

pub(super) fn point_input(value: &str, option: &str) -> Result<BenchmarkNavigation, String> {
    let Some((x, y)) = value.split_once(',') else {
        return Err(format!("{option} requires x,y"));
    };
    let coordinate = |value: &str| {
        value
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("{option} requires integer x,y"))
    };
    let (x, y) = (coordinate(x)?, coordinate(y)?);
    if x < 0 || y < 0 {
        return Err(format!("{option} coordinates cannot be negative"));
    }
    Ok(if option == "--move-after-ready" {
        BenchmarkNavigation::MovePoint { x, y }
    } else {
        BenchmarkNavigation::ClickPoint { x, y }
    })
}

pub(super) fn key_input(value: &str) -> Result<BenchmarkNavigation, String> {
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
    use crate::windows_app::benchmark::options::LaunchOptions;
    use std::time::Instant;

    #[test]
    fn control_value_arguments_are_bounded_and_require_hidden_mode() {
        for value in ["", "a,b=c", "a 🦀"] {
            assert_eq!(
                control_value_input("#query", value).unwrap(),
                BenchmarkNavigation::SetControlValue {
                    selector: "#query".into(),
                    value: value.into()
                }
            );
        }
        assert!(control_value_input(" ", "value").is_err());
        assert!(control_value_input("#q", &"a".repeat(4097)).is_err());
        assert!(control_value_input("#q", "a\0b").is_err());
        for arguments in [
            vec!["--set-control-value-after-ready", "#query", "new query"],
            vec![
                "--benchmark",
                "about:blank",
                "--output",
                "result.json",
                "--set-control-value-after-ready",
                "#query",
            ],
        ] {
            assert!(
                LaunchOptions::parse_from(
                    Instant::now(),
                    arguments.into_iter().map(str::to_string)
                )
                .is_err()
            );
        }
        let options = LaunchOptions::parse_from(
            Instant::now(),
            [
                "--benchmark",
                "about:blank",
                "--output",
                "result.json",
                "--set-control-value-after-ready",
                "#query",
                "new query",
                "--key-after-ready",
                "Enter,Enter",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let actions = options.benchmark.unwrap().navigation_targets;
        assert_eq!(actions.len(), 2);
        assert!(matches!(
            actions[0],
            BenchmarkNavigation::SetControlValue { .. }
        ));
        assert!(matches!(actions[1], BenchmarkNavigation::Key { .. }));
    }

    #[test]
    fn wheel_arguments_are_bounded_and_require_hidden_mode() {
        assert_eq!(
            wheel_input("300,100,-126").unwrap(),
            BenchmarkNavigation::Wheel {
                x: 300,
                y: 100,
                delta: -126
            }
        );
        for value in ["", "1,2", "-1,2,3", "1,2,NaN", "1,2,10001", "7681,1,1"] {
            assert!(wheel_input(value).is_err());
        }
        assert!(
            LaunchOptions::parse_from(
                Instant::now(),
                ["--wheel-after-ready", "1,2,3"]
                    .into_iter()
                    .map(str::to_string)
            )
            .is_err()
        );
    }

    #[test]
    fn validates_bounded_integer_scroll_targets() {
        for (value, y) in [("0", 0), (" 800 ", 800), ("2147483647", i32::MAX)] {
            assert_eq!(
                scroll_input(value).unwrap(),
                BenchmarkNavigation::ScrollTo { y }
            );
        }
        for value in [
            "",
            " ",
            "-1",
            "+1",
            "1.5",
            "1e3",
            "NaN",
            "inf",
            "800,0",
            "2147483648",
        ] {
            assert!(scroll_input(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn scroll_requires_hidden_mode_and_a_value() {
        for arguments in [
            vec!["--scroll-after-ready", "800"],
            vec!["--scroll-after-ready"],
        ] {
            assert!(
                LaunchOptions::parse_from(
                    Instant::now(),
                    arguments.into_iter().map(str::to_string)
                )
                .is_err()
            );
        }
    }
}
