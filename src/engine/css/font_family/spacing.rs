//! Canvas spacing is a CSS <length>, not the page property's `normal` or percentage.
//! Retain specified font/viewport terms so changing the Canvas font changes em spacing.

use super::super::{Length, parse_length};
use cssparser::{Parser, ParserInput, Token};

pub(crate) fn parse(value: &str) -> Option<(String, [f32; 7])> {
    if value.len() > 1024 {
        return None;
    }
    let mut input = ParserInput::new(value);
    if !length_tokens(&mut Parser::new(&mut input)) {
        return None;
    }
    let length = parse_length(value)?;
    let mut terms = [0.0; 7];
    match length {
        Length::Px(value) => terms[0] = value,
        Length::Em(value) => terms[1] = value,
        Length::Rem(value) => terms[2] = value,
        Length::Vw(value) => terms[3] = value,
        Length::Vh(value) => terms[4] = value,
        Length::Vmin(value) => terms[5] = value,
        Length::Vmax(value) => terms[6] = value,
        Length::Calc {
            px,
            percent: 0.0,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            terms = [px, em, rem, vw, vh, vmin, vmax];
        }
        _ => return None,
    }
    if terms.iter().any(|term| !term.is_finite()) {
        return None;
    }
    Some((
        super::super::cssom::lengths::serialize_length(length),
        terms,
    ))
}

fn length_tokens(input: &mut Parser<'_, '_>) -> bool {
    while let Ok(token) = input.next().cloned() {
        let valid = match token {
            Token::Percentage { .. } | Token::Ident(_) => false,
            Token::Function(name) if name.eq_ignore_ascii_case("calc") => nested_lengths(input),
            Token::ParenthesisBlock => nested_lengths(input),
            Token::Function(_) => false,
            _ => true,
        };
        if !valid {
            return false;
        }
    }
    true
}

fn nested_lengths(input: &mut Parser<'_, '_>) -> bool {
    input
        .parse_nested_block(|nested| {
            if length_tokens(nested) {
                Ok(())
            } else {
                Err(nested.new_custom_error::<(), ()>(()))
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specified_spacing_keeps_units_and_rejects_percentage_even_after_cancellation() {
        for (source, serialized, terms) in [
            ("+2.50PX", "2.5px", [2.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            ("-0.5em", "-0.5em", [0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0]),
            ("1rem", "1rem", [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
            (
                "calc(1em + 2px)",
                "calc(2px + 1em)",
                [2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            ),
        ] {
            let actual = parse(source).unwrap();
            assert_eq!(actual, (serialized.into(), terms), "{source}");
        }
        for source in [
            "normal",
            "auto",
            "10%",
            "calc(10% - 10%)",
            "calc(2px + 1%)",
            "2 px",
            "2px!important",
            "2",
            "var(--space)",
            "NaNpx",
            "calc(1px / 0)",
        ] {
            assert!(parse(source).is_none(), "{source}");
        }
    }

    #[test]
    fn absolute_units_share_page_length_and_calc_conversion() {
        for (unit, value) in [
            ("in", 96.0),
            ("pc", 16.0),
            ("pt", 96.0 / 72.0),
            ("cm", 96.0 / 2.54),
            ("mm", 96.0 / 25.4),
            ("q", 96.0 / 101.6),
        ] {
            assert!((parse(&format!("1{unit}")).unwrap().1[0] - value).abs() < 0.00001);
            assert!(
                (parse(&format!("calc(1{unit} + 2px)")).unwrap().1[0] - value - 2.0).abs()
                    < 0.00001
            );
        }
    }
}
