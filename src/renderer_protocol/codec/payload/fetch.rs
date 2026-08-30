use crate::limits::{
    MAX_FETCH_HEADER_NAME_BYTES, MAX_FETCH_HEADER_VALUE_BYTES, MAX_REDIRECTS,
    MAX_RENDERER_FETCH_HEADERS, MAX_URL_BYTES,
};
use crate::renderer_protocol::ProtocolError;
use crate::renderer_protocol::document::*;
use crate::renderer_protocol::wire::{WireReader, WireWriter};

pub(super) fn encode_request_head(
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

pub(super) fn decode_request_head(
    reader: &mut WireReader<'_>,
) -> Result<FetchRequestHead, ProtocolError> {
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

pub(super) fn encode_response_head(
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
        } => {
            writer.u8(1);
            writer.u8(response_type_tag(*response_type));
            writer.u32(urls.len() as u32);
            for url in urls {
                writer.string(url)?;
            }
            writer.u16(*status);
            encode_headers(writer, headers)?;
        }
        FetchResponseResult::Failure(error) => {
            writer.u8(2);
            writer.u8(error_kind_tag(error.kind));
            writer.string(&error.message)?;
        }
    }
    Ok(())
}

pub(super) fn decode_response_head(
    reader: &mut WireReader<'_>,
) -> Result<FetchResponseHead, ProtocolError> {
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
            }
        }
        2 => FetchResponseResult::Failure(BrowserFetchError {
            kind: decode_error_kind(reader.u8()?)?,
            message: reader.string(64 * 1024)?,
        }),
        _ => return Err(ProtocolError::InvalidPayload("Fetch response result")),
    };
    Ok(FetchResponseHead { request_id, result })
}

pub(super) fn error_kind_tag(kind: BrowserFetchErrorKind) -> u8 {
    match kind {
        BrowserFetchErrorKind::InvalidRequest => 1,
        BrowserFetchErrorKind::Network => 2,
        BrowserFetchErrorKind::Aborted => 3,
        BrowserFetchErrorKind::Cors => 4,
        BrowserFetchErrorKind::Redirect => 5,
        BrowserFetchErrorKind::BodyTooLarge => 6,
    }
}

pub(super) fn decode_error_kind(tag: u8) -> Result<BrowserFetchErrorKind, ProtocolError> {
    match tag {
        1 => Ok(BrowserFetchErrorKind::InvalidRequest),
        2 => Ok(BrowserFetchErrorKind::Network),
        3 => Ok(BrowserFetchErrorKind::Aborted),
        4 => Ok(BrowserFetchErrorKind::Cors),
        5 => Ok(BrowserFetchErrorKind::Redirect),
        6 => Ok(BrowserFetchErrorKind::BodyTooLarge),
        _ => Err(ProtocolError::InvalidPayload("Fetch error kind")),
    }
}

fn encode_headers(
    writer: &mut WireWriter,
    headers: &[(String, String)],
) -> Result<(), ProtocolError> {
    if headers.len() > MAX_RENDERER_FETCH_HEADERS {
        return Err(ProtocolError::InvalidPayload("header count"));
    }
    writer.u32(headers.len() as u32);
    for (name, value) in headers {
        if name.len() > MAX_FETCH_HEADER_NAME_BYTES || value.len() > MAX_FETCH_HEADER_VALUE_BYTES {
            return Err(ProtocolError::InvalidPayload("header length"));
        }
        writer.string(name)?;
        writer.string(value)?;
    }
    Ok(())
}

fn decode_headers(reader: &mut WireReader<'_>) -> Result<Vec<(String, String)>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_RENDERER_FETCH_HEADERS {
        return Err(ProtocolError::InvalidPayload("header count"));
    }
    (0..count)
        .map(|_| {
            Ok((
                reader.string(MAX_FETCH_HEADER_NAME_BYTES)?,
                reader.string(MAX_FETCH_HEADER_VALUE_BYTES)?,
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
tagged_enum!(destination_tag, decode_destination, ResourceDestination, "Fetch destination", { ResourceDestination::Style => 1, ResourceDestination::Image => 2, ResourceDestination::Script => 3, ResourceDestination::Font => 4, ResourceDestination::Fetch => 5, ResourceDestination::Video => 6 });
tagged_enum!(mode_tag, decode_mode, FetchMode, "Fetch mode", { FetchMode::SameOrigin => 1, FetchMode::NoCors => 2, FetchMode::Cors => 3 });
tagged_enum!(credentials_tag, decode_credentials, FetchCredentials, "Fetch credentials", { FetchCredentials::Omit => 1, FetchCredentials::SameOrigin => 2, FetchCredentials::Include => 3 });
tagged_enum!(cache_tag, decode_cache, FetchCache, "Fetch cache", { FetchCache::Default => 1, FetchCache::NoStore => 2, FetchCache::Reload => 3, FetchCache::NoCache => 4, FetchCache::ForceCache => 5, FetchCache::OnlyIfCached => 6 });
tagged_enum!(redirect_tag, decode_redirect, FetchRedirect, "Fetch redirect", { FetchRedirect::Follow => 1, FetchRedirect::Error => 2, FetchRedirect::Manual => 3 });
tagged_enum!(referrer_policy_tag, decode_referrer_policy, FetchReferrerPolicy, "Fetch referrer policy", { FetchReferrerPolicy::NoReferrer => 1, FetchReferrerPolicy::NoReferrerWhenDowngrade => 2, FetchReferrerPolicy::SameOrigin => 3, FetchReferrerPolicy::Origin => 4, FetchReferrerPolicy::StrictOrigin => 5, FetchReferrerPolicy::OriginWhenCrossOrigin => 6, FetchReferrerPolicy::StrictOriginWhenCrossOrigin => 7, FetchReferrerPolicy::UnsafeUrl => 8 });
tagged_enum!(response_type_tag, decode_response_type, FetchResponseType, "Fetch response type", { FetchResponseType::Basic => 1, FetchResponseType::Cors => 2, FetchResponseType::Opaque => 3, FetchResponseType::OpaqueRedirect => 4 });
