//! CSS Text 3 case transformations supported by the text layout pipeline.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextTransform {
    #[default]
    None,
    Capitalize,
    Uppercase,
    Lowercase,
}

impl TextTransform {
    pub fn parse(input: &str) -> Option<Self> {
        if input.eq_ignore_ascii_case("none") {
            Some(Self::None)
        } else if input.eq_ignore_ascii_case("capitalize") {
            Some(Self::Capitalize)
        } else if input.eq_ignore_ascii_case("uppercase") {
            Some(Self::Uppercase)
        } else if input.eq_ignore_ascii_case("lowercase") {
            Some(Self::Lowercase)
        } else {
            None
        }
    }

    pub fn css_text(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Capitalize => "capitalize",
            Self::Uppercase => "uppercase",
            Self::Lowercase => "lowercase",
        }
    }
}
