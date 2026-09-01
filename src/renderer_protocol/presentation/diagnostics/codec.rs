use super::super::super::wire::{WireReader, WireWriter};
use super::*;

mod geometry;
mod style;
use crate::limits::{
    MAX_PAGE_DIAGNOSTIC_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS,
};
use crate::renderer_protocol::ProtocolError;
use geometry::{decode_optional_rect, decode_rects, encode_optional_rect, encode_rects};
use style::{decode_style, encode_style};

const MAX_MATCHES_PER_SELECTOR: usize = 32;
const MAX_ATTRIBUTES_PER_NODE: usize = 64;
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
    writer.u64(value.attribute_count);
    writer.bool(value.attributes_truncated);
    count(value.attributes.len(), MAX_ATTRIBUTES_PER_NODE)?;
    writer.u32(value.attributes.len() as u32);
    for attribute in &value.attributes {
        string(writer, &attribute.name, MAX_DIAGNOSTIC_TEXT_BYTES)?;
        string(writer, &attribute.value, MAX_DIAGNOSTIC_TEXT_BYTES)?;
    }
    if value.attribute_count < value.attributes.len() as u64
        || value.attributes_truncated != (value.attribute_count > value.attributes.len() as u64)
    {
        return invalid();
    }
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
    let attribute_count = reader.u64()?;
    let attributes_truncated = reader.bool()?;
    let retained_attribute_count = bounded_count(reader.u32()?, MAX_ATTRIBUTES_PER_NODE)?;
    let mut attributes = Vec::with_capacity(retained_attribute_count);
    for _ in 0..retained_attribute_count {
        attributes.push(AttributeDiagnostics {
            name: reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?,
            value: reader.string(MAX_DIAGNOSTIC_TEXT_BYTES)?,
        });
    }
    if attribute_count < retained_attribute_count as u64
        || attributes_truncated != (attribute_count > retained_attribute_count as u64)
    {
        return invalid();
    }
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
        attribute_count,
        attributes_truncated,
        attributes,
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
