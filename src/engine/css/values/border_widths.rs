//! Border widths accept non-negative <length>, not <length-percentage>. Math
//! range clamping occurs at computed-value time; literal negatives are invalid.
//! https://www.w3.org/TR/css-backgrounds-3/#border-width
use super::*;

pub(in crate::engine::css) fn parse(value: &str) -> Option<Length> {
    let length = match value.trim().to_ascii_lowercase().as_str() {
        "thin" => Length::Px(1.0),
        "medium" => Length::Px(3.0),
        "thick" => Length::Px(5.0),
        _ => parse_length(value)?,
    };
    if length == Length::Auto || length.has_percentage() {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let is_calculation = matches!(parser.next(), Ok(Token::Function(_)));
    if !is_calculation
        && length
            .resolve(0.0, 16.0)
            .is_none_or(|value| !value.is_finite() || value < 0.0)
    {
        return None;
    }
    Some(length)
}

pub(in crate::engine::css) fn edges(value: &str) -> Option<Edges> {
    let parts = super::super::syntax::borrowed_components(value)?;
    if !(1..=4).contains(&parts.len()) {
        return None;
    }
    let lengths = parts.into_iter().map(parse).collect::<Option<Vec<_>>>()?;
    Some(super::super::scroll_spacing::expand_edges(&lengths))
}

pub(in crate::engine::css) fn apply(style: &mut ComputedStyle, property: &str, value: &str) {
    if property == "border-width" {
        if let Some(edges) = edges(value) {
            style.border_width = edges;
        }
        return;
    }
    let Some(length) = parse(value) else {
        return;
    };
    match property {
        "border-top-width" => style.border_width.top = length,
        "border-right-width" => style.border_width.right = length,
        "border-bottom-width" => style.border_width.bottom = length,
        "border-left-width" => style.border_width.left = length,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_only_type_checks_reject_zero_and_canceled_percentages() {
        for value in [
            "auto",
            "-1px",
            "0%",
            "min(0%)",
            "max(1px, 0%)",
            "calc(0%)",
            "calc(5% - 5%)",
            "clamp(1px, 5%, 10px)",
        ] {
            assert!(parse(value).is_none(), "accepted {value}");
        }
        for value in [
            "thin",
            "medium",
            "thick",
            "0",
            "2em",
            "min(1px, 2px)",
            "clamp(1px, 2px, 3px)",
            "calc(1px - 2px)",
        ] {
            assert!(parse(value).is_some(), "rejected {value}");
        }
    }

    #[test]
    fn malformed_edges_do_not_partially_override_prior_widths() {
        let mut style = ComputedStyle::initial();
        apply(&mut style, "border-width", "thin medium thick");
        let before = style.border_width.clone();
        for value in ["1px min(0%)", "1px auto", "1px 2px 3px 4px 5px"] {
            apply(&mut style, "border-width", value);
            assert_eq!(style.border_width, before);
        }
        assert_eq!(before.top, Length::Px(1.0));
        assert_eq!(before.right, Length::Px(3.0));
        assert_eq!(before.bottom, Length::Px(5.0));
        assert_eq!(before.left, Length::Px(3.0));
    }
}
