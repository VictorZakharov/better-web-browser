//! Validated hidden pointer, keyboard, and scroll action arguments.
use super::BenchmarkNavigation;

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
