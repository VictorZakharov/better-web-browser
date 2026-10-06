use crate::limits::{MAX_FONT_BYTES, MAX_FONT_TABLES};
use flate2::read::ZlibDecoder;
use std::io::Read;

#[derive(Debug, Clone, PartialEq)]
pub struct WebFontFace {
    pub family: String,
    pub weight: u16,
    /// Inclusive CSS Fonts 4 descriptor interval; matching uses this instead of `weight`.
    pub weight_min: f32,
    pub weight_max: f32,
    pub italic: bool,
    pub url: String,
    /// Supported remote candidates after `url`, in authored order.
    pub fallback_urls: Vec<String>,
    pub unicode_range: String,
    pub features: crate::engine::css::FontFeatures,
}

pub(crate) mod descriptors;
mod face_match;
#[cfg(windows)]
pub(crate) mod shaping;
pub(crate) mod sources;
#[cfg(all(test, windows))]
pub(crate) mod test_features;
#[cfg(test)]
mod tests;
pub(crate) mod unicode_ranges;

#[derive(Debug, Clone)]
pub struct WebFont {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    /// Validated immutable font bytes shared by page snapshots and font catalogs.
    pub sfnt: std::sync::Arc<[u8]>,
    pub source_url: String,
    /// Script-owned FontFaceSet membership; stylesheet faces have no source ID.
    pub script_source_id: Option<u32>,
    pub(crate) unicode_ranges: unicode_ranges::UnicodeRanges,
    pub features: crate::engine::css::FontFeatures,
}

#[cfg(test)]
fn discover_font_faces(css: &str, stylesheet_url: &str) -> Vec<WebFontFace> {
    crate::engine::css::stylesheet::font_faces::collect(
        css,
        stylesheet_url,
        crate::engine::css::media::MediaEnvironment::new(800.0, 600.0, 1.0, false),
    )
}

pub fn decode_web_font(face: &WebFontFace, bytes: &[u8]) -> Result<WebFont, String> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!(
            "webfont source exceeds the {MAX_FONT_BYTES}-byte limit"
        ));
    }
    let sfnt = match bytes.get(..4) {
        Some(b"wOFF") => decode_woff(bytes)?,
        Some(b"wOF2") => {
            // Reject oversized containers before the decoder allocates reconstructed tables.
            if bytes.len() < 48
                || read_u32(bytes, 16)? as usize > MAX_FONT_BYTES
                || read_u16(bytes, 12)? as usize > MAX_FONT_TABLES
            {
                return Err("WOFF2 font exceeds the browser font limits".into());
            }
            wuff::decompress_woff2(bytes).map_err(|error| format!("invalid WOFF2 font: {error}"))?
        }
        Some(b"OTTO") | Some([0, 1, 0, 0]) | Some(b"true") | Some(b"typ1") => bytes.to_vec(),
        _ => return Err("unsupported webfont container".into()),
    };
    if sfnt.len() > MAX_FONT_BYTES {
        return Err(format!(
            "decoded webfont exceeds the {MAX_FONT_BYTES}-byte limit"
        ));
    }
    Ok(WebFont {
        family: face.family.clone(),
        weight: face.weight,
        italic: face.italic,
        sfnt: sfnt.into(),
        source_url: face.url.clone(),
        script_source_id: None,
        unicode_ranges: unicode_ranges::UnicodeRanges::parse(&face.unicode_range)
            .ok_or_else(|| "invalid font unicode-range descriptor".to_owned())?,
        features: face.features.clone(),
    })
}

#[cfg(test)]
fn parse_font_weight(value: &str) -> Option<(f32, f32)> {
    descriptors::weight(value)
}

#[derive(Clone, Copy)]
struct WoffTable {
    tag: [u8; 4],
    source_offset: usize,
    compressed_length: usize,
    original_length: usize,
    checksum: u32,
}

