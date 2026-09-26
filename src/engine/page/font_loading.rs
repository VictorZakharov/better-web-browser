//! Installation of document-scoped web fonts.

use super::Page;
use crate::engine::font::{WebFontFace, decode_web_font};

impl Page {
    pub fn add_font(
        &mut self,
        url: String,
        family: String,
        weight: u16,
        italic: bool,
        bytes: &[u8],
    ) -> Result<(), String> {
        if self.fonts.iter().any(|font| {
            font.family.eq_ignore_ascii_case(&family)
                && font.weight == weight
                && font.italic == italic
        }) {
            return Ok(());
        }
        let face = WebFontFace {
            family,
            weight,
            weight_min: f32::from(weight),
            weight_max: f32::from(weight),
            italic,
            url,
        };
        self.fonts.push(decode_web_font(&face, bytes)?);
        Ok(())
    }
}
