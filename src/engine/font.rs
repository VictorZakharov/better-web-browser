use crate::limits::{MAX_FONT_BYTES, MAX_FONT_TABLES};
use crate::navigation::resolve_url;
use flate2::read::ZlibDecoder;
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WebFontFace {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub url: String,
}

#[derive(Debug)]
pub struct WebFont {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub sfnt: Vec<u8>,
}

pub fn discover_font_faces(css: &str, stylesheet_url: &str) -> Vec<WebFontFace> {
    let lowercase = css.to_ascii_lowercase();
    let mut cursor = 0;
    let mut faces = Vec::new();
    while let Some(relative_start) = lowercase[cursor..].find("@font-face") {
        let start = cursor + relative_start;
        let Some(relative_open) = lowercase[start..].find('{') else {
            break;
        };
        let open = start + relative_open;
        let Some(close) = find_matching_brace(css, open) else {
            break;
        };
        let declarations = &css[open + 1..close];
        let family = declaration_value(declarations, "font-family")
            .map(unquote)
            .filter(|family| !family.is_empty());
        let source = declaration_value(declarations, "src")
            .and_then(supported_font_url)
            .and_then(|url| resolve_url(stylesheet_url, &url));
        if let (Some(family), Some(url)) = (family, source) {
            let weight = declaration_value(declarations, "font-weight")
                .and_then(parse_font_weight)
                .unwrap_or(400);
            let italic = declaration_value(declarations, "font-style")
                .is_some_and(|style| matches!(style.trim(), "italic" | "oblique"));
            let face = WebFontFace {
                family,
                weight,
                italic,
                url,
            };
            if !faces.contains(&face) {
                faces.push(face);
            }
        }
        cursor = close + 1;
    }
    faces
}

pub fn decode_web_font(face: &WebFontFace, bytes: &[u8]) -> Result<WebFont, String> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!(
            "webfont source exceeds the {MAX_FONT_BYTES}-byte limit"
        ));
    }
    let sfnt = match bytes.get(..4) {
        Some(b"wOFF") => decode_woff(bytes)?,
        Some(b"wOF2") => return Err("WOFF2 fonts are not supported yet".into()),
        Some(b"OTTO") | Some([0, 1, 0, 0]) | Some(b"true") | Some(b"typ1") => bytes.to_vec(),
        _ => return Err("unsupported webfont container".into()),
    };
    Ok(WebFont {
        family: face.family.clone(),
        weight: face.weight,
        italic: face.italic,
        sfnt,
    })
}

fn declaration_value<'a>(declarations: &'a str, wanted: &str) -> Option<&'a str> {
    declarations.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case(wanted)
            .then_some(value.trim())
    })
}

fn supported_font_url(source: &str) -> Option<String> {
    let lowercase = source.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(relative_url) = lowercase[cursor..].find("url(") {
        let open = cursor + relative_url + 4;
        let close = find_matching_parenthesis(source, open - 1)?;
        let url = unquote(source[open..close].trim());
        let candidate_end = lowercase[close + 1..]
            .find(',')
            .map(|offset| close + 1 + offset)
            .unwrap_or(source.len());
        let descriptor = lowercase[close + 1..candidate_end].trim();
        let path = url
            .split(['?', '#'])
            .next()
            .unwrap_or(&url)
            .to_ascii_lowercase();
        let is_woff = descriptor.contains("format(\"woff\")")
            || descriptor.contains("format('woff')")
            || path.ends_with(".woff");
        let is_sfnt = descriptor.contains("format(\"truetype\")")
            || descriptor.contains("format('truetype')")
            || descriptor.contains("format(\"opentype\")")
            || descriptor.contains("format('opentype')")
            || path.ends_with(".ttf")
            || path.ends_with(".otf");
        if (is_woff || is_sfnt) && !url.starts_with("data:") {
            return Some(url);
        }
        cursor = close + 1;
    }
    None
}

fn parse_font_weight(value: &str) -> Option<u16> {
    match value.trim() {
        "normal" => Some(400),
        "bold" => Some(700),
        value => value
            .split_ascii_whitespace()
            .next()?
            .parse::<u16>()
            .ok()
            .map(|weight| weight.clamp(1, 1000)),
    }
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'))
        .trim()
        .to_string()
}

fn find_matching_brace(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quote = None;
    for (offset, character) in input[open..].char_indices() {
        match (quote, character) {
            (Some(active), candidate) if active == candidate => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '{') => depth += 1,
            (None, '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_matching_parenthesis(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quote = None;
    for (offset, character) in input[open..].char_indices() {
        match (quote, character) {
            (Some(active), candidate) if active == candidate => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_a_supported_fallback_source_for_font_faces() {
        let faces = discover_font_faces(
            r#"@font-face {
                font-family: "Montserrat";
                font-weight: 700;
                font-style: normal;
                src: url(../fonts/montserrat.woff2) format("woff2"),
                     url('../fonts/montserrat.woff') format('woff');
            }"#,
            "https://example.com/css/main.css",
        );
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].family, "Montserrat");
        assert_eq!(faces[0].weight, 700);
        assert_eq!(faces[0].url, "https://example.com/fonts/montserrat.woff");
    }

    #[test]
    fn reconstructs_an_uncompressed_woff_container() {
        let mut woff = vec![0_u8; 76];
        woff[..4].copy_from_slice(b"wOFF");
        woff[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        woff[8..12].copy_from_slice(&76_u32.to_be_bytes());
        woff[12..14].copy_from_slice(&1_u16.to_be_bytes());
        woff[16..20].copy_from_slice(&40_u32.to_be_bytes());
        woff[44..48].copy_from_slice(b"head");
        woff[48..52].copy_from_slice(&64_u32.to_be_bytes());
        woff[52..56].copy_from_slice(&12_u32.to_be_bytes());
        woff[56..60].copy_from_slice(&12_u32.to_be_bytes());

        let sfnt = decode_woff(&woff).unwrap();
        assert_eq!(sfnt.len(), 40);
        assert_eq!(&sfnt[..4], &0x0001_0000_u32.to_be_bytes());
        assert_eq!(&sfnt[12..16], b"head");
        assert_ne!(&sfnt[36..40], &[0, 0, 0, 0]);
    }
}
