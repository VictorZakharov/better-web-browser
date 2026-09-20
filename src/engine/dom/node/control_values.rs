//! Input state classification, value sanitization, and number formatting.
//!
//! Pure string functions shared by control state, validity, and numeric APIs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputValueMode {
    /// Hidden input: live value is the default.
    Default,
    /// Text-like, number, range: own live value with sanitization.
    Value,
    /// Checkbox/radio: `value` attribute or `"on"`.
    DefaultOn,
    /// File input: existing file engine owns the value.
    Filename,
    /// Submit/image/reset/button: `value` attribute.
    ButtonDefault,
}

/// Scoped input types with text-like value semantics.
pub(crate) fn text_like_type(state: &str) -> bool {
    matches!(
        state,
        "text" | "search" | "tel" | "url" | "email" | "password"
    )
}

/// Canonical input state: missing and invalid types select Text.
/// https://html.spec.whatwg.org/multipage/input.html#attr-input-type
pub(crate) fn canonical_input_state(raw: &str) -> String {
    let state = raw.to_ascii_lowercase();
    if matches!(
        state.as_str(),
        "hidden"
            | "text"
            | "search"
            | "tel"
            | "url"
            | "email"
            | "password"
            | "date"
            | "month"
            | "week"
            | "time"
            | "datetime-local"
            | "number"
            | "range"
            | "color"
            | "checkbox"
            | "radio"
            | "file"
            | "submit"
            | "image"
            | "reset"
            | "button"
    ) {
        return state;
    }
    "text".to_string()
}

/// Value mode for an input state name (already lowercased).
pub(crate) fn input_value_mode(state: &str) -> InputValueMode {
    if text_like_type(state) || state == "number" || state == "range" {
        InputValueMode::Value
    } else if state == "checkbox" || state == "radio" {
        InputValueMode::DefaultOn
    } else if state == "file" {
        InputValueMode::Filename
    } else if matches!(state, "submit" | "image" | "reset" | "button") {
        InputValueMode::ButtonDefault
    } else {
        // Hidden and date/time/color states read the default; only value-mode
        // states keep a live store.
        InputValueMode::Default
    }
}

/// Runs the per-state value sanitization algorithm.
pub(crate) fn sanitize_input_value(state: &str, value: &str) -> String {
    if text_like_type(state) || state == "password" {
        return strip_newlines(value);
    }
    if state == "url" {
        return strip_newlines(value)
            .trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'))
            .to_string();
    }
    if state == "email" {
        return strip_newlines(value)
            .trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'))
            .to_string();
    }
    if state == "number" {
        return if is_valid_float(value) {
            value.to_string()
        } else {
            String::new()
        };
    }
    if state == "range" {
        if !is_valid_float(value) {
            return String::new();
        }
        return clamp_range(value, None, None);
    }
    value.to_string()
}

pub(crate) fn strip_newlines(value: &str) -> String {
    value.replace("\r\n", "").replace(['\r', '\n'], "")
}

/// Textarea API normalization: CRLF and CR become LF.
pub(crate) fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

/// Floating-point grammar from the HTML "rules for parsing floating-point
/// number values": optional sign, digits with optional fraction, optional
/// exponent. Infinities and NaN spellings are not valid.
pub(crate) fn is_valid_float(value: &str) -> bool {
    parse_float_value(value).is_some()
}

pub(crate) fn parse_float_value(value: &str) -> Option<f64> {
    let trimmed = value.trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'));
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut index = 0;
    if bytes[index] == b'+' || bytes[index] == b'-' {
        index += 1;
    }
    let mut digits = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        digits += 1;
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            digits += 1;
            index += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let mut exponent_digits = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            exponent_digits += 1;
            index += 1;
        }
        if exponent_digits == 0 {
            return None;
        }
    }
    if index != bytes.len() {
        return None;
    }
    let parsed: f64 = trimmed.parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Best representation of the range default (midpoint, or minimum).
pub(crate) fn range_default(min_attr: &Option<String>, max_attr: &Option<String>) -> String {
    let minimum = min_attr
        .as_deref()
        .and_then(parse_float_value)
        .unwrap_or(0.0);
    let maximum = max_attr
        .as_deref()
        .and_then(parse_float_value)
        .unwrap_or(100.0);
    if maximum < minimum {
        return number_to_string(minimum);
    }
    number_to_string(minimum + (maximum - minimum) / 2.0)
}

