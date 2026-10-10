//! CSS Backgrounds 3 §6.1: ordered, non-inherited box shadows.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub lengths: [Length; 4],
    pub color: Option<Color>,
    pub inset: bool,
}

pub(in crate::engine::css) fn parse(value: &str) -> Option<Vec<BoxShadow>> {
    if value.trim().eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    super::super::shorthands::background::list(value, shadow)
}

fn shadow(value: &str) -> Option<BoxShadow> {
    let parts = borrowed_components(value)?;
    let (mut lengths, mut color, mut inset) = (Vec::new(), None, false);
    let mut ended_lengths = false;
    for part in parts {
        if let Some(length) =
            parse_length(part).filter(|length| *length != Length::Auto && !length.has_percentage())
        {
            // The 2–4 lengths are one contiguous grammar component.
            if ended_lengths || lengths.len() == 4 {
                return None;
            }
            lengths.push(length);
        } else {
            ended_lengths |= !lengths.is_empty();
            if ident(part).as_deref() == Some("inset") {
                if inset {
                    return None;
                }
                inset = true;
            } else {
                let parsed = if ident(part).as_deref() == Some("currentcolor") {
                    None
                } else {
                    Some(parse_color(part)?)
                };
                if color.replace(parsed).is_some() {
                    return None;
                }
            }
        }
    }
    if !(2..=4).contains(&lengths.len()) {
        return None;
    }
    if lengths.get(2).is_some_and(|length| {
        // Mathematical negatives clamp at computed-value time; literal ones are invalid.
        !matches!(length, Length::Calc { .. } | Length::Math(_))
            && length.resolve(0.0, 16.0).is_none_or(|value| value < 0.0)
    }) {
        return None;
    }
    while lengths.len() < 4 {
        lengths.push(Length::Px(0.0))
    }
    Some(BoxShadow {
        lengths: lengths.try_into().ok()?,
        color: color.unwrap_or(None),
        inset,
    })
}

impl BoxShadow {
    pub(crate) fn map_lengths(&mut self, map: impl Fn(&Length) -> Length) {
        self.lengths = self.lengths.each_ref().map(map);
    }
    pub(in crate::engine::css) fn css_text(&self, style: &ComputedStyle) -> String {
        let lengths = self
            .lengths
            .iter()
            .enumerate()
            .map(|(index, length)| {
                let value = length.resolve(0.0, style.font_size).unwrap_or(0.0);
                format!("{}px", if index == 2 { value.max(0.0) } else { value })
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "{} {lengths}{}",
            super::super::cssom::serialize_color(self.color.unwrap_or(style.color)),
            if self.inset { " inset" } else { "" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_lists_are_atomic_and_lengths_are_contiguous() {
        for value in [
            "0 0",
            "inset red 1em 2px 3px -1px",
            "0 0 4px currentColor inset,blue 0 1px",
        ] {
            assert!(parse(value).is_some(), "{value}");
        }
        for value in [
            "0",
            "0 0 -1px",
            "0 0 0%",
            "0 0 red 2px",
            "0 0 inset inset",
            "red blue 0 0",
            "none,0 0",
            "0 0,",
        ] {
            assert!(parse(value).is_none(), "{value}");
        }
    }
}
