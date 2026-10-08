//! Shared number / integer admission. CSS numbers are tokens, not Rust strings
//! (Rust's `NaN` / `inf` and separated signs must not become property values).
use super::*;

pub(in crate::engine::css) fn number(value: &str) -> Option<f32> {
    math::number(value).or_else(|| literal(value))
}

pub(in crate::engine::css) fn percentage(value: &str) -> Option<f32> {
    if let Some(value) = math::percentage_points(value) {
        return Some(value);
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Percentage { unit_value, .. } = input.next().ok()?.clone() else {
        return None;
    };
    input.expect_exhausted().ok()?;
    unit_value.is_finite().then_some(unit_value * 100.0)
}

pub(in crate::engine::css) fn nonnegative(value: &str) -> Option<f32> {
    if let Some(value) = math::number(value) {
        return Some(value.max(0.0));
    }
    let value = literal(value)?;
    (value >= 0.0).then_some(value)
}

pub(in crate::engine::css) fn integer(value: &str) -> Option<i32> {
    integer_with_minimum(value, i32::MIN)
}

pub(in crate::engine::css) fn positive_integer(value: &str) -> Option<i32> {
    integer_with_minimum(value, 1)
}

fn integer_with_minimum(value: &str, minimum: i32) -> Option<i32> {
    if let Some(value) = math::number_wide(value) {
        // Integer consumers must not pass through the f32 layout storage:
        // adjacent z-index values above 2^24 would collapse to the same layer.
        // CSS Values 4 §5.1: integer ties go toward positive infinity, not
        // Rust's away-from-zero round. Storage is the existing signed CSS layer.
        return Some(
            (value + 0.5)
                .floor()
                .clamp(f64::from(minimum), f64::from(i32::MAX)) as i32,
        );
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Number {
        int_value: Some(value),
        ..
    } = input.next().ok()?.clone()
    else {
        return None;
    };
    input.expect_exhausted().ok()?;
    (value >= minimum).then_some(value)
}

fn literal(value: &str) -> Option<f32> {
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Number { value, .. } = input.next().ok()?.clone() else {
        return None;
    };
    input.expect_exhausted().ok()?;
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn number_tokens_and_calculations_have_distinct_range_policy() {
        for (source, expected) in [
            (".5", 0.5),
            ("+2", 2.0),
            ("1e2", 100.0),
            ("0/**/", 0.0),
            ("calc(-2)", 0.0),
            ("pow(2,3)", 8.0),
            ("sqrt(-1)", 0.0),
        ] {
            assert_eq!(nonnegative(source), Some(expected), "{source}");
        }
        for source in [
            "-1",
            "NaN",
            "inf",
            "infinity",
            "1 2",
            "+ 1",
            "1px",
            "25%",
            "calc(2em)",
        ] {
            assert_eq!(nonnegative(source), None, "{source}");
        }
    }
    #[test]
    fn integer_calculations_use_positive_ties_and_bounded_storage() {
        for (source, expected) in [
            ("1", 1),
            ("-2", -2),
            ("+2", 2),
            ("1/**/", 1),
            ("calc(1.5)", 2),
            ("calc(-1.5)", -1),
            ("calc(-2.5)", -2),
            ("calc(16777217)", 16_777_217),
            ("calc(16777216 + 1)", 16_777_217),
            ("calc(2147483646)", 2_147_483_646),
            ("calc(-2147483647)", -2_147_483_647),
            ("calc(16777217.499)", 16_777_217),
            ("calc(16777217.5)", 16_777_218),
            ("calc(-16777217.5)", -16_777_217),
            ("calc(infinity)", i32::MAX),
            ("calc(-infinity)", i32::MIN),
            ("calc(NaN)", 0),
        ] {
            assert_eq!(integer(source), Some(expected), "{source}");
        }
        for source in ["1.5", "1e0", "1px", "1%", "auto", "NaN", "calc(2em)"] {
            assert_eq!(integer(source), None, "{source}");
        }
    }
}
