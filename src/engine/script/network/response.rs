//! Shared classic/module response validation for top-level and child document scripts.
use super::super::ScriptKind;
use crate::fetch::{FetchError, FetchErrorKind, FetchResponse};

pub(crate) fn decode(
    response: FetchResponse,
    kind: ScriptKind,
) -> Result<(String, String), String> {
    if !response.is_success() {
        return Err(format!("server returned HTTP {}", response.status));
    }
    validate_script_response(&response, kind).map_err(|error| error.to_string())?;
    let bytes = response.body.as_bytes();
    let source = if kind == ScriptKind::Module {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .into_owned()
    } else {
        crate::winhttp::decode_text(bytes, response.content_type())
    };
    Ok((response.final_url().as_str().to_owned(), source))
}
/// HTML delegates module-script MIME checking to the MIME Sniffing Standard's
/// JavaScript MIME type list. Classic scripts intentionally retain legacy behavior.
pub(crate) fn validate_script_response(
    response: &FetchResponse,
    kind: ScriptKind,
) -> Result<(), FetchError> {
    if kind != ScriptKind::Module || !response.is_success() {
        return Ok(());
    }
    let essence = response
        .content_type()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(
        essence.as_str(),
        "application/ecmascript"
            | "application/javascript"
            | "application/x-ecmascript"
            | "application/x-javascript"
            | "text/ecmascript"
            | "text/javascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/jscript"
            | "text/livescript"
            | "text/x-ecmascript"
            | "text/x-javascript"
    ) {
        return Ok(());
    }
    Err(FetchError::new(
        FetchErrorKind::Network,
        format!(
            "module script response has non-JavaScript MIME type `{}`",
            response.content_type().unwrap_or_default().trim()
        ),
    ))
}
