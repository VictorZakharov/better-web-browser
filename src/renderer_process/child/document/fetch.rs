//! Translation between engine Fetch values and the authority-free renderer wire intent.

use crate::engine::{PageResource, ScriptKind};
use crate::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, FetchUrl,
    HeaderList, RedirectMode, Referrer, ReferrerPolicy, RequestCache, RequestDestination,
    RequestMode, ResponseType,
};
use crate::renderer_protocol::{
    BrowserFetchErrorKind, BrowserFetchResponse, DocumentId, FetchCache, FetchCredentials,
    FetchInitiator, FetchMode, FetchRedirect, FetchReferrer, FetchReferrerPolicy, FetchRequestHead,
    FetchResponseHead, FetchResponseResult, FetchResponseType, RendererFetchRequest,
    ResourceDestination,
};

pub(super) fn page_resource_request(
    request_id: u64,
    document: DocumentId,
    resource: &PageResource,
) -> RendererFetchRequest {
    let (url, initiator, destination, mode) = match resource {
        PageResource::Stylesheet { url } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Style,
            FetchMode::NoCors,
        ),
        PageResource::Image { url } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Image,
            FetchMode::NoCors,
        ),
        PageResource::Script {
            url,
            kind,
            fetch_options,
        } => (
            url,
            match kind {
                ScriptKind::Classic => FetchInitiator::ClassicScript,
                ScriptKind::Module => FetchInitiator::ModuleScript,
            },
            ResourceDestination::Script,
            mode(fetch_options.mode),
        ),
        PageResource::Font { url, .. } => (
            url,
            FetchInitiator::Subresource,
            ResourceDestination::Font,
            FetchMode::Cors,
        ),
    };
    RendererFetchRequest {
        head: FetchRequestHead {
            request_id,
            document,
            initiator,
            destination,
            url: url.clone(),
            method: "GET".into(),
            headers: Vec::new(),
            mode,
            credentials: match resource {
                PageResource::Script { fetch_options, .. } => {
                    credentials(fetch_options.credentials)
                }
                _ => FetchCredentials::SameOrigin,
            },
            cache: FetchCache::Default,
            redirect: FetchRedirect::Follow,
            referrer: FetchReferrer::Client,
            referrer_policy: match resource {
                PageResource::Script { fetch_options, .. } => {
                    referrer_policy(fetch_options.referrer_policy)
                }
                _ => FetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            },
            body_length: 0,
        },
        body: Vec::new(),
    }
}

pub(super) fn script_api_request(
    request_id: u64,
    document: DocumentId,
    request: FetchRequest,
) -> RendererFetchRequest {
    let body = request
        .body
        .as_ref()
        .map(|body| body.as_bytes().to_vec())
        .unwrap_or_default();
    RendererFetchRequest {
        head: FetchRequestHead {
            request_id,
            document,
            initiator: FetchInitiator::ScriptApi,
            destination: destination(request.destination),
            url: request.url.as_str().to_string(),
            method: request.method,
            headers: request
                .headers
                .iter()
                .map(|header| (header.name().to_string(), header.value().to_string()))
                .collect(),
            mode: mode(request.mode),
            credentials: credentials(request.credentials),
            cache: cache(request.cache),
            redirect: redirect(request.redirect),
            referrer: referrer(request.referrer),
            referrer_policy: referrer_policy(request.referrer_policy),
            body_length: body.len() as u32,
        },
        body,
    }
}

pub(super) fn into_fetch_result(
    response: BrowserFetchResponse,
) -> Result<FetchResponse, FetchError> {
    match response.head.result {
        FetchResponseResult::Success {
            response_type,
            urls,
            status,
            headers,
        } => {
            let urls = urls
                .into_iter()
                .map(|url| FetchUrl::parse(&url))
                .collect::<Result<Vec<_>, _>>()?;
            let mut header_list = HeaderList::new();
            for (name, value) in headers {
                header_list.append(&name, &value)?;
            }
            Ok(FetchResponse {
                response_type: response_type_from_wire(response_type),
                url_list: urls,
                status,
                headers: header_list,
                body: Body::from_bytes(response.body),
            })
        }
        FetchResponseResult::Failure(error) => Err(FetchError::new(
            error_kind_from_wire(error.kind),
            error.message,
        )),
    }
}

pub(super) fn into_fetch_head_result(head: FetchResponseHead) -> Result<FetchResponse, FetchError> {
    into_fetch_result(BrowserFetchResponse {
        head,
        body: Vec::new(),
    })
}

pub(super) fn into_fetch_error(error: crate::renderer_protocol::BrowserFetchError) -> FetchError {
    FetchError::new(error_kind_from_wire(error.kind), error.message)
}

