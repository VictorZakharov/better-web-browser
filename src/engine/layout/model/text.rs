use super::*;
#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
    pub letter_spacing: f32,
    pub word_spacing: f32,
}

impl FontSpec {
    pub(in crate::engine::layout) fn from_style(style: &ComputedStyle) -> Self {
        Self {
            family: style.font_family.clone(),
            size: style.font_size,
            weight: style.font_weight,
            italic: style.italic,
            underline: style.text_decoration_underline,
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PositionedGlyph {
    pub raster_id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapedText {
    pub geometry: TextGeometry,
    pub width: f32,
    pub height: f32,
    pub raster_run_id: u64,
    pub glyphs: Vec<PositionedGlyph>,
}

pub trait TextMeasurer {
    /// Returns the same layout width and height as `shape` for this text and font.
    /// Geometry-only callers may use this method to avoid allocating painted glyph payloads;
    /// implementations must retain contextual shaping, fallback, and spacing in these metrics.
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32);

    /// Font/advance geometry with no raster allocation, in DOM UTF-16 source order.
    fn text_geometry(&mut self, text: &str, font: &FontSpec) -> TextGeometry {
        fragments::measured_geometry(self, text, font)
    }

    /// Adds paintable glyph information without changing the metrics returned by `measure`.
    fn shape(&mut self, text: &str, font: &FontSpec) -> ShapedText {
        let (width, height) = self.measure(text, font);
        ShapedText {
            width,
            height,
            raster_run_id: 0,
            glyphs: Vec::new(),
            geometry: TextGeometry::default(),
        }
    }
}
