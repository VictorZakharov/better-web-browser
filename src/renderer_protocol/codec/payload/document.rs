use crate::limits::{MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD, MAX_REDIRECTS, MAX_URL_BYTES};
use crate::renderer_protocol::document::*;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{BrowserMessage, ProtocolError, RendererMessage};

const MAX_HEADERS: usize = 256;
const MAX_HEADER_NAME: usize = 1024;
const MAX_HEADER_VALUE: usize = 16 * 1024;

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
        BrowserMessage::FetchResponseEnd(request_id) => {
            writer.u64(*request_id);
            0x0115
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
        0x0115 => BrowserMessage::FetchResponseEnd(nonzero(reader.u64()?, "Fetch response")?),
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
        RendererMessage::PresentationStart {
            document,
            revision,
            total_length,
        } => {
            writer.u64(document.get());
            writer.u64(*revision);
            writer.u32(*total_length);
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
        RendererMessage::TimeAdvanced {
            document,
            next_timer_micros,
        } => {
            writer.u64(document.get());
            writer.bool(next_timer_micros.is_some());
            if let Some(delay) = next_timer_micros {
                writer.u64(*delay);
            }
            0x011c
        }
        RendererMessage::DocumentFailed { document, detail } => {
            writer.u64(document.get());
            writer.string(detail)?;
            0x0118
        }
        RendererMessage::NavigationRequested { document, url } => {
            writer.u64(document.get());
            writer.string(url)?;
            0x011a
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
        0x0112 => RendererMessage::PresentationStart {
            document: DocumentId::new(reader.u64()?)?,
            revision: nonzero(reader.u64()?, "presentation revision")?,
            total_length: reader.u32()?,
        },
        0x0114 => RendererMessage::PresentationChunk(decode_chunk(&mut reader)?),
        0x0116 => RendererMessage::PresentationEnd {
            document: DocumentId::new(reader.u64()?)?,
            revision: nonzero(reader.u64()?, "presentation revision")?,
        },
        0x011c => RendererMessage::TimeAdvanced {
            document: DocumentId::new(reader.u64()?)?,
            next_timer_micros: reader.bool()?.then(|| reader.u64()).transpose()?,
        },
        0x0118 => RendererMessage::DocumentFailed {
            document: DocumentId::new(reader.u64()?)?,
            detail: reader.string(MAX_CONTROL_PAYLOAD)?,
        },
        0x011a => RendererMessage::NavigationRequested {
            document: DocumentId::new(reader.u64()?)?,
            url: reader.string(MAX_URL_BYTES)?,
        },
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
    writer.string(&start.cookie_header)?;
    writer.u32(start.body_length);
    encode_viewport(writer, start.viewport);
    Ok(())
}

fn decode_document_start(reader: &mut WireReader<'_>) -> Result<DocumentStart, ProtocolError> {
    let start = DocumentStart {
        document: DocumentId::new(reader.u64()?)?,
        url: reader.string(MAX_URL_BYTES)?,
        status: reader.u16()?,
        content_type: reader.string(16 * 1024)?,
        cookie_header: reader.string(64 * 1024)?,
        body_length: reader.u32()?,
        viewport: decode_viewport(reader)?,
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

fn encode_request_head(
    writer: &mut WireWriter,
    head: &FetchRequestHead,
) -> Result<(), ProtocolError> {
    head.validate()?;
    writer.u64(head.request_id);
    writer.u64(head.document.get());
    writer.u8(initiator_tag(head.initiator));
    writer.u8(destination_tag(head.destination));
    writer.string(&head.url)?;
    writer.string(&head.method)?;
    encode_headers(writer, &head.headers)?;
    writer.u8(mode_tag(head.mode));
    writer.u8(credentials_tag(head.credentials));
    writer.u8(cache_tag(head.cache));
    writer.u8(redirect_tag(head.redirect));
    encode_referrer(writer, &head.referrer)?;
    writer.u8(referrer_policy_tag(head.referrer_policy));
    writer.u32(head.body_length);
    Ok(())
}

fn decode_request_head(reader: &mut WireReader<'_>) -> Result<FetchRequestHead, ProtocolError> {
    let head = FetchRequestHead {
        request_id: nonzero(reader.u64()?, "Fetch request")?,
        document: DocumentId::new(reader.u64()?)?,
        initiator: decode_initiator(reader.u8()?)?,
        destination: decode_destination(reader.u8()?)?,
        url: reader.string(MAX_URL_BYTES)?,
        method: reader.string(64)?,
        headers: decode_headers(reader)?,
        mode: decode_mode(reader.u8()?)?,
        credentials: decode_credentials(reader.u8()?)?,
        cache: decode_cache(reader.u8()?)?,
        redirect: decode_redirect(reader.u8()?)?,
        referrer: decode_referrer(reader)?,
        referrer_policy: decode_referrer_policy(reader.u8()?)?,
        body_length: reader.u32()?,
    };
    head.validate()?;
    Ok(head)
}

fn encode_response_head(
    writer: &mut WireWriter,
    head: &FetchResponseHead,
) -> Result<(), ProtocolError> {
    writer.u64(head.request_id);
    match &head.result {
        FetchResponseResult::Success {
            response_type,
            urls,
            status,
            headers,
            body_length,
        } => {
            writer.u8(1);
            writer.u8(response_type_tag(*response_type));
            writer.u32(urls.len() as u32);
            for url in urls {
                writer.string(url)?;
            }
            writer.u16(*status);
            encode_headers(writer, headers)?;
            writer.u32(*body_length);
        }
        FetchResponseResult::Failure(error) => {
            writer.u8(2);
            writer.u8(error_kind_tag(error.kind));
            writer.string(&error.message)?;
        }
    }
    Ok(())
}

fn decode_response_head(reader: &mut WireReader<'_>) -> Result<FetchResponseHead, ProtocolError> {
    let request_id = nonzero(reader.u64()?, "Fetch response")?;
    let result = match reader.u8()? {
        1 => {
            let response_type = decode_response_type(reader.u8()?)?;
            let count = reader.u32()? as usize;
            if count == 0 || count > MAX_REDIRECTS + 1 {
                return Err(ProtocolError::InvalidPayload("Fetch response URLs"));
            }
            let urls = (0..count)
                .map(|_| reader.string(MAX_URL_BYTES))
                .collect::<Result<Vec<_>, _>>()?;
            FetchResponseResult::Success {
                response_type,
                urls,
                status: reader.u16()?,
                headers: decode_headers(reader)?,
                body_length: reader.u32()?,
            }
        }
        2 => FetchResponseResult::Failure(BrowserFetchError {
            kind: decode_error_kind(reader.u8()?)?,
            message: reader.string(64 * 1024)?,
        }),
        _ => return Err(ProtocolError::InvalidPayload("Fetch response result")),
    };
    let head = FetchResponseHead { request_id, result };
    if head.body_length() > crate::limits::MAX_RESPONSE_BODY_BYTES {
        return Err(ProtocolError::InvalidPayload("Fetch response body"));
    }
    Ok(head)
}

fn encode_headers(
    writer: &mut WireWriter,
    headers: &[(String, String)],
) -> Result<(), ProtocolError> {
    if headers.len() > MAX_HEADERS {
        return Err(ProtocolError::InvalidPayload("header count"));
    }
    writer.u32(headers.len() as u32);
    for (name, value) in headers {
        if name.len() > MAX_HEADER_NAME || value.len() > MAX_HEADER_VALUE {
            return Err(ProtocolError::InvalidPayload("header length"));
        }
        writer.string(name)?;
        writer.string(value)?;
    }
    Ok(())
}

fn decode_headers(reader: &mut WireReader<'_>) -> Result<Vec<(String, String)>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_HEADERS {
        return Err(ProtocolError::InvalidPayload("header count"));
    }
    (0..count)
        .map(|_| {
            Ok((
                reader.string(MAX_HEADER_NAME)?,
                reader.string(MAX_HEADER_VALUE)?,
            ))
        })
        .collect()
}

fn encode_referrer(writer: &mut WireWriter, referrer: &FetchReferrer) -> Result<(), ProtocolError> {
    match referrer {
        FetchReferrer::Client => writer.u8(1),
        FetchReferrer::None => writer.u8(2),
        FetchReferrer::Url(url) => {
            writer.u8(3);
            writer.string(url)?;
        }
    }
    Ok(())
}

fn decode_referrer(reader: &mut WireReader<'_>) -> Result<FetchReferrer, ProtocolError> {
    match reader.u8()? {
        1 => Ok(FetchReferrer::Client),
        2 => Ok(FetchReferrer::None),
        3 => Ok(FetchReferrer::Url(reader.string(MAX_URL_BYTES)?)),
        _ => Err(ProtocolError::InvalidPayload("Fetch referrer")),
    }
}

fn nonzero(value: u64, field: &'static str) -> Result<u64, ProtocolError> {
    (value != 0)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}

macro_rules! tagged_enum {
    ($encode:ident, $decode:ident, $ty:ty, $field:literal, {$($variant:path => $tag:literal),+ $(,)?}) => {
        fn $encode(value: $ty) -> u8 { match value { $($variant => $tag),+ } }
        fn $decode(tag: u8) -> Result<$ty, ProtocolError> { match tag { $($tag => Ok($variant)),+, _ => Err(ProtocolError::InvalidPayload($field)) } }
    };
}

tagged_enum!(initiator_tag, decode_initiator, FetchInitiator, "Fetch initiator", { FetchInitiator::Subresource => 1, FetchInitiator::ClassicScript => 2, FetchInitiator::ModuleScript => 3, FetchInitiator::ScriptApi => 4, FetchInitiator::ClassicWorker => 5, FetchInitiator::ModuleWorker => 6 });
tagged_enum!(destination_tag, decode_destination, ResourceDestination, "Fetch destination", { ResourceDestination::Style => 1, ResourceDestination::Image => 2, ResourceDestination::Script => 3, ResourceDestination::Font => 4, ResourceDestination::Fetch => 5 });
tagged_enum!(mode_tag, decode_mode, FetchMode, "Fetch mode", { FetchMode::SameOrigin => 1, FetchMode::NoCors => 2, FetchMode::Cors => 3 });
tagged_enum!(credentials_tag, decode_credentials, FetchCredentials, "Fetch credentials", { FetchCredentials::Omit => 1, FetchCredentials::SameOrigin => 2, FetchCredentials::Include => 3 });
tagged_enum!(cache_tag, decode_cache, FetchCache, "Fetch cache", { FetchCache::Default => 1, FetchCache::NoStore => 2, FetchCache::Reload => 3, FetchCache::NoCache => 4, FetchCache::ForceCache => 5, FetchCache::OnlyIfCached => 6 });
tagged_enum!(redirect_tag, decode_redirect, FetchRedirect, "Fetch redirect", { FetchRedirect::Follow => 1, FetchRedirect::Error => 2, FetchRedirect::Manual => 3 });
tagged_enum!(referrer_policy_tag, decode_referrer_policy, FetchReferrerPolicy, "Fetch referrer policy", { FetchReferrerPolicy::NoReferrer => 1, FetchReferrerPolicy::NoReferrerWhenDowngrade => 2, FetchReferrerPolicy::SameOrigin => 3, FetchReferrerPolicy::Origin => 4, FetchReferrerPolicy::StrictOrigin => 5, FetchReferrerPolicy::OriginWhenCrossOrigin => 6, FetchReferrerPolicy::StrictOriginWhenCrossOrigin => 7, FetchReferrerPolicy::UnsafeUrl => 8 });
tagged_enum!(response_type_tag, decode_response_type, FetchResponseType, "Fetch response type", { FetchResponseType::Basic => 1, FetchResponseType::Cors => 2, FetchResponseType::Opaque => 3, FetchResponseType::OpaqueRedirect => 4 });
tagged_enum!(error_kind_tag, decode_error_kind, BrowserFetchErrorKind, "Fetch error kind", { BrowserFetchErrorKind::InvalidRequest => 1, BrowserFetchErrorKind::Network => 2, BrowserFetchErrorKind::Aborted => 3, BrowserFetchErrorKind::Cors => 4, BrowserFetchErrorKind::Redirect => 5, BrowserFetchErrorKind::BodyTooLarge => 6 });
