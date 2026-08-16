//! Fetch policy orchestration over the one-hop WinHTTP transport.

use super::client::{HttpClient, TransportResponse};
use crate::fetch::{
    Body, CredentialsMode, FetchError, FetchErrorKind, FetchRequest, FetchResponse, FetchUrl,
    HeaderList, RedirectMode, Referrer, RequestContext, RequestMode, ResponseType,
    cors_filtered_headers, is_cors_safelisted_request_header, needs_cors_check, needs_preflight,
    validate_cors_response, validate_preflight_response,
};
use crate::limits::{MAX_PREFLIGHT_BODY_BYTES, MAX_REDIRECTS};

impl HttpClient {
    pub fn fetch(&self, mut request: FetchRequest) -> Result<FetchResponse, FetchError> {
        request.validate()?;
        request.signal.check()?;

        let mut url_list = vec![request.url.clone()];
        for redirect_count in 0..=MAX_REDIRECTS {
            request.signal.check()?;
            if needs_cors_check(&request) && request.mode == RequestMode::SameOrigin {
                return Err(FetchError::new(
                    FetchErrorKind::Cors,
                    "cross-origin request blocked by same-origin mode",
                ));
            }
            if needs_preflight(&request) {
                self.run_preflight(&request)?;
            }

            let outbound_headers = self.outbound_headers(&request)?;
            let transport = self.send_once(
                &request.url,
                &request.method,
                &outbound_headers,
                request.body.as_ref(),
                request.response_body_limit,
                &request.signal,
            )?;
            self.store_response_cookies(&request, &transport.headers)?;
            validate_cors_response(&request, &transport.headers)?;

            if is_redirect_status(transport.status)
                && let Some(location) = transport.headers.get("location")
            {
                match request.redirect {
                    RedirectMode::Error => {
                        return Err(FetchError::new(
                            FetchErrorKind::Redirect,
                            "redirect blocked by request redirect mode",
                        ));
                    }
                    RedirectMode::Manual => {
                        return Ok(manual_redirect_response(request, url_list, transport));
                    }
                    RedirectMode::Follow => {}
                }
                if redirect_count == MAX_REDIRECTS {
                    return Err(FetchError::new(
                        FetchErrorKind::Redirect,
                        format!("redirect limit of {MAX_REDIRECTS} was exceeded"),
                    ));
                }
                let next_url = request.url.resolve(location)?;
                if !request.url.origin().is_same_origin(&next_url.origin()) {
                    request.headers.remove("authorization");
                }
                rewrite_redirect_method(&mut request, transport.status);
                request.url = next_url.clone();
                url_list.push(next_url);
                continue;
            }

            return Ok(filtered_response(request, url_list, transport));
        }
        unreachable!("the bounded redirect loop always returns")
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
        let response = self.send_once(
            &request.url,
            "OPTIONS",
            &headers,
            None,
            MAX_PREFLIGHT_BODY_BYTES,
            &request.signal,
        )?;
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
        if !headers.contains("accept-language") {
            headers.append("accept-language", "en-CA,en;q=0.9")?;
        }
        if credentials_permitted(request)
            && let Some(cookie) = self
                .cookie_header_value(request.url.parsed())
                .map_err(FetchError::network)?
        {
            headers.set("cookie", &cookie)?;
        }
        if let Some(referrer) = referrer_header(request) {
            headers.set("referer", &referrer)?;
        }
        if ((needs_cors_check(request) && request.mode == RequestMode::Cors)
            || !matches!(request.method.as_str(), "GET" | "HEAD"))
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
            self.store_response_cookie(request.url.as_str(), cookie)
                .map_err(FetchError::network)?;
        }
        Ok(())
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
    if source.is_secure() && !request.url.is_secure() {
        return None;
    }
    if source.origin().is_same_origin(&request.url.origin()) {
        Some(source.as_str().to_string())
    } else {
        Some(format!("{}/", source.origin().serialize()))
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

fn manual_redirect_response(
    request: FetchRequest,
    url_list: Vec<FetchUrl>,
    transport: TransportResponse,
) -> FetchResponse {
    if request.context == RequestContext::Script {
        return FetchResponse {
            response_type: ResponseType::OpaqueRedirect,
            url_list,
            status: 0,
            headers: HeaderList::new(),
            body: Body::from_bytes(Vec::new()),
        };
    }
    FetchResponse {
        response_type: ResponseType::Basic,
        url_list,
        status: transport.status,
        headers: transport.headers,
        body: transport.body,
    }
}

fn filtered_response(
    request: FetchRequest,
    url_list: Vec<FetchUrl>,
    transport: TransportResponse,
) -> FetchResponse {
    let cross_origin = needs_cors_check(&request);
    if request.context == RequestContext::Script
        && cross_origin
        && request.mode == RequestMode::NoCors
    {
        return FetchResponse {
            response_type: ResponseType::Opaque,
            url_list,
            status: 0,
            headers: HeaderList::new(),
            body: Body::from_bytes(Vec::new()),
        };
    }
    let response_type = if request.context == RequestContext::Script && cross_origin {
        ResponseType::Cors
    } else {
        ResponseType::Basic
    };
    let headers = if request.context == RequestContext::Script {
        cors_filtered_headers(&transport.headers, &request)
    } else {
        transport.headers
    };
    FetchResponse {
        response_type,
        url_list,
        status: transport.status,
        headers,
        body: transport.body,
    }
}