/// HTML delegates module-script MIME checking to the MIME Sniffing Standard's
/// JavaScript MIME type list. Classic scripts intentionally retain legacy behavior.
pub(super) fn validate_script_response(
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

fn destination(value: RequestDestination) -> ResourceDestination {
    match value {
        RequestDestination::Style => ResourceDestination::Style,
        RequestDestination::Image => ResourceDestination::Image,
        RequestDestination::Script => ResourceDestination::Script,
        RequestDestination::Font => ResourceDestination::Font,
        RequestDestination::Document | RequestDestination::Fetch => ResourceDestination::Fetch,
    }
}

fn mode(value: RequestMode) -> FetchMode {
    match value {
        RequestMode::SameOrigin => FetchMode::SameOrigin,
        RequestMode::NoCors => FetchMode::NoCors,
        RequestMode::Cors => FetchMode::Cors,
        RequestMode::Navigate => FetchMode::Cors,
    }
}

fn credentials(value: CredentialsMode) -> FetchCredentials {
    match value {
        CredentialsMode::Omit => FetchCredentials::Omit,
        CredentialsMode::SameOrigin => FetchCredentials::SameOrigin,
        CredentialsMode::Include => FetchCredentials::Include,
    }
}

fn cache(value: RequestCache) -> FetchCache {
    match value {
        RequestCache::Default => FetchCache::Default,
        RequestCache::NoStore => FetchCache::NoStore,
        RequestCache::Reload => FetchCache::Reload,
        RequestCache::NoCache => FetchCache::NoCache,
        RequestCache::ForceCache => FetchCache::ForceCache,
        RequestCache::OnlyIfCached => FetchCache::OnlyIfCached,
    }
}

fn redirect(value: RedirectMode) -> FetchRedirect {
    match value {
        RedirectMode::Follow => FetchRedirect::Follow,
        RedirectMode::Error => FetchRedirect::Error,
        RedirectMode::Manual => FetchRedirect::Manual,
    }
}

fn referrer(value: Referrer) -> FetchReferrer {
    match value {
        Referrer::NoReferrer => FetchReferrer::None,
        Referrer::Url(url) => FetchReferrer::Url(url.as_str().to_string()),
    }
}

fn referrer_policy(value: ReferrerPolicy) -> FetchReferrerPolicy {
    match value {
        ReferrerPolicy::NoReferrer => FetchReferrerPolicy::NoReferrer,
        ReferrerPolicy::NoReferrerWhenDowngrade => FetchReferrerPolicy::NoReferrerWhenDowngrade,
        ReferrerPolicy::SameOrigin => FetchReferrerPolicy::SameOrigin,
        ReferrerPolicy::Origin => FetchReferrerPolicy::Origin,
        ReferrerPolicy::StrictOrigin => FetchReferrerPolicy::StrictOrigin,
        ReferrerPolicy::OriginWhenCrossOrigin => FetchReferrerPolicy::OriginWhenCrossOrigin,
        ReferrerPolicy::StrictOriginWhenCrossOrigin => {
            FetchReferrerPolicy::StrictOriginWhenCrossOrigin
        }
        ReferrerPolicy::UnsafeUrl => FetchReferrerPolicy::UnsafeUrl,
    }
}

fn response_type_from_wire(value: FetchResponseType) -> ResponseType {
    match value {
        FetchResponseType::Basic => ResponseType::Basic,
        FetchResponseType::Cors => ResponseType::Cors,
        FetchResponseType::Opaque => ResponseType::Opaque,
        FetchResponseType::OpaqueRedirect => ResponseType::OpaqueRedirect,
    }
}

fn error_kind_from_wire(value: BrowserFetchErrorKind) -> FetchErrorKind {
    match value {
        BrowserFetchErrorKind::InvalidRequest => FetchErrorKind::InvalidRequest,
        BrowserFetchErrorKind::Network => FetchErrorKind::Network,
        BrowserFetchErrorKind::Aborted => FetchErrorKind::Aborted,
        BrowserFetchErrorKind::Cors => FetchErrorKind::Cors,
        BrowserFetchErrorKind::Redirect => FetchErrorKind::Redirect,
        BrowserFetchErrorKind::BodyTooLarge => FetchErrorKind::BodyTooLarge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(content_type: &str) -> FetchResponse {
        let mut headers = HeaderList::new();
        if !content_type.is_empty() {
            headers.append("content-type", content_type).unwrap();
        }
        FetchResponse {
            response_type: ResponseType::Basic,
            url_list: vec![FetchUrl::parse("https://example.com/module.js").unwrap()],
            status: 200,
            headers,
            body: Body::from_bytes(b"export default 1".to_vec()),
        }
    }

    #[test]
    fn module_script_responses_require_a_javascript_mime_type() {
        for mime in [
            "text/javascript",
            "TEXT/JAVASCRIPT; charset=utf-8",
            "application/ecmascript",
            "text/javascript1.5",
        ] {
            assert!(validate_script_response(&response(mime), ScriptKind::Module).is_ok());
        }
        for mime in ["", "text/plain", "text/html", "application/json"] {
            assert!(validate_script_response(&response(mime), ScriptKind::Module).is_err());
            assert!(validate_script_response(&response(mime), ScriptKind::Classic).is_ok());
        }
    }

    #[test]
    fn script_elements_preserve_cors_credentials_and_referrer_policy() {
        let document = DocumentId::new(1).unwrap();
        let options = crate::engine::ScriptFetchOptions::for_element(
            ScriptKind::Module,
            Some("use-credentials"),
            Some("no-referrer"),
        );
        let request = page_resource_request(
            7,
            document,
            &PageResource::Script {
                url: "https://cdn.example/module.js".into(),
                kind: ScriptKind::Module,
                fetch_options: options,
            },
        );
        assert_eq!(request.head.mode, FetchMode::Cors);
        assert_eq!(request.head.credentials, FetchCredentials::Include);
        assert_eq!(
            request.head.referrer_policy,
            FetchReferrerPolicy::NoReferrer
        );

        let classic = page_resource_request(
            8,
            document,
            &PageResource::Script {
                url: "https://example.com/classic.js".into(),
                kind: ScriptKind::Classic,
                fetch_options: crate::engine::ScriptFetchOptions::for_kind(ScriptKind::Classic),
            },
        );
        assert_eq!(classic.head.mode, FetchMode::NoCors);
        assert_eq!(classic.head.credentials, FetchCredentials::Include);
    }
}
