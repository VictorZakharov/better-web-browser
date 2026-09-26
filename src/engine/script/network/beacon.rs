//! Synchronous Beacon admission in one script realm.

use super::*;

pub(super) fn queue(serialized: &str, state: &mut HostState) -> JsResult<JsValue> {
    // URL is capped at 16 KiB and a 64 KiB body encodes to at most 87,384 base64 bytes.
    // Reject oversized host-call payloads before decoding any attacker-controlled body.
    if serialized.len() > crate::limits::MAX_URL_BYTES + 88 * 1024 {
        return Ok(JsValue::Boolean(false));
    }
    let mut request = request_from_serialized(
        state
            .inherited_url
            .as_deref()
            .unwrap_or(&state.document_url),
        serialized,
    )?;
    let body_len = request
        .body
        .as_ref()
        .map_or(0, |body| body.as_bytes().len());
    // Keep the synchronous return value meaningful: this batch has actually reserved space,
    // not merely accepted a URL-shaped string. The reservation is released after IPC handoff.
    if body_len > 64 * 1024
        || state.beacon_queued_bytes.saturating_add(body_len) > 64 * 1024
        || state.beacon_queued_count >= 64
    {
        return Ok(JsValue::Boolean(false));
    }
    if request.method != "POST"
        || request.url.parsed().is_none()
        || !matches!(request.mode, RequestMode::NoCors | RequestMode::Cors)
        || request.credentials != CredentialsMode::Include
        || request.cache != RequestCache::Default
        || request.redirect != RedirectMode::Follow
        || request.headers.iter().count() > 1
        || request
            .headers
            .iter()
            .any(|header| header.name() != "content-type")
    {
        return Err(type_error("invalid Beacon request"));
    }
    request.origin = Some(state.document_origin.clone());
    request.client = state.fetch_client;
    request.policy = state.policy.clone();
    state.beacon_queued_bytes += body_len;
    state.beacon_queued_count += 1;
    state.pending_fetch_actions.push(ScriptFetchAction::Beacon {
        request: Box::new(request),
    });
    Ok(JsValue::Boolean(true))
}
