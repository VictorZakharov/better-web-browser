//! CSS 2 physical float sides and clearance (not inherited).
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
}

impl Clear {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "left" => Self::Left,
            "right" => Self::Right,
            "both" => Self::Both,
            _ => return None,
        })
    }
    pub(crate) fn css_keyword(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Left => "left",
            Self::Right => "right",
            Self::Both => "both",
        }
    }
    pub(crate) fn includes(self, side: Float) -> bool {
        matches!(
            (self, side),
            (Self::Both, Float::Left | Float::Right)
                | (Self::Left, Float::Left)
                | (Self::Right, Float::Right)
        )
    }
}

impl super::ComputedStyle {
    pub(crate) fn blockify_float(&mut self) {
        if self.float != Float::None {
            self.display = match self.display {
                super::Display::Inline | super::Display::InlineBlock => super::Display::Block,
                super::Display::InlineFlex => super::Display::Flex,
                super::Display::InlineTable => super::Display::Table,
                display => display,
            };
        }
    }
}
