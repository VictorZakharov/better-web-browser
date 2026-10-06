//! CSS Fonts unicode-range is a coverage restriction, not a font-file rewrite.
//! Reuse cssparser's UnicodeRange grammar (existing MPL-2.0 dependency) and keep
//! an immutable, bounded descriptor shared by snapshots and native selection.
//! https://drafts.csswg.org/css-fonts-4/#descdef-font-face-unicode-range
use cssparser::{Parser, ParserInput, ToCss, UnicodeRange};
use std::sync::Arc;

const MAX_SOURCE_BYTES: usize = 4096;
const MAX_RANGES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeRanges(Arc<[UnicodeRange]>);

impl Default for UnicodeRanges {
    fn default() -> Self {
        Self(
            vec![UnicodeRange {
                start: 0,
                end: 0x10ffff,
            }]
            .into(),
        )
    }
}

impl UnicodeRanges {
    pub(crate) fn parse(source: &str) -> Option<Self> {
        if source.len() > MAX_SOURCE_BYTES {
            return None;
        }
        let mut input = ParserInput::new(source);
        let mut parser = Parser::new(&mut input);
        let mut ranges = Vec::new();
        loop {
            if ranges.len() == MAX_RANGES {
                return None;
            }
            ranges.push(UnicodeRange::parse(&mut parser).ok()?);
            if parser.is_exhausted() {
                break;
            }
            parser.expect_comma().ok()?;
        }
        Some(Self(ranges.into()))
    }

    pub(crate) fn serialize(&self) -> String {
        self.0
            .iter()
            .map(ToCss::to_css_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn contains(&self, codepoint: u32) -> bool {
        self.0
            .iter()
            .any(|range| (range.start..=range.end).contains(&codepoint))
    }

    pub(crate) fn intersects(&self, text: &str) -> bool {
        text.chars()
            .any(|character| self.contains(character as u32))
    }
}

#[cfg(test)]
mod tests;
