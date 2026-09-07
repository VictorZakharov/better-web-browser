//! Validated hidden pointer and keyboard action arguments.
use super::BenchmarkNavigation;

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
