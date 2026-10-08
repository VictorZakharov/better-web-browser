//! Time admission for animation / transition lists and their shorthands.
//! CSS Values 4 §§7.2, 10.12: zero still needs a time unit; calculated values
//! clamp after evaluation, whereas a negative literal duration is invalid.
use super::*;

pub(in crate::engine::css) fn seconds(value: &str, allow_negative: bool) -> Option<f32> {
    if value.len() > 16_384 {
        return None;
    }
    if let Some(seconds) = math::seconds(value) {
        return Some(if allow_negative {
            seconds
        } else {
            seconds.max(0.0)
        });
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Dimension { value, unit, .. } = input.next().ok()?.clone() else {
        return None;
    };
    input.expect_exhausted().ok()?;
    let seconds = match unit.to_ascii_lowercase().as_str() {
        "s" => value,
        "ms" => value * 0.001,
        _ => return None,
    };
    (seconds.is_finite() && (allow_negative || seconds >= 0.0)).then_some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_units_are_css_tokens_and_canonical_seconds() {
        for (source, expected) in [
            ("1s", 1.0),
            ("250ms", 0.25),
            ("1S", 1.0),
            ("250MS", 0.25),
            ("1e3ms", 1.0),
            ("/* leading */ 0s /**/", 0.0),
            ("1\\73", 1.0),
            ("-50ms", -0.05),
        ] {
            let actual = seconds(source, true).unwrap_or_else(|| panic!("{source}"));
            assert!((actual - expected).abs() < 0.00001, "{source}: {actual}");
        }
        for source in ["-1s", "-50ms"] {
            assert_eq!(seconds(source, false), None, "{source}");
        }
        for source in [
            "0",
            "1 s",
            "1/**/s",
            "1s 2s",
            "NaNs",
            "infs",
            "infinitys",
            "1px",
            "1deg",
            "1%",
            "1hz",
            "1s!important",
            "1e1000s",
        ] {
            assert_eq!(seconds(source, true), None, "{source}");
        }
    }

    #[test]
    fn time_functions_share_type_checks_and_property_specific_ranges() {
        for (source, expected) in [
            ("calc(1s + 250ms)", 1.25),
            ("min(1s, 750ms)", 0.75),
            ("max(1s, 750ms)", 1.0),
            ("clamp(100ms, 1s, 750ms)", 0.75),
            ("round(up, 250ms, .1s)", 0.3),
            ("mod(-250ms, .1s)", 0.05),
            ("rem(-250ms, .1s)", -0.05),
            ("hypot(3s, 4000ms)", 5.0),
            ("abs(-1s)", 1.0),
            ("calc(sign(-1s) * 500ms)", -0.5),
            ("calc(pow(2s / 500ms, 2) * 100ms)", 1.6),
            ("calc(sin(90deg) * 1s)", 1.0),
            ("calc(sqrt(-1) * 1s)", 0.0),
        ] {
            let actual = seconds(source, true).unwrap_or_else(|| panic!("{source}"));
            assert!((actual - expected).abs() < 0.00001, "{source}: {actual}");
            assert_eq!(seconds(source, false), Some(actual.max(0.0)), "{source}");
        }
        assert_eq!(seconds("calc(-1s)", true), Some(-1.0));
        assert_eq!(seconds("calc(-1s)", false), Some(0.0));
        assert_eq!(seconds("calc(infinity * 1s)", true), Some(f32::MAX));
        assert_eq!(seconds("calc(-infinity * 1s)", false), Some(0.0));
        for source in [
            "calc(1)",
            "min(0)",
            "min(1s,0)",
            "calc(1s + 1px)",
            "calc(1s + 1%)",
            "calc(1s + 1deg)",
            "calc(sign(1em) * 1s)",
            "calc(1em / 1px * 1s)",
            "calc(sign(1vw) * 1s)",
            "calc(sign(1%) * 1s)",
            "calc(1s * 1s)",
            "calc(1 / 1s)",
            "sin(1s)",
            "cos(1s)",
            "tan(1s)",
            "sqrt(1s)",
            "pow(1s,2)",
            "round(1s)",
            "round(1s,1)",
            "min(1s,) junk",
            "calc(1s+ 1s)",
            "calc(1s +1s)",
        ] {
            assert_eq!(seconds(source, true), None, "{source}");
        }
    }

    #[test]
    fn signed_zero_and_nan_stay_inside_the_containing_calculation() {
        for (source, expected) in [
            ("calc(sign(1s / -0s) * 1s)", -1.0),
            ("calc(sign(mod(1s,-1s)) * 1s)", 0.0),
            ("calc(sign(1s / mod(1s,-1s)) * 1s)", -1.0),
            ("calc(sign(1s / rem(-1s,1s)) * 1s)", -1.0),
            ("calc(sign(1s / abs(-0s)) * 1s)", 1.0),
            ("calc(NaN * 1s + 2s)", 0.0),
        ] {
            assert_eq!(seconds(source, true), Some(expected), "{source}");
        }
    }
}
