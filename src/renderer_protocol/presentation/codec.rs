use super::super::wire::{WireReader, WireWriter};
use super::diagnostics::{decode_diagnostics, encode_diagnostics};
use super::layout::{decode_layout, encode_layout};
use super::reader::{decode_reader, encode_reader};
use super::*;
use crate::limits::{
    MAX_DECODED_IMAGE_BYTES, MAX_DECODED_IMAGE_DIMENSION, MAX_DECODED_IMAGE_PIXELS,
    MAX_GLYPH_RASTER_BYTES, MAX_GLYPH_RASTER_DIMENSION, MAX_GLYPH_RASTER_PIXELS, MAX_GLYPH_RASTERS,
    MAX_PAGE_DIAGNOSTIC_BYTES, MAX_PAGE_IMAGES, MAX_PRESENTED_GLYPH_BYTES, MAX_RENDERED_TEXT_BYTES,
    MAX_RENDERER_PRESENTATION_BYTES, MAX_RUNTIME_REPORT_ENTRIES, MAX_RUNTIME_REPORT_TEXT_BYTES,
    MAX_URL_BYTES,
};
use std::collections::HashSet;

pub(super) fn encode(value: &RendererPresentation) -> Result<Vec<u8>, ProtocolError> {
    validate_glyph_rasters(value.glyph_epoch, &value.glyphs)?;
    let mut writer = WireWriter::new();
    writer.u64(value.document.get());
    writer.u64(value.revision);
    writer.bool(value.clock_advanced);
    writer.string(&value.title)?;
    writer.string(&value.final_url)?;
    writer.u16(value.status);
    writer.string(&value.character_set)?;
    encode_reader(&mut writer, &value.reader)?;
    encode_runtime(&mut writer, &value.runtime)?;
    encode_style(&mut writer, value.style);
    encode_load(&mut writer, value.load);
    writer.bytes(&encode_diagnostics(&value.page_diagnostics)?)?;
    value.accessibility.encode_into(&mut writer)?;
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
    writer.u64(value.glyph_epoch);
    writer.u32(value.glyphs.len() as u32);
    for glyph in &value.glyphs {
        writer.u32(glyph.id);
        writer.bool(glyph.color);
        writer.u32(glyph.image.width);
        writer.u32(glyph.image.height);
        writer.bytes(&glyph.image.bgra)?;
    }
    let bytes = writer.finish();
    if bytes.len() > MAX_RENDERER_PRESENTATION_BYTES {
        return Err(ProtocolError::PayloadTooLarge(bytes.len() as u32));
    }
    Ok(bytes)
}

fn validate_glyph_rasters(
    epoch: u64,
    glyphs: &[PresentedGlyphRaster],
) -> Result<(), ProtocolError> {
    if epoch == 0 {
        return Err(ProtocolError::InvalidPayload("glyph epoch"));
    }
    if glyphs.len() > MAX_GLYPH_RASTERS {
        return Err(ProtocolError::InvalidPayload("glyph raster count"));
    }
    let mut ids = HashSet::with_capacity(glyphs.len());
    let mut total = 0_usize;
    for glyph in glyphs {
        let pixels = u64::from(glyph.image.width) * u64::from(glyph.image.height);
        let expected = usize::try_from(pixels.saturating_mul(4))
            .map_err(|_| ProtocolError::InvalidPayload("glyph raster size"))?;
        if glyph.id == 0
            || !ids.insert(glyph.id)
            || glyph.image.width == 0
            || glyph.image.height == 0
            || glyph.image.width > MAX_GLYPH_RASTER_DIMENSION
            || glyph.image.height > MAX_GLYPH_RASTER_DIMENSION
            || pixels > MAX_GLYPH_RASTER_PIXELS
            || expected > MAX_GLYPH_RASTER_BYTES
            || glyph.image.bgra.len() != expected
        {
            return Err(ProtocolError::InvalidPayload("glyph raster"));
        }
        total = total
            .checked_add(expected)
            .ok_or(ProtocolError::InvalidPayload("glyph raster budget"))?;
    }
    if total > MAX_PRESENTED_GLYPH_BYTES {
        return Err(ProtocolError::InvalidPayload("glyph raster budget"));
    }
    Ok(())
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
    let clock_advanced = reader.bool()?;
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
    let page_diagnostics = decode_diagnostics(&reader.bytes(MAX_PAGE_DIAGNOSTIC_BYTES)?)?;
    let accessibility = AccessibilityUpdate::decode_from(&mut reader)?;
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
    let glyph_epoch = reader.u64()?;
    if glyph_epoch == 0 {
        return Err(ProtocolError::InvalidPayload("glyph epoch"));
    }
    let glyph_count = reader.u32()? as usize;
    if glyph_count > MAX_GLYPH_RASTERS {
        return Err(ProtocolError::InvalidPayload("glyph raster count"));
    }
    let mut glyphs = Vec::with_capacity(glyph_count);
    let mut glyph_ids = HashSet::with_capacity(glyph_count);
    let mut glyph_bytes = 0_usize;
    for _ in 0..glyph_count {
        let id = reader.u32()?;
        if id == 0 || !glyph_ids.insert(id) {
            return Err(ProtocolError::InvalidPayload("glyph raster identifier"));
        }
        let color = reader.bool()?;
        let width = reader.u32()?;
        let height = reader.u32()?;
        let pixels = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > MAX_GLYPH_RASTER_DIMENSION
            || height > MAX_GLYPH_RASTER_DIMENSION
            || pixels > MAX_GLYPH_RASTER_PIXELS
        {
            return Err(ProtocolError::InvalidPayload("glyph raster dimensions"));
        }
        let expected = usize::try_from(pixels.saturating_mul(4))
            .map_err(|_| ProtocolError::InvalidPayload("glyph raster size"))?;
        let bgra = reader.bytes(MAX_GLYPH_RASTER_BYTES)?;
        if bgra.len() != expected {
            return Err(ProtocolError::InvalidPayload("glyph raster pixels"));
        }
        glyph_bytes = glyph_bytes
            .checked_add(bgra.len())
            .ok_or(ProtocolError::InvalidPayload("glyph raster budget"))?;
        if glyph_bytes > MAX_PRESENTED_GLYPH_BYTES {
            return Err(ProtocolError::InvalidPayload("glyph raster budget"));
        }
        glyphs.push(PresentedGlyphRaster {
            id,
            image: DecodedImage {
                width,
                height,
                bgra,
            },
            color,
        });
    }
    reader.finish()?;
    Ok(RendererPresentation {
        document,
        revision,
        clock_advanced,
        title,
        final_url,
        status,
        character_set,
        reader: reader_document,
        layout,
        images,
        glyph_epoch,
        glyphs,
        runtime,
        style,
        load,
        page_diagnostics,
        accessibility,
        next_timer_micros,
    })
}