fn decode_woff(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 44 || bytes.get(..4) != Some(b"wOFF") {
        return Err("invalid WOFF header".into());
    }
    let declared_length = read_u32(bytes, 8)? as usize;
    let table_count = read_u16(bytes, 12)? as usize;
    let total_sfnt_size = read_u32(bytes, 16)? as usize;
    if declared_length > bytes.len()
        || table_count == 0
        || table_count > MAX_FONT_TABLES
        || total_sfnt_size > MAX_FONT_BYTES
        || 44 + table_count * 20 > declared_length
    {
        return Err("WOFF header exceeds resource limits".into());
    }

    let mut tables = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let record = 44 + index * 20;
        let source_offset = read_u32(bytes, record + 4)? as usize;
        let compressed_length = read_u32(bytes, record + 8)? as usize;
        let original_length = read_u32(bytes, record + 12)? as usize;
        if source_offset
            .checked_add(compressed_length)
            .is_none_or(|end| end > declared_length)
            || compressed_length > original_length
        {
            return Err("invalid WOFF table bounds".into());
        }
        tables.push(WoffTable {
            tag: bytes[record..record + 4].try_into().unwrap(),
            source_offset,
            compressed_length,
            original_length,
            checksum: read_u32(bytes, record + 16)?,
        });
    }

    let directory_size = 12 + table_count * 16;
    if total_sfnt_size < directory_size {
        return Err("invalid WOFF SFNT size".into());
    }
    let mut output = vec![0_u8; total_sfnt_size];
    output[..4].copy_from_slice(&bytes[4..8]);
    write_u16(&mut output, 4, table_count as u16)?;
    let maximum_power = (usize::BITS - 1 - table_count.leading_zeros()) as u16;
    let search_range = (1_u16 << maximum_power) * 16;
    write_u16(&mut output, 6, search_range)?;
    write_u16(&mut output, 8, maximum_power)?;
    write_u16(&mut output, 10, table_count as u16 * 16 - search_range)?;

    let mut destination = directory_size;
    let mut head_offset = None;
    for (index, table) in tables.iter().enumerate() {
        destination = align4(destination);
        let end = destination
            .checked_add(table.original_length)
            .filter(|end| *end <= output.len())
            .ok_or_else(|| "WOFF tables exceed declared SFNT size".to_string())?;
        let source = &bytes[table.source_offset..table.source_offset + table.compressed_length];
        if table.compressed_length == table.original_length {
            output[destination..end].copy_from_slice(source);
        } else {
            let mut decoder = ZlibDecoder::new(source);
            let mut decoded = Vec::with_capacity(table.original_length);
            decoder
                .read_to_end(&mut decoded)
                .map_err(|error| format!("decompress WOFF table: {error}"))?;
            if decoded.len() != table.original_length {
                return Err("decompressed WOFF table has the wrong length".into());
            }
            output[destination..end].copy_from_slice(&decoded);
        }

        let record = 12 + index * 16;
        output[record..record + 4].copy_from_slice(&table.tag);
        write_u32(&mut output, record + 4, table.checksum)?;
        write_u32(&mut output, record + 8, destination as u32)?;
        write_u32(&mut output, record + 12, table.original_length as u32)?;
        if &table.tag == b"head" && table.original_length >= 12 {
            head_offset = Some(destination);
        }
        destination = end;
    }

    if let Some(head) = head_offset {
        output[head + 8..head + 12].fill(0);
        let checksum = output.chunks(4).fold(0_u32, |sum, chunk| {
            let mut word = [0_u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        });
        write_u32(
            &mut output,
            head + 8,
            0xB1B0_AFBA_u32.wrapping_sub(checksum),
        )?;
    }
    Ok(output)
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_be_bytes)
        .ok_or_else(|| "truncated font data".into())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| "truncated font data".into())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), String> {
    bytes
        .get_mut(offset..offset + 2)
        .ok_or_else(|| "truncated font output".to_string())?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| "truncated font output".to_string())?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}
