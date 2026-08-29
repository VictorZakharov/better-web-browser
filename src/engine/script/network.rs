//! Script-facing Fetch request translation and asynchronous completion delivery.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
use crate::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, RedirectMode,
    Referrer, ReferrerPolicy, RequestCache, RequestMode, ResponseType,
};
use crate::limits::MAX_RESPONSE_BODY_BYTES;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum ScriptFetchAction {
    Start { id: u32, request: Box<FetchRequest> },
    Abort { id: u32 },
}

#[derive(Debug)]
pub enum ScriptFetchEvent {
    Head(Result<FetchResponse, FetchError>),
    Chunk(Vec<u8>),
    End,
    Abort(FetchError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SerializedRequest {
    url: String,
    method: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body_base64: Option<String>,
    mode: String,
    credentials: String,
    cache: String,
    redirect: String,
    referrer: String,
    referrer_policy: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializedResponse {
    ok: bool,
    url: String,
    status: u16,
    status_text: &'static str,
    response_type: &'static str,
    redirected: bool,
    headers: Vec<(String, String)>,
    error_name: Option<&'static str>,
    error_message: Option<String>,
}

pub(super) fn network_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "fetchStart" => {
            let serialized = argument_string(args, 1)?;
            let request = request_from_serialized(&state.document_url, &serialized)?;
            let id = state.next_fetch_id;
            state.next_fetch_id = state.next_fetch_id.checked_add(1).ok_or_else(|| {
                JsNativeError::range().with_message("Fetch request identifiers were exhausted")
            })?;
            state.pending_fetch_actions.push(ScriptFetchAction::Start {
                id,
                request: Box::new(request),
            });
            Ok(Some(JsValue::from(id)))
        }
        "fetchAbort" => {
            state.pending_fetch_actions.push(ScriptFetchAction::Abort {
                id: argument_id(args, 1),
            });
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}

pub(super) fn request_from_serialized(
    document_url: &str,
    serialized: &str,
) -> JsResult<FetchRequest> {
    let init: SerializedRequest = serde_json::from_str(serialized).map_err(|error| {
        JsNativeError::typ().with_message(format!("invalid Fetch request: {error}"))
    })?;
    let mut request = FetchRequest::script(&init.url, document_url).map_err(fetch_error)?;
    request.set_method(&init.method).map_err(fetch_error)?;
    request.mode = match init.mode.as_str() {
        "same-origin" => RequestMode::SameOrigin,
        "no-cors" => RequestMode::NoCors,
        "cors" => RequestMode::Cors,
        _ => {
            return Err(type_error(format!(
                "unsupported request mode `{}`",
                init.mode
            )));
        }
    };
    request.credentials = match init.credentials.as_str() {
        "omit" => CredentialsMode::Omit,
        "same-origin" => CredentialsMode::SameOrigin,
        "include" => CredentialsMode::Include,
        _ => {
            return Err(type_error(format!(
                "unsupported credentials mode `{}`",
                init.credentials
            )));
        }
    };
    request.cache = match init.cache.as_str() {
        "default" => RequestCache::Default,
        "no-store" => RequestCache::NoStore,
        "reload" => RequestCache::Reload,
        "no-cache" => RequestCache::NoCache,
        "force-cache" => RequestCache::ForceCache,
        "only-if-cached" => RequestCache::OnlyIfCached,
        _ => {
            return Err(type_error(format!(
                "unsupported request cache mode `{}`",
                init.cache
            )));
        }
    };
    request.redirect = match init.redirect.as_str() {
        "follow" => RedirectMode::Follow,
        "error" => RedirectMode::Error,
        "manual" => RedirectMode::Manual,
        _ => {
            return Err(type_error(format!(
                "unsupported redirect mode `{}`",
                init.redirect
            )));
        }
    };
    request.referrer = match init.referrer.as_str() {
        "" => Referrer::NoReferrer,
        "about:client" => request.referrer.clone(),
        value => Referrer::Url(crate::fetch::FetchUrl::parse(value).map_err(fetch_error)?),
    };
    request.referrer_policy = match init.referrer_policy.as_str() {
        "" | "strict-origin-when-cross-origin" => ReferrerPolicy::StrictOriginWhenCrossOrigin,
        "no-referrer" => ReferrerPolicy::NoReferrer,
        "no-referrer-when-downgrade" => ReferrerPolicy::NoReferrerWhenDowngrade,
        "same-origin" => ReferrerPolicy::SameOrigin,
        "origin" => ReferrerPolicy::Origin,
        "strict-origin" => ReferrerPolicy::StrictOrigin,
        "origin-when-cross-origin" => ReferrerPolicy::OriginWhenCrossOrigin,
        "unsafe-url" => ReferrerPolicy::UnsafeUrl,
        _ => {
            return Err(type_error(format!(
                "unsupported referrer policy `{}`",
                init.referrer_policy
            )));
        }
    };
    for (name, value) in init.headers {
        request
            .set_script_header(&name, &value)
            .map_err(fetch_error)?;
    }
    if let Some(encoded) = init.body_base64 {
        let bytes = decode_base64(&encoded).map_err(type_error)?;
        if bytes.len() > MAX_RESPONSE_BODY_BYTES {
            return Err(JsNativeError::range()
                .with_message(format!(
                    "request body exceeds the {MAX_RESPONSE_BODY_BYTES}-byte limit"
                ))
                .into());
        }
        request.body = Some(Body::from_bytes(bytes));
    }
    request.validate().map_err(fetch_error)?;
    Ok(request)
}

pub(super) fn deliver_completion(
    context: &mut Context,
    id: u32,
    result: Result<FetchResponse, FetchError>,
) -> JsResult<()> {
    match result {
        Ok(mut response) => {
            let bytes =
                std::mem::replace(&mut response.body, Body::from_bytes(Vec::new())).into_bytes();
            deliver_event(context, id, ScriptFetchEvent::Head(Ok(response)))?;
            if !bytes.is_empty() {
                deliver_event(context, id, ScriptFetchEvent::Chunk(bytes))?;
            }
            deliver_event(context, id, ScriptFetchEvent::End)
        }
        Err(error) => deliver_event(context, id, ScriptFetchEvent::Head(Err(error))),
    }
}

pub(super) fn deliver_event(
    context: &mut Context,
    id: u32,
    event: ScriptFetchEvent,
) -> JsResult<()> {
    match event {
        ScriptFetchEvent::Head(result) => {
            let metadata = serde_json::to_string(&serialized_response(result))
                .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
            call_network_hook(
                context,
                "__startFetch",
                &[JsValue::from(id), js_string(metadata)],
            )?;
        }
        ScriptFetchEvent::Chunk(bytes) => {
            let body = JsValue::Bytes(bytes);
            call_network_hook(context, "__pushFetch", &[JsValue::from(id), body])?;
        }
        ScriptFetchEvent::End => {
            call_network_hook(context, "__finishFetch", &[JsValue::from(id)])?;
        }
        ScriptFetchEvent::Abort(error) => {
            let name = if error.kind() == FetchErrorKind::Aborted {
                "AbortError"
            } else {
                "TypeError"
            };
            call_network_hook(
                context,
                "__abortFetch",
                &[
                    JsValue::from(id),
                    js_string(name.to_string()),
                    js_string(error.to_string()),
                ],
            )?;
        }
    }
    context.run_jobs()
}

fn serialized_response(result: Result<FetchResponse, FetchError>) -> SerializedResponse {
    match result {
        Ok(response) => SerializedResponse {
            ok: true,
            url: response.final_url().as_str().to_string(),
            status: response.status,
            status_text: status_text(response.status),
            response_type: response_type(response.response_type),
            redirected: response.url_list.len() > 1,
            headers: response
                .headers
                .iter()
                .map(|header| (header.name().to_string(), header.value().to_string()))
                .collect(),
            error_name: None,
            error_message: None,
        },
        Err(error) => SerializedResponse {
            ok: false,
            url: String::new(),
            status: 0,
            status_text: "",
            response_type: "error",
            redirected: false,
            headers: Vec::new(),
            error_name: Some(if error.kind() == FetchErrorKind::Aborted {
                "AbortError"
            } else {
                "TypeError"
            }),
            error_message: Some(error.to_string()),
        },
    }
}

fn call_network_hook(context: &mut Context, name: &str, arguments: &[JsValue]) -> JsResult<()> {
    context.call_global(name, arguments)?;
    Ok(())
}

fn response_type(response_type: ResponseType) -> &'static str {
    match response_type {
        ResponseType::Basic => "basic",
        ResponseType::Cors => "cors",
        ResponseType::Opaque => "opaque",
        ResponseType::OpaqueRedirect => "opaqueredirect",
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        418 => "I'm a Teapot",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

fn fetch_error(error: FetchError) -> JsError {
    type_error(error.to_string())
}

fn type_error(message: impl Into<String>) -> JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(4) {
        return Err("request body has invalid base64 length".into());
    }
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    for (chunk_index, chunk) in input.as_bytes().chunks_exact(4).enumerate() {
        let last = chunk_index + 1 == input.len() / 4;
        let padding = usize::from(chunk[3] == b'=') + usize::from(chunk[2] == b'=');
        if (padding > 0 && !last) || (chunk[2] == b'=' && chunk[3] != b'=') {
            return Err("request body has invalid base64 padding".into());
        }
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3])?
        };
        let bits = (u32::from(a) << 18) | (u32::from(b) << 12) | (u32::from(c) << 6) | u32::from(d);
        output.push((bits >> 16) as u8);
        if padding < 2 {
            output.push((bits >> 8) as u8);
        }
        if padding == 0 {
            output.push(bits as u8);
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("request body contains invalid base64 data".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_standard_padded_base64() {
        assert_eq!(decode_base64("UnVzdA==").unwrap(), b"Rust");
        assert_eq!(decode_base64("SGVsbG8h").unwrap(), b"Hello!");
        assert!(decode_base64("abc").is_err());
        assert!(decode_base64("AA=A").is_err());
    }
}
