//! Fetch policy orchestration whose final response body remains incremental.

use super::{
    HttpClient, fetch_data_url, is_redirect_status, needs_cors_check, needs_preflight,
    rewrite_redirect_method,
};
use crate::fetch::{
    Body, FetchError, FetchErrorKind, FetchRequest, FetchResponse, FetchUrl, HeaderList,
    RedirectMode, RequestContext, RequestMode, ResponseType, cors_filtered_headers,
    validate_cors_response,
};
use crate::limits::{MAX_FETCH_STREAM_CHUNK_BYTES, MAX_REDIRECTS};
use crate::winhttp::client::{TransportBodyStream, TransportRequest, TransportStreamResponse};

pub struct StreamingFetchResponse {
    pub response_type: ResponseType,
    pub url_list: Vec<FetchUrl>,
    pub status: u16,
    pub headers: HeaderList,
    body: StreamingBody,
    body_limit: usize,
}

enum StreamingBody {
    Network(TransportBodyStream),
    Memory { bytes: Vec<u8>, cursor: usize },
    Empty,
}

impl StreamingFetchResponse {
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FetchError> {
        match &mut self.body {
            StreamingBody::Network(body) => body.next_chunk(),
            StreamingBody::Memory { bytes, cursor } => {
                if *cursor == bytes.len() {
                    return Ok(None);
                }
                let end = cursor
                    .saturating_add(MAX_FETCH_STREAM_CHUNK_BYTES)
                    .min(bytes.len());
                let chunk = bytes[*cursor..end].to_vec();
                *cursor = end;
                Ok(Some(chunk))
            }
            StreamingBody::Empty => Ok(None),
        }
    }

    pub fn into_buffered(mut self) -> Result<FetchResponse, FetchError> {
        let mut body = Body::empty(self.body_limit);
        while let Some(chunk) = self.next_chunk()? {
            body.push(&chunk)?;
        }
        Ok(FetchResponse {
            response_type: self.response_type,
            url_list: self.url_list,
            status: self.status,
            headers: self.headers,
            body,
        })
    }

    fn from_buffered(response: FetchResponse, body_limit: usize) -> Self {
        Self {
            response_type: response.response_type,
            url_list: response.url_list,
            status: response.status,
            headers: response.headers,
            body: StreamingBody::Memory {
                bytes: response.body.into_bytes(),
                cursor: 0,
            },
            body_limit,
        }
    }
}

impl HttpClient {
    pub fn fetch_stream(
        &self,
        mut request: FetchRequest,
    ) -> Result<StreamingFetchResponse, FetchError> {
        request.validate()?;
        request.signal.check()?;
        let body_limit = request.response_body_limit;
        if request.url.is_data() {
            return fetch_data_url(request)
                .map(|response| StreamingFetchResponse::from_buffered(response, body_limit));
        }

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
            let transport = self.send_once_stream(TransportRequest {
                url: &request.url,
                method: &request.method,
                headers: &outbound_headers,
                body: request.body.as_ref(),
                response_body_limit: request.response_body_limit,
                signal: &request.signal,
                cache: request.cache,
            })?;
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
                        return Ok(manual_redirect(request, url_list, transport));
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

            return Ok(filtered(request, url_list, transport));
        }
        unreachable!("the bounded redirect loop always returns")
    }
}

fn manual_redirect(
    request: FetchRequest,
    url_list: Vec<FetchUrl>,
    transport: TransportStreamResponse,
) -> StreamingFetchResponse {
    if request.context == RequestContext::Script {
        return StreamingFetchResponse {
            response_type: ResponseType::OpaqueRedirect,
            url_list,
            status: 0,
            headers: HeaderList::new(),
            body: StreamingBody::Empty,
            body_limit: request.response_body_limit,
        };
    }
    StreamingFetchResponse {
        response_type: ResponseType::Basic,
        url_list,
        status: transport.status,
        headers: transport.headers,
        body: StreamingBody::Network(transport.body),
        body_limit: request.response_body_limit,
    }
}

fn filtered(
    request: FetchRequest,
    url_list: Vec<FetchUrl>,
    transport: TransportStreamResponse,
) -> StreamingFetchResponse {
    let cross_origin = needs_cors_check(&request);
    if request.context == RequestContext::Script
        && cross_origin
        && request.mode == RequestMode::NoCors
    {
        return StreamingFetchResponse {
            response_type: ResponseType::Opaque,
            url_list,
            status: 0,
            headers: HeaderList::new(),
            body: StreamingBody::Empty,
            body_limit: request.response_body_limit,
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
    StreamingFetchResponse {
        response_type,
        url_list,
        status: transport.status,
        headers,
        body: StreamingBody::Network(transport.body),
        body_limit: request.response_body_limit,
    }
}
