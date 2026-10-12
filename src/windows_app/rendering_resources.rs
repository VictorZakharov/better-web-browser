//! Native UI fonts and decoded-image/glyph bitmap caches.

mod glyphs;
mod images;

use super::*;

pub(super) use glyphs::GlyphBitmaps;
pub(super) use images::{ImageBitmaps, Reflection};

#[derive(Clone, Copy)]
pub(super) enum FontKind {
    Body,
    Small,
    Heading1,
    Heading2,
    Heading3,
    Mono,
}

pub(super) struct Fonts {
    pub(super) ui: Hfont,
    pub(super) ui_semibold: Hfont,
    pub(super) ui_small: Hfont,
    pub(super) body: Hfont,
    pub(super) small: Hfont,
    pub(super) heading1: Hfont,
    pub(super) heading2: Hfont,
    pub(super) heading3: Hfont,
    pub(super) mono: Hfont,
}

impl Fonts {
    pub(super) unsafe fn create(dpi: u32) -> Result<Self, String> {
        let fonts = Self {
            ui: create_font(scaled_font_height(-16, dpi), 400, false, "Segoe UI"),
            ui_semibold: create_font(scaled_font_height(-16, dpi), 600, false, "Segoe UI"),
            ui_small: create_font(scaled_font_height(-14, dpi), 400, false, "Segoe UI"),
            body: create_font(scaled_font_height(-19, dpi), 400, false, "Segoe UI"),
            small: create_font(scaled_font_height(-16, dpi), 400, false, "Segoe UI"),
            heading1: create_font(scaled_font_height(-34, dpi), 600, false, "Segoe UI"),
            heading2: create_font(scaled_font_height(-28, dpi), 600, false, "Segoe UI"),
            heading3: create_font(scaled_font_height(-23, dpi), 600, false, "Segoe UI"),
            mono: create_font(scaled_font_height(-18, dpi), 400, false, "Cascadia Mono"),
        };
        if [
            fonts.ui,
            fonts.ui_semibold,
            fonts.ui_small,
            fonts.body,
            fonts.small,
            fonts.heading1,
            fonts.heading2,
            fonts.heading3,
            fonts.mono,
        ]
        .iter()
        .any(|font| font.is_null())
        {
            Err(last_error("create interface fonts"))
        } else {
            Ok(fonts)
        }
    }

    pub(super) fn get(&self, kind: FontKind) -> Hfont {
        match kind {
            FontKind::Body => self.body,
            FontKind::Small => self.small,
            FontKind::Heading1 => self.heading1,
            FontKind::Heading2 => self.heading2,
            FontKind::Heading3 => self.heading3,
            FontKind::Mono => self.mono,
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [
                self.ui,
                self.ui_semibold,
                self.ui_small,
                self.body,
                self.small,
                self.heading1,
                self.heading2,
                self.heading3,
                self.mono,
            ] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    family: String,
    size: i32,
    weight: u16,
    italic: bool,
    underline: bool,
}

#[derive(Default)]
pub(super) struct DynamicFonts {
    fonts: HashMap<FontKey, Hfont>,
}

impl DynamicFonts {
    pub(super) unsafe fn get_or_create(&mut self, spec: &FontSpec, dpi: u32) -> Hfont {
        let key = font_key(spec, dpi);
        if let Some(font) = self.fonts.get(&key) {
            return *font;
        }
        let family = wide(&key.family);
        let font = CreateFontW(
            -key.size,
            0,
            0,
            0,
            i32::from(key.weight),
            key.italic as u32,
            key.underline as u32,
            0,
            1,
            0,
            0,
            5,
            0,
            family.as_ptr(),
        );
        self.fonts.insert(key, font);
        font
    }

    pub(super) unsafe fn clear(&mut self) {
        for font in self.fonts.drain().map(|(_, font)| font) {
            if !font.is_null() {
                DeleteObject(font);
            }
        }
    }
}

fn font_key(spec: &FontSpec, dpi: u32) -> FontKey {
    let requested = spec
        .family
        .split(',')
        .next()
        .unwrap_or("sans-serif")
        .trim()
        .trim_matches(['\'', '"']);
    let family = match requested.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "ui-sans-serif" => "Arial".to_string(),
        "serif" | "ui-serif" => "Times New Roman".to_string(),
        "monospace" | "ui-monospace" => "Consolas".to_string(),
        _ => requested.to_string(),
    };
    FontKey {
        family,
        size: (spec.size * dpi_scale(dpi)).round().clamp(1.0, 768.0) as i32,
        weight: spec.weight.clamp(100, 900),
        italic: spec.italic,
        underline: spec.underline,
    }
}

impl Drop for DynamicFonts {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}
