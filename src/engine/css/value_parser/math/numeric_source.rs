//! Recover precision from a numeric token already validated by cssparser.
//! This does not classify tokens or decode units: that remains cssparser's job.
//! Its Token stores f32, which cannot preserve decimal remainder/round inputs.
//! CSS Syntax 3 §4.3.13 defines the numeric prefix before a dimension's unit.
//! https://www.w3.org/TR/css-syntax-3/#consume-number

pub(super) fn value(token: &str) -> Option<f64> {
    let bytes = token.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            end = exponent + 1;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
        }
    }
    let value = token.get(..end)?.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_precision_does_not_inherit_token_float_rounding() {
        for source in [".1s", "0.1s", "1e-1s", "100ms"] {
            let scale = if source.ends_with("ms") { 0.001 } else { 1.0 };
            assert_eq!(value(source).unwrap() * scale, 0.1, "{source}");
        }
        assert_eq!(value("16777217"), Some(16_777_217.0));
        assert_eq!(value("1e100"), Some(1e100));
        assert!(value("1e400").is_none());
        assert!(value("-0s").unwrap().is_sign_negative());
    }

    #[test]
    fn exponents_are_distinct_from_unit_letters_and_escapes() {
        for (source, expected) in [
            ("1em", 1.0),
            ("1eM", 1.0),
            ("1e3px", 1000.0),
            ("1E-2em", 0.01),
            ("+.5turn", 0.5),
            ("-.25s", -0.25),
            ("1\\73", 1.0),
            ("1e-3\\73", 0.001),
        ] {
            assert_eq!(value(source), Some(expected), "{source}");
        }
    }
}
