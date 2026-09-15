//! Shared document fields and bounded transfer chunks.
use super::*;

pub(super) fn encode_document_start(
    writer: &mut WireWriter,
    start: &DocumentStart,
) -> Result<(), ProtocolError> {
    start.validate()?;
    writer.u64(start.document.get());
    writer.string(&start.url)?;
    writer.u16(start.status);
    writer.string(&start.content_type)?;
    writer.u32(start.diagnostic_selectors.len() as u32);
    for selector in &start.diagnostic_selectors {
        writer.string(selector)?;
    }
    writer.u32(start.body_length);
    encode_viewport(writer, start.viewport);
    writer.bool(start.prefers_dark_color_scheme);
    Ok(())
}

pub(super) fn decode_document_start(
    reader: &mut WireReader<'_>,
) -> Result<DocumentStart, ProtocolError> {
    let start = DocumentStart {
        document: DocumentId::new(reader.u64()?)?,
        url: reader.string(MAX_URL_BYTES)?,
        status: reader.u16()?,
        content_type: reader.string(16 * 1024)?,
        diagnostic_selectors: {
            let count = reader.u32()? as usize;
            if count > crate::limits::MAX_PAGE_DIAGNOSTIC_SELECTORS {
                return Err(ProtocolError::InvalidPayload(
                    "document diagnostic selectors",
                ));
            }
            (0..count)
                .map(|_| reader.string(crate::limits::MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES))
                .collect::<Result<Vec<_>, _>>()?
        },
        body_length: reader.u32()?,
        viewport: decode_viewport(reader)?,
        prefers_dark_color_scheme: reader.bool()?,
    };
    start.validate()?;
    Ok(start)
}

pub(super) fn encode_viewport(writer: &mut WireWriter, viewport: PresentedViewport) {
    writer.f32(viewport.width);
    writer.f32(viewport.height);
    writer.f32(viewport.style_width);
    writer.u32(viewport.dpi);
    writer.bool(viewport.prefers_dark_color_scheme);
}

pub(super) fn decode_viewport(
    reader: &mut WireReader<'_>,
) -> Result<PresentedViewport, ProtocolError> {
    PresentedViewport {
        width: reader.f32()?,
        height: reader.f32()?,
        style_width: reader.f32()?,
        dpi: reader.u32()?,
        prefers_dark_color_scheme: reader.bool()?,
    }
    .validate()
}

pub(super) fn encode_chunk(
    writer: &mut WireWriter,
    chunk: &TransferChunk,
) -> Result<(), ProtocolError> {
    if chunk.transfer_id == 0 || chunk.bytes.len() > MAX_FRAME_PAYLOAD.saturating_sub(16) {
        return Err(ProtocolError::InvalidPayload("transfer chunk"));
    }
    writer.u64(chunk.transfer_id);
    writer.u32(chunk.offset);
    writer.bytes(&chunk.bytes)
}

pub(super) fn decode_chunk(reader: &mut WireReader<'_>) -> Result<TransferChunk, ProtocolError> {
    Ok(TransferChunk {
        transfer_id: nonzero(reader.u64()?, "transfer")?,
        offset: reader.u32()?,
        bytes: reader.bytes(MAX_FRAME_PAYLOAD.saturating_sub(16))?,
    })
}

pub(super) fn nonzero(value: u64, field: &'static str) -> Result<u64, ProtocolError> {
    (value != 0)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
