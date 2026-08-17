//! Renderer-local GDI measurement and webfont registration.
//!
//! Win32k remains available to the AppContainer during this migration stage. Keeping these handles
//! in the renderer ensures malformed font data cannot fault the privileged browser process.

use crate::engine::{FontSpec, TextMeasurer, WebFont};
use std::collections::HashMap;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject,
    GetTextExtentPoint32W, HDC, HFONT, RemoveFontMemResourceEx, SelectObject,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FontKey {
    family: String,
    size: i32,
    weight: u16,
    italic: bool,
    underline: bool,
}

pub(super) struct RendererTextSystem {
    dc: HDC,
    dpi: u32,
    fonts: HashMap<FontKey, HFONT>,
    web_fonts: Vec<HANDLE>,
    pub(super) measure_calls: usize,
}

impl RendererTextSystem {
    pub(super) fn new(dpi: u32) -> Result<Self, String> {
        let dc = unsafe { CreateCompatibleDC(null_mut()) };
        if dc.is_null() {
            return Err(format!(
                "create renderer text device context: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self {
            dc,
            dpi,
            fonts: HashMap::new(),
            web_fonts: Vec::new(),
            measure_calls: 0,
        })
    }

    pub(super) fn set_dpi(&mut self, dpi: u32) {
        if self.dpi == dpi {
            return;
        }
        self.clear_fonts();
        self.dpi = dpi;
    }

    pub(super) fn register_web_fonts(&mut self, fonts: &[WebFont]) {
        self.clear_web_fonts();
        for font in fonts {
            let Ok(size) = u32::try_from(font.sfnt.len()) else {
                continue;
            };
            let mut count = 0_u32;
            let handle = unsafe {
                AddFontMemResourceEx(
                    font.sfnt.as_ptr().cast(),
                    size,
                    null(),
                    (&mut count as *mut u32).cast_const(),
                )
            };
            if !handle.is_null() && count > 0 {
                self.web_fonts.push(handle);
            }
        }
        self.clear_fonts();
    }

    fn font(&mut self, spec: &FontSpec) -> HFONT {
        let key = font_key(spec, self.dpi);
        if let Some(font) = self.fonts.get(&key) {
            return *font;
        }
        let family = wide(&key.family);
        let font = unsafe {
            CreateFontW(
                -key.size,
                0,
                0,
                0,
                i32::from(key.weight),
                key.italic.into(),
                key.underline.into(),
                0,
                1,
                0,
                0,
                5,
                0,
                family.as_ptr(),
            )
        };
        self.fonts.insert(key, font);
        font
    }

    fn clear_fonts(&mut self) {
        for font in self.fonts.drain().map(|(_, font)| font) {
            if !font.is_null() {
                unsafe { DeleteObject(font) };
            }
        }
    }

    fn clear_web_fonts(&mut self) {
        for font in self.web_fonts.drain(..) {
            if !font.is_null() {
                unsafe { RemoveFontMemResourceEx(font) };
            }
        }
    }
}

impl TextMeasurer for RendererTextSystem {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        let handle = self.font(font);
        unsafe { SelectObject(self.dc, handle) };
        let text = wide_without_nul(text);
        let mut size = windows_sys::Win32::Foundation::SIZE::default();
        let measured =
            unsafe { GetTextExtentPoint32W(self.dc, text.as_ptr(), text.len() as i32, &mut size) }
                != 0;
        self.measure_calls = self.measure_calls.saturating_add(1);
        let scale = self.dpi as f32 / 96.0;
        if measured {
            (size.cx as f32 / scale, size.cy as f32 / scale)
        } else {
            (
                (text.len() as f32 * font.size * 0.55).max(1.0),
                font.size * 1.2,
            )
        }
    }
}

impl Drop for RendererTextSystem {
    fn drop(&mut self) {
        self.clear_fonts();
        self.clear_web_fonts();
        if !self.dc.is_null() {
            unsafe { DeleteDC(self.dc) };
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
        size: (spec.size * dpi as f32 / 96.0).round().clamp(1.0, 768.0) as i32,
        weight: spec.weight.clamp(100, 900),
        italic: spec.italic,
        underline: spec.underline,
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn wide_without_nul(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
