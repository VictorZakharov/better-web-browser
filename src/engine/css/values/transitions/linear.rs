//! Validate CSS Easing Level 2 linear() stops using the existing CSS tokenizer.
//! Missing/descending percentages are valid and normalized by the shared sampler.
//! https://drafts.csswg.org/css-easing-2/#linear-easing-function
use crate::engine::css::{Parser, ParserInput, Token, split_css_top_level};

pub(super) fn valid(value: &str) -> bool {
    let Some(body) = value
        .strip_prefix("linear(")
        .and_then(|v| v.strip_suffix(')'))
    else {
        return false;
    };
    let mut count = 0;
    for stop in split_css_top_level(body, ',') {
        count += 1;
        if count > 64 {
            return false;
        }
        let mut input = ParserInput::new(stop);
        let mut parser = Parser::new(&mut input);
        let mut output = None;
        let mut positions = 0;
        while !parser.is_exhausted() {
            match parser.next() {
                Ok(Token::Number { value, .. }) if value.is_finite() && output.is_none() => {
                    output = Some(*value)
                }
                Ok(Token::Percentage { unit_value, .. })
                    if unit_value.is_finite() && positions < 2 =>
                {
                    positions += 1
                }
                _ => return false,
            }
        }
        if output.is_none() {
            return false;
        }
    }
    count >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_distributed_doubled_descending_and_extrapolated_stops() {
        for source in [
            "linear(0,1)",
            "linear(0, .5, 1)",
            "linear(0 0% 20%, 1 80% 100%)",
            "linear(0 -10%, 1 110%)",
            "linear(0 75%, .5 25%, 1)",
            "linear(-1, 2)",
        ] {
            assert!(valid(source), "{source}");
        }
    }
    #[test]
    fn rejects_missing_outputs_extra_components_and_unbounded_stop_lists() {
        for source in [
            "linear()",
            "linear(0)",
            "linear(0,)",
            "linear(0, 10%)",
            "linear(0 1, 1)",
            "linear(0 0% 20% 40%,1)",
            "linear(0px,1)",
            "linear(nan,1)",
        ] {
            assert!(!valid(source), "{source}");
        }
        assert!(!valid(
            &("linear(".to_owned()
                + &std::iter::repeat_n("0", 65).collect::<Vec<_>>().join(",")
                + ")")
        ));
    }
}
