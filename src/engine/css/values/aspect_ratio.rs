//! CSS Sizing 4 preferred aspect-ratio value and canonical serialization.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AspectRatio {
    Auto,
    Ratio {
        width: f32,
        height: f32,
        prefer_natural: bool,
    },
}

impl AspectRatio {
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.eq_ignore_ascii_case("auto") {
            return Some(Self::Auto);
        }
        let parts = input.split_ascii_whitespace().collect::<Vec<_>>();
        let (prefer_natural, ratio) = match parts.as_slice() {
            ["auto", rest @ ..] => (true, rest.join(" ")),
            [rest @ .., "auto"] => (true, rest.join(" ")),
            _ => (false, input.to_string()),
        };
        if ratio.is_empty() || ratio.split_ascii_whitespace().any(|part| part == "auto") {
            return None;
        }
        let values = ratio.split('/').map(str::trim).collect::<Vec<_>>();
        let [width, height] = values.as_slice() else {
            if values.len() == 1 {
                let width = parse_nonnegative_number(values[0])?;
                return Some(Self::Ratio {
                    width,
                    height: 1.0,
                    prefer_natural,
                });
            }
            return None;
        };
        let width = parse_nonnegative_number(width)?;
        let height = parse_nonnegative_number(height)?;
        Some(Self::Ratio {
            width,
            height,
            prefer_natural,
        })
    }

    pub fn preferred(self, natural: Option<(f32, f32)>) -> Option<(f32, bool)> {
        let natural = natural
            .and_then(|(width, height)| (width > 0.0 && height > 0.0).then_some(width / height));
        match self {
            Self::Auto => natural.map(|ratio| (ratio, false)),
            Self::Ratio {
                width,
                height,
                prefer_natural,
            } => {
                if height <= 0.0 || width <= 0.0 {
                    return natural.map(|ratio| (ratio, false));
                }
                if prefer_natural && let Some(natural) = natural {
                    Some((natural, false))
                } else {
                    Some((width / height, !prefer_natural))
                }
            }
        }
    }

    pub fn css_text(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Ratio {
                width,
                height,
                prefer_natural,
            } => format!(
                "{}{width} / {height}",
                if prefer_natural { "auto " } else { "" }
            ),
        }
    }
}

fn parse_nonnegative_number(input: &str) -> Option<f32> {
    if input.is_empty() || input.chars().any(char::is_whitespace) {
        return None;
    }
    let value = input.parse::<f32>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}
