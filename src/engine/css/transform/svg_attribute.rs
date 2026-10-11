//! SVG's legacy attribute grammar is not the CSS transform-function grammar.
//! Reuse the locked SVG lexer, then share CSS cascade and animation semantics.
//! https://www.w3.org/TR/css-transforms-1/#svg-transform-functions
use super::parser::parse_transform;
use svgtypes::{TransformListParser, TransformListToken as Token};

pub(in crate::engine::css) fn css_value(value: &str) -> Option<String> {
    if value.len() > 16_384 {
        return None;
    }
    let tokens = TransformListParser::from(value)
        .take(65)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if tokens.len() > 64 {
        return None;
    }
    if tokens.is_empty() {
        return Some("none".into());
    }
    // A three-value rotate is the spec's exceptional matrix-interpolation
    // case. Number syntax has already been validated by the shared lexer.
    let pivot = value.split(')').any(|group| {
        let group = group.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        group.split_once('(').is_some_and(|(name, _)| {
            name.trim_matches(|c: char| c.is_ascii_whitespace() || c == ',') == "rotate"
                && TransformListParser::from(format!("{group})").as_str()).count() == 3
        })
    });
    let normalized = if pivot {
        let m: svgtypes::Transform = value.parse().ok()?;
        format!("matrix({},{},{},{},{},{})", m.a, m.b, m.c, m.d, m.e, m.f)
    } else {
        tokens
            .into_iter()
            .map(|token| match token {
                Token::Matrix { a, b, c, d, e, f } => format!("matrix({a},{b},{c},{d},{e},{f})"),
                Token::Translate { tx, ty } => format!("translate({tx}px,{ty}px)"),
                Token::Scale { sx, sy } => format!("scale({sx},{sy})"),
                Token::Rotate { angle } => format!("rotate({angle}deg)"),
                Token::SkewX { angle } => format!("skewX({angle}deg)"),
                Token::SkewY { angle } => format!("skewY({angle}deg)"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    parse_transform(&normalized).map(|_| normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_keep_legacy_grammar_and_unwrapped_single_angle_interpolation() {
        assert_eq!(
            css_value("translate (10 20) rotate(720)").unwrap(),
            "translate(10px,20px) rotate(720deg)"
        );
        let angle = super::super::interpolate(
            &css_value("rotate(0)").unwrap(),
            &css_value("rotate(720)").unwrap(),
            0.25,
        )
        .unwrap();
        let (x, y) = parse_transform(&angle)
            .unwrap()
            .matrix(0.0, 0.0, 0.0)
            .point(1.0, 0.0);
        assert!((x + 1.0).abs() < 1e-5 && y.abs() < 1e-5, "{angle}");
        for invalid in [
            "translate(10px)",
            "scaleX(.5)",
            "rotate(20deg)",
            "none",
            "inherit",
            "scale(1) garbage",
            "matrix(1e999,0,0,1,0,0)",
        ] {
            assert!(css_value(invalid).is_none(), "{invalid}");
        }
        assert_eq!(css_value("  ").unwrap(), "none");
        assert!(css_value(&"scale(1) ".repeat(65)).is_none());
    }

    #[test]
    fn three_value_rotate_uses_the_specified_matrix_interpolation_path() {
        let value = css_value("translate(3,4), rotate(90,10,20)").unwrap();
        assert!(value.starts_with("matrix("));
        let matrix = parse_transform(&value).unwrap().matrix(0.0, 0.0, 0.0);
        let (x, y) = matrix.point(10.0, 20.0);
        assert!((x - 13.0).abs() < 1e-5 && (y - 24.0).abs() < 1e-5);
    }
}
