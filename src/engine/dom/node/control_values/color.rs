//! HTML color-state value sanitization.

pub(super) fn sanitize(value: &str) -> String {
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        value.to_ascii_lowercase()
    } else {
        "#000000".to_string()
    }
}
