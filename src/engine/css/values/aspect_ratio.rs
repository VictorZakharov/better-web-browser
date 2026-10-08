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
        super::scalars::RatioValue::parse(input)?.context_free()
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

#[cfg(test)]
mod tests;
