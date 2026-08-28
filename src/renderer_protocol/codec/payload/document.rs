use super::fetch::{
    decode_error_kind, decode_request_head, decode_response_head, encode_request_head,
    encode_response_head, error_kind_tag,
};
use crate::limits::{MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD, MAX_URL_BYTES};
use crate::renderer_protocol::document::*;
use crate::renderer_protocol::presentation::codec::{
    decode_load, decode_runtime, encode_load, encode_runtime,
};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    BrowserMessage, NavigationCause, NavigationDisposition, PointerCursor, PointerCursorResult,
    ProtocolError, RendererMessage, RendererRuntimeUpdate,
};

pub(super) fn encode_browser_document(
    message: &BrowserMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        BrowserMessage::BeginDocument(start) => {
            encode_document_start(&mut writer, start)?;
            0x0101
        }
        BrowserMessage::DocumentChunk(chunk) => {
            encode_chunk(&mut writer, chunk)?;
            0x0103
        }
        BrowserMessage::EndDocument(document) => {
            writer.u64(document.get());
            0x0105
        }
        BrowserMessage::FetchResponseStart(head) => {
            encode_response_head(&mut writer, head)?;
            0x0111
        }
        BrowserMessage::FetchResponseChunk(chunk) => {
            encode_chunk(&mut writer, chunk)?;
            0x0113
        }
        BrowserMessage::FetchResponseEnd(end) => {
            writer.u64(end.request_id);
            writer.u32(end.total_length);
            0x0115
        }
        BrowserMessage::FetchResponseAbort(abort) => {
            writer.u64(abort.request_id);
            writer.u8(error_kind_tag(abort.error.kind));
            writer.string(&abort.error.message)?;
            0x0117
        }
        BrowserMessage::AdvanceTime {
            document,
            elapsed_micros,
            max_callbacks,
        } => {
            writer.u64(document.get());
            writer.u64(*elapsed_micros);
            writer.u32(*max_callbacks);
            0x0121
        }
        BrowserMessage::ViewportChanged { document, viewport } => {
            writer.u64(document.get());
            encode_viewport(&mut writer, *viewport);
            0x0123
        }
        BrowserMessage::CancelDocument(document) => {
            writer.u64(document.get());
            0x0125
        }
        _ => return Err(ProtocolError::InvalidPayload("browser document message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_browser_document(
    kind: u16,
    payload: &[u8],
) -> Result<BrowserMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let message = match kind {
        0x0101 => BrowserMessage::BeginDocument(decode_document_start(&mut reader)?),
        0x0103 => BrowserMessage::DocumentChunk(decode_chunk(&mut reader)?),
        0x0105 => BrowserMessage::EndDocument(DocumentId::new(reader.u64()?)?),
        0x0111 => BrowserMessage::FetchResponseStart(decode_response_head(&mut reader)?),
        0x0113 => BrowserMessage::FetchResponseChunk(decode_chunk(&mut reader)?),
        0x0115 => BrowserMessage::FetchResponseEnd(FetchResponseEnd {
            request_id: nonzero(reader.u64()?, "Fetch response")?,
            total_length: reader.u32()?,
        }),
        0x0117 => BrowserMessage::FetchResponseAbort(FetchResponseAbort {
            request_id: nonzero(reader.u64()?, "Fetch response")?,
            error: BrowserFetchError {
                kind: decode_error_kind(reader.u8()?)?,
                message: reader.string(64 * 1024)?,
            },
        }),
        0x0121 => BrowserMessage::AdvanceTime {
            document: DocumentId::new(reader.u64()?)?,
            elapsed_micros: reader.u64()?,
            max_callbacks: reader.u32()?,
        },
        0x0123 => BrowserMessage::ViewportChanged {
            document: DocumentId::new(reader.u64()?)?,
            viewport: decode_viewport(&mut reader)?,
        },
        0x0125 => BrowserMessage::CancelDocument(DocumentId::new(reader.u64()?)?),
        _ => return Err(ProtocolError::UnexpectedMessage(kind)),
    };
    reader.finish()?;
    Ok(message)
}

pub(super) fn encode_renderer_document(
    message: &RendererMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        RendererMessage::FetchBatchStart {
            document,
            batch_id,
            request_count,
        } => {
            writer.u64(document.get());
            writer.u64(*batch_id);
            writer.u32(*request_count);
            0x0102
        }
        RendererMessage::FetchRequestStart { batch_id, request } => {
            writer.u64(*batch_id);
            encode_request_head(&mut writer, request)?;
            0x0104
        }
        RendererMessage::FetchRequestChunk(chunk) => {
            encode_chunk(&mut writer, chunk)?;
            0x0106
        }
        RendererMessage::FetchRequestEnd(request_id) => {
            writer.u64(*request_id);
            0x0108
        }
        RendererMessage::FetchRequestAbort {
            document,
            request_id,
        } => {
            writer.u64(document.get());
            writer.u64(*request_id);
            0x010a
        }
        RendererMessage::PresentationStart {
            document,
            revision,
            total_length,
            encode_micros,
        } => {
            writer.u64(document.get());
            writer.u64(*revision);
            writer.u32(*total_length);
            writer.u64(*encode_micros);
            0x0112
        }
        RendererMessage::PresentationChunk(chunk) => {
            encode_chunk(&mut writer, chunk)?;
            0x0114
        }
        RendererMessage::PresentationEnd { document, revision } => {
            writer.u64(document.get());
            writer.u64(*revision);
            0x0116
        }
        RendererMessage::RuntimeUpdate(update) => {
            writer.u64(update.document.get());
            writer.bool(update.clock_advanced);
            encode_runtime(&mut writer, &update.runtime)?;
            encode_load(&mut writer, update.load);
            writer.bool(update.next_timer_micros.is_some());
            if let Some(delay) = update.next_timer_micros {
                writer.u64(delay);
            }
            0x0120
        }
        RendererMessage::DocumentFailed { document, detail } => {
            writer.u64(document.get());
            writer.string(detail)?;
            0x0118
        }
        RendererMessage::NavigationRequested {
            document,
            url,
            disposition,
            cause,
        } => {
            writer.u64(document.get());
            writer.string(url)?;
            writer.u8(match disposition {
                NavigationDisposition::CurrentTab => 1,
                NavigationDisposition::NewForegroundTab => 2,
                NavigationDisposition::NewBackgroundTab => 3,
            });
            writer.u8(match cause {
                NavigationCause::UserActivation => 1,
                NavigationCause::Redirect => 2,
            });
            0x011a
        }
        RendererMessage::PointerCursor(result) => {
            let result = result.validate()?;
            writer.u64(result.document.get());
            writer.u64(result.sequence);
            writer.u8(match result.cursor {
                PointerCursor::Default => 1,
                PointerCursor::Pointer => 2,
            });
            0x011e
        }
        _ => return Err(ProtocolError::InvalidPayload("renderer document message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_renderer_document(
    kind: u16,
    payload: &[u8],
) -> Result<RendererMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let message = match kind {
        0x0102 => RendererMessage::FetchBatchStart {
            document: DocumentId::new(reader.u64()?)?,
            batch_id: nonzero(reader.u64()?, "Fetch batch")?,
            request_count: reader.u32()?,
        },
        0x0104 => RendererMessage::FetchRequestStart {
            batch_id: nonzero(reader.u64()?, "Fetch batch")?,
            request: decode_request_head(&mut reader)?,
        },
        0x0106 => RendererMessage::FetchRequestChunk(decode_chunk(&mut reader)?),
        0x0108 => RendererMessage::FetchRequestEnd(nonzero(reader.u64()?, "Fetch request")?),
        0x010a => RendererMessage::FetchRequestAbort {
            document: DocumentId::new(reader.u64()?)?,
            request_id: nonzero(reader.u64()?, "Fetch request")?,
        },
        0x0112 => RendererMessage::PresentationStart {
            document: DocumentId::new(reader.u64()?)?,
            revision: nonzero(reader.u64()?, "presentation revision")?,
            total_length: reader.u32()?,
            encode_micros: reader.u64()?,
        },
        0x0114 => RendererMessage::PresentationChunk(decode_chunk(&mut reader)?),
        0x0116 => RendererMessage::PresentationEnd {
            document: DocumentId::new(reader.u64()?)?,
            revision: nonzero(reader.u64()?, "presentation revision")?,
        },
        0x0120 => RendererMessage::RuntimeUpdate(RendererRuntimeUpdate {
            document: DocumentId::new(reader.u64()?)?,
            clock_advanced: reader.bool()?,
            runtime: decode_runtime(&mut reader)?,
            load: decode_load(&mut reader)?,
            next_timer_micros: reader.bool()?.then(|| reader.u64()).transpose()?,
        }),
        0x0118 => RendererMessage::DocumentFailed {
            document: DocumentId::new(reader.u64()?)?,
            detail: reader.string(MAX_CONTROL_PAYLOAD)?,
        },
        0x011a => RendererMessage::NavigationRequested {
            document: DocumentId::new(reader.u64()?)?,
            url: reader.string(MAX_URL_BYTES)?,
            disposition: match reader.u8()? {
                1 => NavigationDisposition::CurrentTab,
                2 => NavigationDisposition::NewForegroundTab,
                3 => NavigationDisposition::NewBackgroundTab,
                _ => return Err(ProtocolError::InvalidPayload("navigation disposition")),
            },
            cause: match reader.u8()? {
                1 => NavigationCause::UserActivation,
                2 => NavigationCause::Redirect,
                _ => return Err(ProtocolError::InvalidPayload("navigation cause")),
            },
        },
        0x011e => RendererMessage::PointerCursor(
            PointerCursorResult {
                document: DocumentId::new(reader.u64()?)?,
                sequence: reader.u64()?,
                cursor: match reader.u8()? {
                    1 => PointerCursor::Default,
                    2 => PointerCursor::Pointer,
                    _ => return Err(ProtocolError::InvalidPayload("pointer cursor")),
                },
            }
            .validate()?,
        ),
        _ => return Err(ProtocolError::UnexpectedMessage(kind)),
    };
    reader.finish()?;
    Ok(message)
}

fn encode_document_start(
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

fn decode_document_start(reader: &mut WireReader<'_>) -> Result<DocumentStart, ProtocolError> {
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

fn encode_viewport(writer: &mut WireWriter, viewport: PresentedViewport) {
    writer.f32(viewport.width);
    writer.f32(viewport.height);
    writer.f32(viewport.style_width);
    writer.u32(viewport.dpi);
}

fn decode_viewport(reader: &mut WireReader<'_>) -> Result<PresentedViewport, ProtocolError> {
    PresentedViewport {
        width: reader.f32()?,
        height: reader.f32()?,
        style_width: reader.f32()?,
        dpi: reader.u32()?,
    }
    .validate()
}

fn encode_chunk(writer: &mut WireWriter, chunk: &TransferChunk) -> Result<(), ProtocolError> {
    if chunk.transfer_id == 0 || chunk.bytes.len() > MAX_FRAME_PAYLOAD.saturating_sub(16) {
        return Err(ProtocolError::InvalidPayload("transfer chunk"));
    }
    writer.u64(chunk.transfer_id);
    writer.u32(chunk.offset);
    writer.bytes(&chunk.bytes)
}

fn decode_chunk(reader: &mut WireReader<'_>) -> Result<TransferChunk, ProtocolError> {
    Ok(TransferChunk {
        transfer_id: nonzero(reader.u64()?, "transfer")?,
        offset: reader.u32()?,
        bytes: reader.bytes(MAX_FRAME_PAYLOAD.saturating_sub(16))?,
    })
}

fn nonzero(value: u64, field: &'static str) -> Result<u64, ProtocolError> {
    (value != 0)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
