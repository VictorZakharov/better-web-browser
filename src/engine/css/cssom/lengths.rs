use super::{Length, serialize_number, serialize_px};

pub(super) fn serialize_computed_length(value: Length, font_size: f32) -> String {
    let value = match value {
        Length::Math(value) => value
            .map_lengths(&|length| match length {
                Length::Em(value) => Length::Px(value * font_size),
                value => value.clone(),
            })
            .into_length(),
        Length::Em(em) => Length::Px(em * font_size),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } if em != 0.0 => {
            if percent == 0.0 && rem == 0.0 && vw == 0.0 && vh == 0.0 && vmin == 0.0 && vmax == 0.0
            {
                Length::Px(px + em * font_size)
            } else {
                Length::Calc {
                    px: px + em * font_size,
                    percent,
                    em: 0.0,
                    rem,
                    vw,
                    vh,
                    vmin,
                    vmax,
                }
            }
        }
        value => value,
    };
    serialize_length(value)
}

pub(in crate::engine::css) fn serialize_length(value: Length) -> String {
    match value {
        Length::Auto => "auto".to_string(),
        Length::Math(value) => value.css_text(),
        Length::Px(value) => serialize_px(value),
        Length::Percent(value) => format!("{}%", serialize_number(value)),
        Length::Em(value) => format!("{}em", serialize_number(value)),
        Length::Rem(value) => format!("{}rem", serialize_number(value)),
        Length::Vw(value) => format!("{}vw", serialize_number(value)),
        Length::Vh(value) => format!("{}vh", serialize_number(value)),
        Length::Vmin(value) => format!("{}vmin", serialize_number(value)),
        Length::Vmax(value) => format!("{}vmax", serialize_number(value)),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            let mut expression = String::new();
            for (term, unit) in [
                (px, "px"),
                (percent, "%"),
                (em, "em"),
                (rem, "rem"),
                (vw, "vw"),
                (vh, "vh"),
                (vmin, "vmin"),
                (vmax, "vmax"),
            ] {
                if term == 0.0 {
                    continue;
                }
                if expression.is_empty() {
                    expression.push_str(&serialize_number(term));
                } else {
                    expression.push_str(if term < 0.0 { " - " } else { " + " });
                    expression.push_str(&serialize_number(term.abs()));
                }
                expression.push_str(unit);
            }
            if expression.is_empty() {
                "0px".to_string()
            } else {
                format!("calc({expression})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn mixed_calc_resolves_font_units_but_keeps_unresolved_percentage_terms() {
        let mut style = ComputedStyle::initial();
        style.width = Length::Calc {
            px: 12.0,
            percent: 25.0,
            em: -2.0,
            rem: 0.0,
            vw: 1.5,
            vh: 0.0,
            vmin: 0.0,
            vmax: 0.0,
        };
        assert_eq!(
            resolved_property_value(&style, "width").as_deref(),
            Some("calc(-20px + 25% + 1.5vw)")
        );
        assert_eq!(serialize_length(Length::Rem(1.25)), "1.25rem");
    }
}
