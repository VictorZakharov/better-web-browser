//! Script-facing Fetch request translation and asynchronous completion delivery.

mod base64;
pub(crate) mod response;
pub(super) mod websocket_host;
pub(super) use base64::decode_base64;
pub use websocket_host::ScriptWebSocketAction;

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
    Consume { id: u32, total: u32 },
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
            let mut request = request_from_serialized(
                state
                    .inherited_url
                    .as_deref()
                    .unwrap_or(&state.document_url),
                &serialized,
            )?;
            request.origin = Some(state.document_origin.clone());
            let id = state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?;
            state.pending_fetch_actions.push(ScriptFetchAction::Start {
                id,
                request: Box::new(request),
            });
            Ok(Some(JsValue::from(id)))
        }
        "fetchBufferLimit" => Ok(Some(JsValue::from(
            crate::limits::MAX_FETCH_STREAM_WINDOW_BYTES as u32,
        ))),
        "fetchConsumed" => {
            let id = argument_id(args, 1);
            if state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Ok(Some(JsValue::undefined()));
            }
            state
                .pending_fetch_actions
                .push(ScriptFetchAction::Consume {
                    id,
                    total: argument_id(args, 2),
                });
            Ok(Some(JsValue::undefined()))
        }
        "fetchAbort" => {
            let id = argument_id(args, 1);
            if state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Ok(Some(JsValue::undefined()));
            }
            state
                .pending_fetch_actions
                .push(ScriptFetchAction::Abort { id });
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
