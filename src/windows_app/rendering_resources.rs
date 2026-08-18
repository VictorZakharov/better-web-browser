//! Native UI fonts, decoded-image/glyph bitmaps, and trusted local-layout measurement.

mod glyphs;

use super::paint_primitives::bitmap_info;
use super::*;

pub(super) use glyphs::GlyphBitmaps;

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

#[derive(Default)]
pub(super) struct ImageBitmaps {
    bitmaps: HashMap<String, Hbitmap>,
}

impl ImageBitmaps {
    pub(super) unsafe fn get_or_create(
        &mut self,
        key: &str,
        image: &DecodedImage,
        dc: Hdc,
    ) -> Hbitmap {
        if let Some(bitmap) = self.bitmaps.get(key) {
            return *bitmap;
        }
        let info = bitmap_info(image);
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if !bitmap.is_null() && !pixels.is_null() {
            std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), pixels.cast(), image.bgra.len());
            self.bitmaps.insert(key.to_string(), bitmap);
        }
        bitmap
    }

    pub(super) unsafe fn get_or_create_tinted(
        &mut self,
        key: &str,
        image: &DecodedImage,
        tint: [u8; 4],
        dc: Hdc,
    ) -> Hbitmap {
        let cache_key = format!(
            "{key}#tint:{:02x}{:02x}{:02x}{:02x}",
            tint[0], tint[1], tint[2], tint[3]
        );
        if let Some(bitmap) = self.bitmaps.get(&cache_key) {
            return *bitmap;
        }

        let mut tinted = Vec::with_capacity(image.bgra.len());
        for pixel in image.bgra.chunks_exact(4) {
            let alpha = u16::from(pixel[3]) * u16::from(tint[3]) / 255;
            tinted.push((u16::from(tint[2]) * alpha / 255) as u8);
            tinted.push((u16::from(tint[1]) * alpha / 255) as u8);
            tinted.push((u16::from(tint[0]) * alpha / 255) as u8);
            tinted.push(alpha as u8);
        }

        let info = bitmap_info(image);
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if !bitmap.is_null() && !pixels.is_null() {
            std::ptr::copy_nonoverlapping(tinted.as_ptr(), pixels.cast(), tinted.len());
            self.bitmaps.insert(cache_key, bitmap);
        }
        bitmap
    }

    pub(super) unsafe fn clear(&mut self) {
        for bitmap in self.bitmaps.drain().map(|(_, bitmap)| bitmap) {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
        }
    }
}

impl Drop for ImageBitmaps {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}

pub(super) struct GdiTextMeasurer<'a> {
    pub(super) dc: Hdc,
    pub(super) fonts: &'a mut DynamicFonts,
    pub(super) dpi: u32,
    pub(super) calls: usize,
}

impl TextMeasurer for GdiTextMeasurer<'_> {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        unsafe {
            let handle = self.fonts.get_or_create(font, self.dpi);
            SelectObject(self.dc, handle);
            let size = measure_text(self.dc, text);
            self.calls += 1;
            let scale = dpi_scale(self.dpi);
            (size.cx as f32 / scale, size.cy as f32 / scale)
        }
    }
}
