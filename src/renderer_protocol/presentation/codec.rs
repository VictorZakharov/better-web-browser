use super::super::wire::{WireReader, WireWriter};
use super::layout::{decode_layout, encode_layout};
use super::reader::{decode_reader, encode_reader};
use super::*;
use crate::limits::{
    MAX_DECODED_IMAGE_BYTES, MAX_DECODED_IMAGE_DIMENSION, MAX_DECODED_IMAGE_PIXELS,
    MAX_PAGE_IMAGES, MAX_RENDERED_TEXT_BYTES, MAX_RENDERER_PRESENTATION_BYTES, MAX_URL_BYTES,
};

const MAX_REPORT_ENTRIES: usize = 512;
const MAX_REPORT_TEXT: usize = 64 * 1024;

pub(super) fn encode(value: &RendererPresentation) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = WireWriter::new();
    writer.u64(value.document.get());
    writer.u64(value.revision);
    writer.string(&value.title)?;
    writer.string(&value.final_url)?;
    writer.u16(value.status);
    writer.string(&value.character_set)?;
    encode_reader(&mut writer, &value.reader)?;
    encode_runtime(&mut writer, &value.runtime)?;
    encode_style(&mut writer, value.style);
    encode_load(&mut writer, value.load);
    writer.bool(value.next_timer_micros.is_some());
    if let Some(delay) = value.next_timer_micros {
        writer.u64(delay);
    }
    encode_layout(&mut writer, &value.layout)?;
    writer.u32(value.images.len() as u32);
    for image in &value.images {
        writer.string(&image.url)?;
        writer.u32(image.image.width);
        writer.u32(image.image.height);
        writer.bytes(&image.image.bgra)?;
    }
    let bytes = writer.finish();
    if bytes.len() > MAX_RENDERER_PRESENTATION_BYTES {
        return Err(ProtocolError::PayloadTooLarge(bytes.len() as u32));
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<RendererPresentation, ProtocolError> {
    if bytes.len() > MAX_RENDERER_PRESENTATION_BYTES {
        return Err(ProtocolError::PayloadTooLarge(bytes.len() as u32));
    }
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let revision = reader.u64()?;
    if revision == 0 {
        return Err(ProtocolError::InvalidPayload("zero presentation revision"));
    }
    let title = reader.string(MAX_RENDERED_TEXT_BYTES)?;
    let final_url = reader.string(MAX_URL_BYTES)?;
    let status = reader.u16()?;
    let character_set = reader.string(256)?;
    let reader_document = decode_reader(&mut reader)?;
    if reader_document.source_url != final_url {
        return Err(ProtocolError::InvalidPayload("reader document URL"));
    }
    let runtime = decode_runtime(&mut reader)?;
    let style = decode_style(&mut reader)?;
    let load = decode_load(&mut reader)?;
    let next_timer_micros = reader.bool()?.then(|| reader.u64()).transpose()?;
    let layout = decode_layout(&mut reader)?;
    let image_count = reader.u32()? as usize;
    if image_count > MAX_PAGE_IMAGES {
        return Err(ProtocolError::InvalidPayload("presented image count"));
    }
    let mut images = Vec::with_capacity(image_count);
    let mut image_bytes = 0_usize;
    for _ in 0..image_count {
        let url = reader.string(MAX_URL_BYTES)?;
        let width = reader.u32()?;
        let height = reader.u32()?;
        let pixels = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > MAX_DECODED_IMAGE_DIMENSION
            || height > MAX_DECODED_IMAGE_DIMENSION
            || pixels > MAX_DECODED_IMAGE_PIXELS
        {
            return Err(ProtocolError::InvalidPayload("presented image dimensions"));
        }
        let expected = usize::try_from(pixels.saturating_mul(4))
            .map_err(|_| ProtocolError::InvalidPayload("presented image size"))?;
        let bgra = reader.bytes(MAX_DECODED_IMAGE_BYTES as usize)?;
        if bgra.len() != expected {
            return Err(ProtocolError::InvalidPayload("presented image pixels"));
        }
        image_bytes = image_bytes
            .checked_add(bgra.len())
            .ok_or(ProtocolError::InvalidPayload("presented image budget"))?;
        if image_bytes > MAX_RENDERER_PRESENTATION_BYTES {
            return Err(ProtocolError::InvalidPayload("presented image budget"));
        }
        images.push(PresentedImage {
            url,
            image: DecodedImage {
                width,
                height,
                bgra,
            },
        });
    }
    reader.finish()?;
    Ok(RendererPresentation {
        document,
        revision,
        title,
        final_url,
        status,
        character_set,
        reader: reader_document,
        layout,
        images,
        runtime,
        style,
        load,
        next_timer_micros,
    })
}

fn encode_runtime(writer: &mut WireWriter, report: &RuntimeReport) -> Result<(), ProtocolError> {
    writer.u64(report.scripts_executed);
    writer.u64(report.dom_mutations);
    encode_strings(writer, &report.errors)?;
    encode_strings(writer, &report.console)?;
    encode_strings(writer, &report.diagnostics)?;
    writer.bool(report.navigation_url.is_some());
    if let Some(url) = &report.navigation_url {
        writer.string(url)?;
    }
    encode_strings(writer, &report.cookie_updates)?;
    writer.bool(report.runtime_active);
    writer.bool(report.runtime_stopped);
    writer.bool(report.render_requested);
    Ok(())
}

fn decode_runtime(reader: &mut WireReader<'_>) -> Result<RuntimeReport, ProtocolError> {
    let scripts_executed = reader.u64()?;
    let dom_mutations = reader.u64()?;
    let errors = decode_strings(reader)?;
    let console = decode_strings(reader)?;
    let diagnostics = decode_strings(reader)?;
    let navigation_url = reader
        .bool()?
        .then(|| reader.string(MAX_URL_BYTES))
        .transpose()?;
    let cookie_updates = decode_strings(reader)?;
    Ok(RuntimeReport {
        scripts_executed,
        dom_mutations,
        errors,
        console,
        diagnostics,
        navigation_url,
        cookie_updates,
        runtime_active: reader.bool()?,
        runtime_stopped: reader.bool()?,
        render_requested: reader.bool()?,
    })
}

fn encode_strings(writer: &mut WireWriter, values: &[String]) -> Result<(), ProtocolError> {
    if values.len() > MAX_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("runtime report count"));
    }
    writer.u32(values.len() as u32);
    for value in values {
        if value.len() > MAX_REPORT_TEXT {
            return Err(ProtocolError::InvalidPayload("runtime report text"));
        }
        writer.string(value)?;
    }
    Ok(())
}

