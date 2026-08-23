//! Public HTTP client facade and one-hop bounded WinHTTP transport.

use super::cookies::CookieStore;
use super::ffi::*;
use crate::branding::USER_AGENT;
use crate::fetch::{
    Body, FetchError, FetchRequest, FetchResponse, FetchSignal, FetchUrl, HeaderList, RequestCache,
};
use crate::limits::MAX_FETCH_STREAM_CHUNK_BYTES;
use crate::navigation::ParsedUrl;
use std::collections::HashMap;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

pub type HttpResponse = FetchResponse;

pub(super) struct TransportResponse {
    pub status: u16,
    pub headers: HeaderList,
}

pub(super) struct TransportStreamResponse {
    pub status: u16,
    pub headers: HeaderList,
    pub body: TransportBodyStream,
}

pub(super) struct TransportBodyStream {
    request: InternetHandle,
    signal: FetchSignal,
    response_body_limit: usize,
    received: usize,
    finished: bool,
}

impl TransportBodyStream {
    pub(super) fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FetchError> {
        if self.finished {
            return Ok(None);
        }
        self.signal.check()?;
        // WinHttpReadData tries to fill a caller-provided buffer. Querying first is the
        // documented progressive-consumption path and prevents a small response from being
        // buffered to completion merely because our bounded IPC chunk is larger than it.
        let mut available = 0_u32;
        check(
            unsafe { WinHttpQueryDataAvailable(self.request.0, &mut available) },
            "query available response data",
        )
        .map_err(FetchError::network)?;
        let read_capacity = if available == 0 {
            // A zero query can represent end-of-body. ReadData remains the authoritative EOF
            // signal, as required by the WinHTTP contract.
            1
        } else {
            (available as usize).min(MAX_FETCH_STREAM_CHUNK_BYTES)
        };
        let mut buffer = vec![0_u8; read_capacity];
        let mut bytes_read = 0_u32;
        check(
            unsafe {
                WinHttpReadData(
                    self.request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut bytes_read,
                )
            },
            "read response",
        )
        .map_err(FetchError::network)?;
        if bytes_read == 0 {
            self.finished = true;
            return Ok(None);
        }
        buffer.truncate(bytes_read as usize);
        self.received = self
            .received
            .checked_add(buffer.len())
            .ok_or_else(|| FetchError::body_too_large(self.response_body_limit))?;
        if self.received > self.response_body_limit {
            return Err(FetchError::body_too_large(self.response_body_limit));
        }
        Ok(Some(buffer))
    }
}

pub(super) struct TransportRequest<'a> {
    pub url: &'a FetchUrl,
    pub method: &'a str,
    pub headers: &'a HeaderList,
    pub body: Option<&'a Body>,
    pub response_body_limit: usize,
    pub signal: &'a crate::fetch::FetchSignal,
    pub cache: RequestCache,
}

pub struct HttpClient {
    connections: Mutex<HashMap<(String, String, u16), Arc<InternetHandle>>>,
    pub(super) cookie_store: CookieStore,
    session: InternetHandle,
}

impl HttpClient {
    pub fn new() -> Result<Self, String> {
        Self::with_store(configured_access_type(), CookieStore::in_memory())
    }

    pub fn with_profile(profile_directory: &Path) -> Result<Self, String> {
        let store = CookieStore::open(profile_directory.join("cookies.json"))?;
        Self::with_store(configured_access_type(), store)
    }

    #[cfg(test)]
    pub(super) fn with_access_type(access_type: u32) -> Result<Self, String> {
        Self::with_store(access_type, CookieStore::in_memory())
    }