/// Clamps a valid float string into `[minimum, maximum]` (either open).
pub(crate) fn clamp_range(value: &str, minimum: Option<f64>, maximum: Option<f64>) -> String {
    let parsed = parse_float_value(value).unwrap_or(0.0);
    let mut clamped = parsed;
    if let Some(minimum) = minimum {
        clamped = clamped.max(minimum);
    }
    if let Some(maximum) = maximum {
        clamped = clamped.min(maximum);
    }
    number_to_string(clamped)
}

/// Serializes a finite float like the platform number-to-string rule:
/// shortest round-trip digits, plain notation for magnitudes in
/// (1e-6, 1e21], exponential otherwise.
pub(crate) fn number_to_string(value: f64) -> String {
    debug_assert!(value.is_finite(), "only finite values serialize");
    if value == 0.0 {
        return "0".to_string();
    }
    // Shortest round-trip mantissa via Rust's Display, then reposition.
    let plain = format!("{value}");
    let (mantissa, point) = if plain.contains('e') {
        let debug = format!("{value:.17e}");
        let (mantissa, exponent) = debug.split_once('e').unwrap_or((debug.as_str(), "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let digits: String = mantissa
            .trim_start_matches('-')
            .chars()
            .filter(|char| *char != '.')
            .collect();
        let digits = digits.trim_end_matches('0').to_string();
        let digits = if digits.is_empty() {
            "0".to_string()
        } else {
            digits
        };
        (digits, exponent + 1)
    } else {
        let body = plain.trim_start_matches('-');
        let (before, after) = body.split_once('.').unwrap_or((body, ""));
        let int_part = before.trim_start_matches('0');
        let point = if int_part.is_empty() {
            -((after.len() - after.trim_start_matches('0').len()) as i32)
        } else {
            int_part.len() as i32
        };
        let combined = format!("{before}{after}");
        let digits = combined
            .trim_start_matches('0')
            .trim_end_matches('0')
            .to_string();
        let digits = if digits.is_empty() {
            "0".to_string()
        } else {
            digits
        };
        (digits, point)
    };
    let mut out = String::new();
    if value.is_sign_negative() {
        out.push('-');
    }
    if point <= -6 || point > 21 {
        out.push(mantissa.chars().next().unwrap_or('0'));
        if mantissa.len() > 1 {
            out.push('.');
            out.push_str(&mantissa[1..]);
        }
        out.push('e');
        let shift = point - 1;
        if shift >= 0 {
            out.push('+');
        }
        out.push_str(&shift.to_string());
        return out;
    }
    if point <= 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-point) as usize));
        out.push_str(&mantissa);
    } else if point as usize >= mantissa.len() {
        out.push_str(&mantissa);
        out.push_str(&"0".repeat(point as usize - mantissa.len()));
    } else {
        out.push_str(&mantissa[..point as usize]);
        out.push('.');
        out.push_str(&mantissa[point as usize..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_grammar_matches_the_platform_subset() {
        for valid in [
            "12",
            "-1.5",
            "+.5",
            "5.",
            "1e3",
            "1E-3",
            "  12  ",
            "\t-0.25\n",
        ] {
            assert!(is_valid_float(valid), "{valid}");
        }
        for invalid in [
            "",
            "   ",
            "abc",
            "inf",
            "-Infinity",
            "NaN",
            "0x1",
            "1.2.3",
            "e5",
            "1e",
            "+",
            ".",
            "1,000",
            "12px",
            "--1",
        ] {
            assert!(!is_valid_float(invalid), "{invalid}");
        }
        assert_eq!(parse_float_value("0.1"), Some(0.1));
        assert_eq!(parse_float_value("1e308"), Some(1e308));
        assert_eq!(parse_float_value("1e309"), None);
    }

    #[test]
    fn number_serialization_uses_shortest_plain_or_exponent() {
        for (value, expected) in [
            (0.0, "0"),
            (-0.0, "0"),
            (50.0, "50"),
            (0.3, "0.3"),
            (-2.5, "-2.5"),
            (100.0, "100"),
            (0.000001, "0.000001"),
            (0.0000001, "1e-7"),
            (1e21, "1e+21"),
            (1e22, "1e+22"),
            (1e20, "100000000000000000000"),
        ] {
            assert_eq!(number_to_string(value), expected, "{value}");
        }
    }
}
