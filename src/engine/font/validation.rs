//! Admission checks before a decoded webfont becomes a loaded FontFace.
//! This is not an OpenType sanitizer: individual shaping/raster tables continue
//! to be interpreted by the existing bounds-checked font backends.
use super::{read_u16, read_u32};
use crate::limits::MAX_FONT_TABLES;

pub(super) fn validate(bytes: &[u8]) -> Result<(), String> {
    let count = read_u16(bytes, 4)? as usize;
    let directory_end = 12 + count * 16;
    if count == 0 || count > MAX_FONT_TABLES || directory_end > bytes.len() {
        return Err("invalid OpenType table directory".into());
    }
    let mut tags = std::collections::HashSet::with_capacity(count);
    for record in bytes[12..directory_end].chunks_exact(16) {
        let tag: [u8; 4] = record[..4].try_into().unwrap();
        let offset = read_u32(record, 8)? as usize;
        let length = read_u32(record, 12)? as usize;
        if !tags.insert(tag)
            || offset < directory_end
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err("invalid OpenType table bounds or duplicate tag".into());
        }
    }
    #[cfg(windows)]
    validate_metrics(bytes)?;
    Ok(())
}

#[cfg(windows)]
fn validate_metrics(bytes: &[u8]) -> Result<(), String> {
    use swash::TableProvider;
    let font =
        swash::FontRef::from_index(bytes, 0).ok_or_else(|| "invalid OpenType font".to_owned())?;
    // Use the same existing parser that supplies Canvas glyphs. Merely checking
    // the sfnt signature accepts empty containers that can never render text.
    for (tag, minimum) in [(b"head", 54), (b"maxp", 6), (b"hhea", 36), (b"cmap", 4)] {
        if font
            .table_by_tag(u32::from_be_bytes(*tag))
            .is_none_or(|table| table.len() < minimum)
        {
            return Err("webfont is missing usable OpenType metrics or character mapping".into());
        }
    }
    let metrics = swash::proxy::MetricsProxy::from_font(&font);
    if !(16..=16384).contains(&metrics.units_per_em()) || metrics.glyph_count() == 0 {
        return Err("webfont has invalid units-per-em or no glyphs".into());
    }
    let hhea = font.table_by_tag(u32::from_be_bytes(*b"hhea")).unwrap();
    let horizontal_metrics = read_u16(hhea, 34)? as usize;
    let glyphs = metrics.glyph_count() as usize;
    if horizontal_metrics == 0 || horizontal_metrics > glyphs {
        return Err("webfont has invalid horizontal metric count".into());
    }
    let required_bytes = horizontal_metrics * 4 + (glyphs - horizontal_metrics) * 2;
    if font
        .table_by_tag(u32::from_be_bytes(*b"hmtx"))
        .is_none_or(|table| table.len() < required_bytes)
    {
        return Err("webfont has truncated horizontal metrics".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
