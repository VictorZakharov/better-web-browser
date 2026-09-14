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
    let height = swash::FontRef::from_index(font.font.blob.as_ref(), font.font.index as usize)
        .map(|font| {
            let metrics = font.metrics(&[]);
            (metrics.ascent + metrics.descent.abs()) / metrics.units_per_em.max(1) as f32 * size
        })
        .unwrap_or(size);
    // Keep the existing painting baseline: half-leading + ascent.
    RectF {
        x: 0.0,
        y: (line_height - size).max(0.0) * 0.5,
        width: 0.0,
        height,
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
