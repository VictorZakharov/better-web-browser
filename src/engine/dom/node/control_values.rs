//! Input state classification, value sanitization, and number formatting.
//!
//! Pure string functions shared by control state, validity, and numeric APIs.

use super::Node;
use super::control_temporal::{is_temporal_state, parse_temporal};
use super::control_validity;

mod color;

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
    if text_like_type(state)
        || matches!(
            state,
            "date" | "month" | "week" | "time" | "datetime-local" | "number" | "range" | "color"
        )
    {
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
    // Email and URL strip ASCII whitespace after newlines; they precede the
    // text-like branch (`text_like_type` covers them) which only strips
    // newlines.
    if state == "email" || state == "url" {
        return strip_newlines(value)
            .trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'))
            .to_string();
    }
    if text_like_type(state) || state == "password" {
        return strip_newlines(value);
    }
    if state == "number" {
        return if is_strict_float(value) {
            value.to_string()
        } else {
            String::new()
        };
    }
    if state == "range" {
        if !is_strict_float(value) {
            return String::new();
        }
        return value.to_string();
    }
    if state == "color" {
        return color::sanitize(value);
    }
    if is_temporal_state(state) {
        // Out-of-grammar temporal values sanitize to empty (required
        // controls then suffer valueMissing instead of keeping garbage).
        return if parse_temporal(state, value).is_some() {
            value.to_string()
        } else {
            String::new()
        };
    }
    value.to_string()
}

/// Strict float validity: no surrounding (or interior) ASCII whitespace.
/// The lenient parser skips whitespace, but sanitization keeps the value
/// only when it is already a valid floating-point number.
pub(crate) fn is_strict_float(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('+')
        && !value.ends_with('.')
        && !value.contains(".e")
        && !value.contains(".E")
        && !value
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\x0C' | b'\r'))
        && is_valid_float(value)
}

pub(crate) fn strip_newlines(value: &str) -> String {
    value.replace("\r\n", "").replace(['\r', '\n'], "")
}

/// Parses non-negative integers for length/size attributes.
pub(crate) fn parse_non_negative(value: &str) -> Option<u64> {
    let trimmed = value.trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'));
    if trimmed.is_empty()
        || trimmed.starts_with(['+', '-'])
        || !trimmed.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    trimmed.parse().ok()
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

/// Warms the stored pattern verdict from the current inputs using the
/// process pattern engine (its own isolate, Unicode-sets semantics).
/// Already-warm, inapplicable, empty, and pattern-less inputs are cheap
/// no-ops; the store itself is idempotent. Returns true when the stored
/// verdict changed, so callers invalidate exactly then.
///
/// Never evaluates while a page isolate is entered on this thread (it
/// returns false there); scripted writes refresh their verdict on their own
/// realm instead (`refreshPatternVerdict` in `forms_validity.js`), and
/// validity reads evaluate on the calling realm (`controlValidation`).
pub(crate) fn refresh_pattern_verdict(node: &Node) -> bool {
    if crate::engine::pattern_eval::page_isolate_entered() {
        return false;
    }
    if node.tag_name() != Some("input") {
        return false;
    }
    let state = node.input_state_name();
    // Same applicability as the validity algorithm's pattern branch: an
    // empty value never mismatches, so it needs no verdict.
    if !matches!(
        state.as_str(),
        "text" | "search" | "tel" | "url" | "email" | "password"
    ) {
        return false;
    }
    let value = node.input_value();
    if value.is_empty() {
        return false;
    }
    let Some((pattern, values)) = control_validity::current_pattern_inputs(node, &state, &value)
    else {
        return false;
    };
    if control_validity::cached_pattern_verdict(node, &pattern, &values).is_some() {
        return false;
    }
    let verdict = values.iter().all(|candidate| {
        crate::engine::pattern_eval::test_pattern(&pattern, candidate).unwrap_or(true)
    });
    control_validity::store_pattern_verdict(node, &pattern, values, verdict)
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
