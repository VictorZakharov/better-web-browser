use super::super::super::wire::{WireReader, WireWriter};
use super::*;

mod geometry;
use crate::limits::{
    MAX_PAGE_DIAGNOSTIC_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS,
};
use crate::renderer_protocol::ProtocolError;
use geometry::{decode_optional_rect, decode_rects, encode_optional_rect, encode_rects};

const MAX_MATCHES_PER_SELECTOR: usize = 32;
const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 512;
const MAX_RESOURCE_RECTS: usize = 8;
const MAX_COORDINATE: f32 = 10_000_000.0;

pub(in crate::renderer_protocol::presentation) fn encode_diagnostics(
    value: &PageDiagnostics,
) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = WireWriter::new();
    optional_string(
        &mut writer,
        value.error.as_deref(),
        MAX_DIAGNOSTIC_TEXT_BYTES,
    )?;
    if value.error.is_some() && !value.selectors.is_empty() {
        return invalid();
    }
    count(value.selectors.len(), MAX_PAGE_DIAGNOSTIC_SELECTORS)?;
    writer.u32(value.selectors.len() as u32);
    for selector in &value.selectors {
        encode_selector(&mut writer, selector)?;
    }
    let bytes = writer.finish();
    if bytes.len() > MAX_PAGE_DIAGNOSTIC_BYTES {
        return invalid();
    }
    Ok(bytes)
}

