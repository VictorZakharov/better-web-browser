//! Installation of document-scoped web fonts.

use super::Page;
use crate::engine::font::{WebFontFace, decode_web_font};
mod fallback;

impl Page {
    pub fn add_font(
        &mut self,
        url: String,
        family: String,
        weight: u16,
        italic: bool,
        bytes: &[u8],
    ) -> Result<(), String> {
        let face = WebFontFace {
            family,
            weight,
            weight_min: f32::from(weight),
            weight_max: f32::from(weight),
            italic,
            url,
            fallback_urls: Vec::new(),
            unicode_range: "U+0-10FFFF".into(),
            features: Default::default(),
        };
        self.add_font_face(face, bytes)
    }

    pub(crate) fn add_font_face(&mut self, face: WebFontFace, bytes: &[u8]) -> Result<(), String> {
        let ranges = crate::engine::font::unicode_ranges::UnicodeRanges::parse(&face.unicode_range)
            .ok_or_else(|| "invalid font unicode-range descriptor".to_owned())?;
        if self.fonts.iter().any(|font| {
            font.family.eq_ignore_ascii_case(&face.family)
                && font.weight == face.weight
                && font.italic == face.italic
                && font.source_url == face.url
                && font.unicode_ranges == ranges
                && font.features == face.features
        }) {
            return Ok(());
        }
        self.fonts.push(decode_web_font(&face, bytes)?);
        Ok(())
    }
}
