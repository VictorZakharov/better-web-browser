//! Fetch policy orchestration over the one-hop WinHTTP transport.

mod stream;

use super::client::{HttpClient, TransportRequest};
use crate::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, HeaderList,
    Referrer, ReferrerPolicy, RequestCache, RequestMode, ResponseType,
    is_cors_safelisted_request_header, needs_cors_check, needs_preflight,
    validate_preflight_response,
};
use crate::limits::MAX_PREFLIGHT_BODY_BYTES;
use data_url::DataUrl;

pub use stream::StreamingFetchResponse;

impl HttpClient {
    pub fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError> {
        self.fetch_stream(request)?.into_buffered()
    }

    fn run_preflight(&self, request: &FetchRequest) -> Result<(), FetchError> {
        let origin = request
            .origin
            .as_ref()
            .expect("CORS preflight has a client origin")
            .serialize();
        let mut headers = HeaderList::new();
        headers.append("origin", &origin)?;
        headers.append("access-control-request-method", &request.method)?;
        let mut unsafe_names = request
            .headers
            .iter()
            .filter(|header| !is_cors_safelisted_request_header(header))
            .map(|header| header.name().to_string())
            .collect::<Vec<_>>();
        unsafe_names.sort_unstable();
        unsafe_names.dedup();
        if !unsafe_names.is_empty() {
            headers.append("access-control-request-headers", &unsafe_names.join(", "))?;
        }
        let response = self.send_once(TransportRequest {
            url: &request.url,
            method: "OPTIONS",
            headers: &headers,
            body: None,
            response_body_limit: MAX_PREFLIGHT_BODY_BYTES,
            signal: &request.signal,
            cache: RequestCache::NoStore,
        })?;
        if !(200..=299).contains(&response.status) {
            return Err(FetchError::new(
                FetchErrorKind::Cors,
                format!("CORS preflight returned HTTP {}", response.status),
            ));
        }
        validate_preflight_response(request, &response.headers)
    }

    fn outbound_headers(&self, request: &FetchRequest) -> Result<HeaderList, FetchError> {
        let mut headers = request.headers.clone();
        match request.cache {
            RequestCache::NoStore | RequestCache::Reload => {
                headers.set("pragma", "no-cache")?;
                headers.set("cache-control", "no-cache")?;
            }
            RequestCache::NoCache
                if !headers.contains("if-modified-since")
                    && !headers.contains("if-none-match")
                    && !headers.contains("if-unmodified-since")
                    && !headers.contains("if-match")
                    && !headers.contains("if-range") =>
            {
                headers.set("cache-control", "max-age=0")?;
            }
            _ => {}
        }
        if !headers.contains("accept-language") {
            headers.append("accept-language", "en-CA,en;q=0.9")?;
        }
        if credentials_permitted(request)
            && let Some(cookie) = self
                .cookie_header_value(request)
                .map_err(FetchError::network)?
        {
            headers.set("cookie", &cookie)?;
        }
        if let Some(referrer) = referrer_header(request) {
            headers.set("referer", &referrer)?;
        }
        if should_send_origin(request)
            && let Some(origin) = &request.origin
        {
            headers.set("origin", &origin.serialize())?;
        }
        Ok(headers)
    }

    fn store_response_cookies(
        &self,
        request: &FetchRequest,
        headers: &HeaderList,
    ) -> Result<(), FetchError> {
        if !credentials_permitted(request) {
            return Ok(());
        }
        for cookie in headers.values("set-cookie") {
            self.store_response_cookie(request, cookie)
                .map_err(FetchError::network)?;
        }
        Ok(())
    }
}

fn fetch_data_url(request: FetchRequest) -> Result<FetchResponse, FetchError> {
    request.signal.check()?;
    if request.method != "GET" {
        return Err(FetchError::new(
            FetchErrorKind::Network,
            "data URLs can only be fetched with GET",
        ));
    }
    if request.mode == RequestMode::SameOrigin && request.origin.is_some() {
        return Err(FetchError::new(
            FetchErrorKind::Cors,
            "opaque data URL is not same-origin with the requesting document",
        ));
    }
    let data = DataUrl::process(request.url.as_str()).map_err(|error| {
        FetchError::new(
            FetchErrorKind::Network,
            format!("invalid data URL: {error}"),
        )
    })?;
    let content_type = data.mime_type().to_string();
    let (bytes, _) = data.decode_to_vec().map_err(|error| {
        FetchError::new(
            FetchErrorKind::Network,
            format!("invalid data URL body: {error:?}"),
        )
    })?;
    let mut body = Body::empty(request.response_body_limit);
    body.push(&bytes)?;
    let mut headers = HeaderList::new();
    headers.append("content-type", &content_type)?;
    Ok(FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![request.url],
        status: 200,
        headers,
        body,
    })
}

fn should_send_origin(request: &FetchRequest) -> bool {
    if needs_cors_check(request) && request.mode == RequestMode::Cors {
        return true;
    }
    if matches!(request.method.as_str(), "GET" | "HEAD") {
        return false;
    }
    let Some(origin) = &request.origin else {
        return false;
    };
    let cross_origin = !origin.is_same_origin(&request.url.origin());
    let downgrade = origin.is_secure() && !request.url.is_secure();
    match request.referrer_policy {
        ReferrerPolicy::NoReferrer => false,
        ReferrerPolicy::NoReferrerWhenDowngrade
        | ReferrerPolicy::StrictOrigin
        | ReferrerPolicy::StrictOriginWhenCrossOrigin => !downgrade,
        ReferrerPolicy::SameOrigin => !cross_origin,
        _ => true,
    }
}

fn credentials_permitted(request: &FetchRequest) -> bool {
    match request.credentials {
        CredentialsMode::Omit => false,
        CredentialsMode::Include => true,
        CredentialsMode::SameOrigin => request
            .origin
            .as_ref()
            .is_some_and(|origin| origin.is_same_origin(&request.url.origin())),
    }
}

fn referrer_header(request: &FetchRequest) -> Option<String> {
    let Referrer::Url(source) = &request.referrer else {
        return None;
    };
    let same_origin = source.origin().is_same_origin(&request.url.origin());
    let downgrade = source.is_secure() && !request.url.is_secure();
    let full = || source.as_str().to_string();
    let origin = || format!("{}/", source.origin().serialize());
    match request.referrer_policy {
        ReferrerPolicy::NoReferrer => None,
        ReferrerPolicy::NoReferrerWhenDowngrade => (!downgrade).then(full),
        ReferrerPolicy::SameOrigin => same_origin.then(full),
        ReferrerPolicy::Origin => Some(origin()),
        ReferrerPolicy::StrictOrigin => (!downgrade).then(origin),
        ReferrerPolicy::OriginWhenCrossOrigin => Some(if same_origin { full() } else { origin() }),
        ReferrerPolicy::StrictOriginWhenCrossOrigin => {
            if same_origin {
                Some(full())
            } else if downgrade {
                None
            } else {
                Some(origin())
            }
        }
        ReferrerPolicy::UnsafeUrl => Some(full()),
    }
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn rewrite_redirect_method(request: &mut FetchRequest, status: u16) {
    let rewrite = status == 303 && request.method != "GET" && request.method != "HEAD"
        || matches!(status, 301 | 302) && request.method == "POST";
    if rewrite {
        request.method = "GET".into();
        request.body = None;
        request.headers.remove("content-encoding");
        request.headers.remove("content-language");
        request.headers.remove("content-location");
        request.headers.remove("content-type");
    }
}