fn decode_strings(reader: &mut WireReader<'_>) -> Result<Vec<String>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("runtime report count"));
    }
    (0..count).map(|_| reader.string(MAX_REPORT_TEXT)).collect()
}

fn encode_style(writer: &mut WireWriter, report: StyleReport) {
    writer.u64(report.invalidated_nodes);
    writer.u64(report.total_styles);
    writer.u64(report.recomputed_styles);
    writer.u64(report.changed_styles);
    writer.u64(report.removed_styles);
    writer.bool(report.layout_changed);
    writer.bool(report.full_rebuild);
}

fn decode_style(reader: &mut WireReader<'_>) -> Result<StyleReport, ProtocolError> {
    Ok(StyleReport {
        invalidated_nodes: reader.u64()?,
        total_styles: reader.u64()?,
        recomputed_styles: reader.u64()?,
        changed_styles: reader.u64()?,
        removed_styles: reader.u64()?,
        layout_changed: reader.bool()?,
        full_rebuild: reader.bool()?,
    })
}

fn encode_load(writer: &mut WireWriter, report: PageLoadReport) {
    writer.u64(report.parse_micros);
    writer.u64(report.html_parse_micros);
    writer.u64(report.resource_processing_micros);
    writer.u64(report.script_micros);
    writer.u64(report.style_micros);
    writer.u64(report.layout_micros);
    writer.u64(report.text_measure_count);
}

fn decode_load(reader: &mut WireReader<'_>) -> Result<PageLoadReport, ProtocolError> {
    Ok(PageLoadReport {
        parse_micros: reader.u64()?,
        html_parse_micros: reader.u64()?,
        resource_processing_micros: reader.u64()?,
        script_micros: reader.u64()?,
        style_micros: reader.u64()?,
        layout_micros: reader.u64()?,
        text_measure_count: reader.u64()?,
    })
}