    fn with_store(access_type: u32, cookie_store: CookieStore) -> Result<Self, String> {
        let agent = wide(USER_AGENT);
        let session = InternetHandle::new(unsafe {
            WinHttpOpen(agent.as_ptr(), access_type, null(), null(), 0)
        })?;
        unsafe {
            WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 30_000);
        }
        Ok(Self {
            connections: Mutex::new(HashMap::new()),
            cookie_store,
            session,
        })
    }

    pub fn get(&self, url: &str) -> Result<HttpResponse, String> {
        let request = FetchRequest::navigation(url).map_err(|error| error.to_string())?;
        self.fetch(request).map_err(|error| error.to_string())
    }

    pub(super) fn send_once(
        &self,
        transport: TransportRequest<'_>,
    ) -> Result<TransportResponse, FetchError> {
        let response_body_limit = transport.response_body_limit;
        let mut streamed = self.send_once_stream(transport)?;
        let mut body = Body::empty(response_body_limit);
        while let Some(chunk) = streamed.body.next_chunk()? {
            body.push(&chunk)?;
        }
        Ok(TransportResponse {
            status: streamed.status,
            headers: streamed.headers,
        })
    }

    pub(super) fn send_once_stream(
        &self,
        transport: TransportRequest<'_>,
    ) -> Result<TransportStreamResponse, FetchError> {
        let TransportRequest {
            url,
            method,
            headers,
            body: request_body,
            response_body_limit,
            signal,
            cache,
        } = transport;
        signal.check()?;
        let parsed = url.parsed().ok_or_else(|| {
            FetchError::new(
                crate::fetch::FetchErrorKind::InvalidRequest,
                "WinHTTP only transports HTTP(S) URLs",
            )
        })?;
        let connection = self.connection(parsed).map_err(FetchError::network)?;
        let verb = wide(method);
        let object = wide(&parsed.path_and_query);
        let accept = wide(ACCEPT_TYPES);
        let accept_types = [accept.as_ptr(), null()];
        let mut flags = if parsed.scheme == "https" {
            WINHTTP_FLAG_SECURE
        } else {
            0
        };
        if matches!(cache, RequestCache::NoStore | RequestCache::Reload) {
            flags |= WINHTTP_FLAG_REFRESH;
        }
        let request = InternetHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                object.as_ptr(),
                null(),
                null(),
                accept_types.as_ptr(),
                flags,
            )
        })
        .map_err(FetchError::network)?;

        configure_request(request.0)?;
        let wire_headers = headers.to_wire_string();
        let wire_headers = wide(&wire_headers);
        let body = request_body.map(Body::as_bytes).unwrap_or_default();
        let header_length = u32::try_from(wire_headers.len().saturating_sub(1)).map_err(|_| {
            FetchError::new(
                crate::fetch::FetchErrorKind::InvalidRequest,
                "request headers exceed the WinHTTP size limit",
            )
        })?;
        let body_length = u32::try_from(body.len()).map_err(|_| {
            FetchError::new(
                crate::fetch::FetchErrorKind::InvalidRequest,
                "request body exceeds the WinHTTP size limit",
            )
        })?;
        let body_pointer = if body.is_empty() {
            null_mut()
        } else {
            body.as_ptr().cast_mut().cast()
        };
        signal.check()?;
        check(
            unsafe {
                WinHttpSendRequest(
                    request.0,
                    wire_headers.as_ptr(),
                    header_length,
                    body_pointer,
                    body_length,
                    body_length,
                    0,
                )
            },
            "send request",
        )
        .map_err(FetchError::network)?;
        signal.check()?;
        check(
            unsafe { WinHttpReceiveResponse(request.0, null_mut()) },
            "receive response",
        )
        .map_err(FetchError::network)?;
        signal.check()?;

        let status = query_status(request.0).map_err(FetchError::network)? as u16;
        let headers =
            parse_response_headers(&query_raw_headers(request.0).map_err(FetchError::network)?)?;
        Ok(TransportStreamResponse {
            status,
            headers,
            body: TransportBodyStream {
                request,
                signal: signal.clone(),
                response_body_limit,
                received: 0,
                finished: false,
            },
        })
    }

    fn connection(&self, parsed: &ParsedUrl) -> Result<Arc<InternetHandle>, String> {
        let key = (parsed.scheme.clone(), parsed.host.clone(), parsed.port);
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| "HTTP connection cache is unavailable".to_string())?;
        if let Some(connection) = connections.get(&key) {
            return Ok(Arc::clone(connection));
        }
        let host = wide(&parsed.host);
        let connection = Arc::new(InternetHandle::new(unsafe {
            WinHttpConnect(self.session.0, host.as_ptr(), parsed.port, 0)
        })?);
        connections.insert(key, Arc::clone(&connection));
        Ok(connection)
    }
}

fn configure_request(request: HInternet) -> Result<(), FetchError> {
    // Authentication must remain policy-controlled too; Breeze has no HTTP-auth credential store
    // yet, so allowing WinHTTP to select ambient credentials would violate CredentialsMode.
    let mut disabled =
        WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_AUTHENTICATION;
    let mut decompression = WINHTTP_DECOMPRESSION_FLAG_GZIP
        | WINHTTP_DECOMPRESSION_FLAG_DEFLATE
        | WINHTTP_DECOMPRESSION_FLAG_BROTLI;
    check(
        unsafe {
            WinHttpSetOption(
                request,
                WINHTTP_OPTION_DISABLE_FEATURE,
                (&mut disabled as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        },
        "disable automatic redirects, cookies, and authentication",
    )
    .map_err(FetchError::network)?;
    unsafe {
        WinHttpSetOption(
            request,
            WINHTTP_OPTION_DECOMPRESSION,
            (&mut decompression as *mut u32).cast(),
            size_of::<u32>() as u32,
        );
    }
    Ok(())
}

fn parse_response_headers(raw: &str) -> Result<HeaderList, FetchError> {
    let mut headers = HeaderList::new();
    for line in raw.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.append(name, value)?;
    }
    Ok(headers)
}

pub fn get(url: &str) -> Result<HttpResponse, String> {
    HttpClient::new()?.get(url)
}