pub(in crate::renderer_protocol) fn encode_runtime(
    writer: &mut WireWriter,
    report: &RuntimeReport,
) -> Result<(), ProtocolError> {
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

pub(in crate::renderer_protocol) fn decode_runtime(
    reader: &mut WireReader<'_>,
) -> Result<RuntimeReport, ProtocolError> {
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
    if values.len() > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("runtime report count"));
    }
    writer.u32(values.len() as u32);
    for value in values {
        if value.len() > MAX_RUNTIME_REPORT_TEXT_BYTES {
            return Err(ProtocolError::InvalidPayload("runtime report text"));
        }
        writer.string(value)?;
    }
    Ok(())
}

fn decode_strings(reader: &mut WireReader<'_>) -> Result<Vec<String>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("runtime report count"));
    }
    (0..count)
        .map(|_| reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES))
        .collect()
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

pub(in crate::renderer_protocol) fn encode_load(writer: &mut WireWriter, report: PageLoadReport) {
    writer.u64(report.parse_micros);
    writer.u64(report.html_parse_micros);
    writer.u64(report.resource_processing_micros);
    writer.u64(report.script_micros);
    writer.u64(report.style_micros);
    writer.u64(report.layout_micros);
    writer.u64(report.text_measure_count);
    writer.u64(report.text_shape_cache_hits);
    writer.u64(report.text_shape_cache_misses);
    writer.u64(report.text_shape_cache_flushes);
    writer.u64(report.text_shape_cache_entries);
    writer.u64(report.font_catalog_micros);
    writer.u64(report.font_select_micros);
    writer.u64(report.open_type_shape_micros);
    writer.u64(report.glyph_raster_micros);
    writer.u64(report.presentation_encode_micros);
    writer.u64(report.presentation_decode_micros);
}

pub(in crate::renderer_protocol) fn decode_load(
    reader: &mut WireReader<'_>,
) -> Result<PageLoadReport, ProtocolError> {
    Ok(PageLoadReport {
        parse_micros: reader.u64()?,
        html_parse_micros: reader.u64()?,
        resource_processing_micros: reader.u64()?,
        script_micros: reader.u64()?,
        style_micros: reader.u64()?,
        layout_micros: reader.u64()?,
        text_measure_count: reader.u64()?,
        text_shape_cache_hits: reader.u64()?,
        text_shape_cache_misses: reader.u64()?,
        text_shape_cache_flushes: reader.u64()?,
        text_shape_cache_entries: reader.u64()?,
        font_catalog_micros: reader.u64()?,
        font_select_micros: reader.u64()?,
        open_type_shape_micros: reader.u64()?,
        glyph_raster_micros: reader.u64()?,
        presentation_encode_micros: reader.u64()?,
        presentation_decode_micros: reader.u64()?,
    })
}