pub(in crate::renderer_protocol::presentation) fn decode_diagnostics(
    bytes: &[u8],
) -> Result<PageDiagnostics, ProtocolError> {
    if bytes.len() > MAX_PAGE_DIAGNOSTIC_BYTES {
        return invalid();
    }
    let mut reader = WireReader::new(bytes);
    let error = decode_optional_string(&mut reader, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let selector_count = bounded_count(reader.u32()?, MAX_PAGE_DIAGNOSTIC_SELECTORS)?;
    if error.is_some() && selector_count != 0 {
        return invalid();
    }
    let mut selectors = Vec::with_capacity(selector_count);
    for _ in 0..selector_count {
        selectors.push(decode_selector(&mut reader)?);
    }
    reader.finish()?;
    Ok(PageDiagnostics { error, selectors })
}

fn encode_selector(
    writer: &mut WireWriter,
    value: &SelectorDiagnostics,
) -> Result<(), ProtocolError> {
    if value.selector.is_empty() {
        return invalid();
    }
    string(writer, &value.selector, MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES)?;
    optional_string(writer, value.error.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    if value.error.is_some()
        && (value.total_matches != 0 || value.truncated || !value.matches.is_empty())
    {
        return invalid();
    }
    writer.u64(value.total_matches);
    writer.bool(value.truncated);
    count(value.matches.len(), MAX_MATCHES_PER_SELECTOR)?;
    writer.u32(value.matches.len() as u32);
    for node in &value.matches {
        encode_node(writer, node)?;
    }
    Ok(())
}

fn decode_selector(reader: &mut WireReader<'_>) -> Result<SelectorDiagnostics, ProtocolError> {
    let selector = reader.string(MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES)?;
    if selector.is_empty() {
        return invalid();
    }
    let error = decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let total_matches = reader.u64()?;
    let truncated = reader.bool()?;
    let match_count = bounded_count(reader.u32()?, MAX_MATCHES_PER_SELECTOR)?;
    if error.is_some() && (total_matches != 0 || truncated || match_count != 0) {
        return invalid();
    }
    let mut matches = Vec::with_capacity(match_count);
    for _ in 0..match_count {
        matches.push(decode_node(reader)?);
    }
    Ok(SelectorDiagnostics {
        selector,
        error,
        total_matches,
        truncated,
        matches,
    })
}

fn encode_node(writer: &mut WireWriter, value: &NodeDiagnostics) -> Result<(), ProtocolError> {
    writer.u128(value.node_id);
    optional_string(writer, value.tag.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    optional_string(writer, value.id.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    optional_string(writer, value.class.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    encode_optional_identity(writer, value.parent.as_ref())?;
    encode_optional_identity(writer, value.composed_parent.as_ref())?;
    writer.u64(value.child_count);
    writer.u64(value.text_length);
    writer.bool(value.shadow_root.is_some());
    if let Some(shadow) = &value.shadow_root {
        writer.u64(shadow.child_count);
        writer.u64(shadow.descendant_count);
        writer.u64(shadow.text_length);
    }
    encode_optional_resource(writer, value.element_image.as_ref())?;
    encode_style(writer, &value.style)?;
    encode_optional_rect(writer, value.layout_rect)?;
    encode_optional_rect(writer, value.control_rect)?;
    Ok(())
}

fn decode_node(reader: &mut WireReader<'_>) -> Result<NodeDiagnostics, ProtocolError> {
    let node_id = reader.u128()?;
    let tag = decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let id = decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let class = decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let parent = decode_optional_identity(reader)?;
    let composed_parent = decode_optional_identity(reader)?;
    let child_count = reader.u64()?;
    let text_length = reader.u64()?;
    let shadow_root = if reader.bool()? {
        Some(ShadowRootDiagnostics {
            child_count: reader.u64()?,
            descendant_count: reader.u64()?,
            text_length: reader.u64()?,
        })
    } else {
        None
    };
    Ok(NodeDiagnostics {
        node_id,
        tag,
        id,
        class,
        parent,
        composed_parent,
        child_count,
        text_length,
        shadow_root,
        element_image: decode_optional_resource(reader)?,
        style: decode_style(reader)?,
        layout_rect: decode_optional_rect(reader)?,
        control_rect: decode_optional_rect(reader)?,
    })
}

fn encode_optional_identity(
    writer: &mut WireWriter,
    value: Option<&NodeIdentityDiagnostics>,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        optional_string(writer, value.tag.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
        optional_string(writer, value.id.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
        optional_string(writer, value.class.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    }
    Ok(())
}

fn decode_optional_identity(
    reader: &mut WireReader<'_>,
) -> Result<Option<NodeIdentityDiagnostics>, ProtocolError> {
    reader
        .bool()?
        .then(|| {
            Ok(NodeIdentityDiagnostics {
                tag: decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?,
                id: decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?,
                class: decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?,
            })
        })
        .transpose()
}

fn encode_style(writer: &mut WireWriter, value: &StyleDiagnostics) -> Result<(), ProtocolError> {
    for text in [
        &value.display,
        &value.position,
        &value.float,
        &value.flex_direction,
        &value.flex_basis,
        &value.align_items,
        &value.justify_content,
        &value.list_style_type,
        &value.width,
        &value.height,
        &value.min_width,
        &value.max_width,
        &value.min_height,
        &value.max_height,
        &value.background_color,
    ] {
        string(writer, text, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    }
    writer.bool(value.visibility);
    if !value.opacity.is_finite() || !(0.0..=1.0).contains(&value.opacity) {
        return invalid();
    }
    writer.f32(value.opacity);
    writer.bool(value.overflow_hidden);
    writer.bool(value.flex_wrap);
    for number in [value.flex_grow, value.flex_shrink] {
        if !number.is_finite() || number < 0.0 {
            return invalid();
        }
        writer.f32(number);
    }
    encode_optional_resource(writer, value.background_image.as_ref())?;
    encode_optional_resource(writer, value.mask_image.as_ref())?;
    Ok(())
}

fn decode_style(reader: &mut WireReader<'_>) -> Result<StyleDiagnostics, ProtocolError> {
    let display = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let position = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let float = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let flex_direction = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let flex_basis = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let align_items = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let justify_content = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let list_style_type = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let min_width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let max_width = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let min_height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let max_height = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let background_color = reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?;
    let visibility = reader.bool()?;
    let opacity = reader.f32()?;
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return invalid();
    }
    let overflow_hidden = reader.bool()?;
    let flex_wrap = reader.bool()?;
    let flex_grow = reader.f32()?;
    let flex_shrink = reader.f32()?;
    if !flex_grow.is_finite() || flex_grow < 0.0 || !flex_shrink.is_finite() || flex_shrink < 0.0 {
        return invalid();
    }
    Ok(StyleDiagnostics {
        display,
        position,
        float,
        flex_direction,
        flex_wrap,
        flex_grow,
        flex_shrink,
        flex_basis,
        align_items,
        justify_content,
        visibility,
        opacity,
        overflow_hidden,
        list_style_type,
        width,
        height,
        min_width,
        max_width,
        min_height,
        max_height,
        background_color,
        background_image: decode_optional_resource(reader)?,
        mask_image: decode_optional_resource(reader)?,
    })
}

fn encode_optional_resource(
    writer: &mut WireWriter,
    value: Option<&ResourceDiagnostics>,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    let Some(value) = value else {
        return Ok(());
    };
    string(writer, &value.kind, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    optional_string(writer, value.url.as_deref(), MAX_DIAGNOSTIC_TEXT_BYTES)?;
    optional_string(
        writer,
        value.data_prefix.as_deref(),
        MAX_DIAGNOSTIC_TEXT_BYTES,
    )?;
    writer.bool(value.decoded);
    optional_u32(writer, value.width);
    optional_u32(writer, value.height);
    optional_u64(writer, value.nontransparent_pixels);
    encode_rects(writer, &value.paint_rects)?;
    encode_rects(writer, &value.control_rects)
}

fn decode_optional_resource(
    reader: &mut WireReader<'_>,
) -> Result<Option<ResourceDiagnostics>, ProtocolError> {
    if !reader.bool()? {
        return Ok(None);
    }
    Ok(Some(ResourceDiagnostics {
        kind: reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?,
        url: decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?,
        data_prefix: decode_optional_string(reader, MAX_DIAGNOSTIC_TEXT_BYTES)?,
        decoded: reader.bool()?,
        width: decode_optional_u32(reader)?,
        height: decode_optional_u32(reader)?,
        nontransparent_pixels: decode_optional_u64(reader)?,
        paint_rects: decode_rects(reader)?,
        control_rects: decode_rects(reader)?,
    }))
}

fn string(writer: &mut WireWriter, value: &str, maximum: usize) -> Result<(), ProtocolError> {
    if value.len() > maximum {
        return invalid();
    }
    writer.string(value)
}

fn optional_string(
    writer: &mut WireWriter,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), ProtocolError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        string(writer, value, maximum)?;
    }
    Ok(())
}

fn decode_optional_string(
    reader: &mut WireReader<'_>,
    maximum: usize,
) -> Result<Option<String>, ProtocolError> {
    if !reader.bool()? {
        return Ok(None);
    }
    let value = reader.string(maximum)?;
    Ok(Some(value))
}

fn optional_u32(writer: &mut WireWriter, value: Option<u32>) {
    writer.bool(value.is_some());
    if let Some(value) = value {
        writer.u32(value);
    }
}

fn decode_optional_u32(reader: &mut WireReader<'_>) -> Result<Option<u32>, ProtocolError> {
    reader.bool()?.then(|| reader.u32()).transpose()
}

fn optional_u64(writer: &mut WireWriter, value: Option<u64>) {
    writer.bool(value.is_some());
    if let Some(value) = value {
        writer.u64(value);
    }
}

fn decode_optional_u64(reader: &mut WireReader<'_>) -> Result<Option<u64>, ProtocolError> {
    reader.bool()?.then(|| reader.u64()).transpose()
}

fn bounded_count(value: u32, maximum: usize) -> Result<usize, ProtocolError> {
    let value = value as usize;
    count(value, maximum)?;
    Ok(value)
}

fn count(value: usize, maximum: usize) -> Result<(), ProtocolError> {
    if value > maximum {
        return invalid();
    }
    Ok(())
}

fn invalid<T>() -> Result<T, ProtocolError> {
    Err(ProtocolError::InvalidPayload("page diagnostics"))
}
