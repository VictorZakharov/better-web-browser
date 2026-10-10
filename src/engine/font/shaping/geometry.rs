//! Font metric bands shared by inline boxes and DOM Range rectangles.
use super::catalog::SelectedFont;
use crate::engine::layout::RectF;

pub(super) fn utf16_offsets(text: &str) -> Vec<u32> {
    let mut offsets = vec![0; text.len() + 1];
    let mut offset = 0;
    for (byte, ch) in text.char_indices() {
        offsets[byte] = offset;
        offset += ch.len_utf16() as u32;
    }
    offsets[text.len()] = offset;
    offsets
}

pub(super) fn font_band(font: &SelectedFont, size: f32, line_height: f32) -> RectF {
    let metrics = font_metrics(font, size);
    let height = metrics.ascent + metrics.descent;
    RectF {
        x: 0.0,
        y: (line_height - height) * 0.5,
        width: 0.0,
        height,
    }
}

pub(super) struct LineMetrics {
    pub(super) ascent: f32,
    pub(super) descent: f32,
    pub(super) leading: f32,
}
impl LineMetrics {
    pub(super) fn normal_height(&self) -> f32 {
        self.ascent + self.descent + self.leading.max(0.0)
    }
}
pub(super) fn font_metrics(font: &SelectedFont, size: f32) -> LineMetrics {
    let Some(font) = swash::FontRef::from_index(font.font.blob.as_ref(), font.font.index as usize)
    else {
        return LineMetrics {
            ascent: size * 0.8,
            descent: size * 0.2,
            leading: size * 0.2,
        };
    };
    // Swash honors USE_TYPO_METRICS and otherwise uses the hhea metrics. Round
    // each scaled metric independently, as the platform's horizontal font strike
    // does; normal is resolved during layout, not inherited as a fixed 1.2em.
    let metrics = font.metrics(&[]);
    let scale = size / metrics.units_per_em.max(1) as f32;
    LineMetrics {
        ascent: (metrics.ascent * scale).round(),
        descent: (metrics.descent.abs() * scale).round(),
        leading: (metrics.leading * scale).round(),
    }
}

pub(super) fn union_bands(a: RectF, b: RectF) -> RectF {
    if a.height == 0.0 {
        return b;
    }
    let y = a.y.min(b.y);
    RectF {
        x: 0.0,
        y,
        width: a.width.max(b.width),
        height: a.bottom().max(b.bottom()) - y,
    }
}
