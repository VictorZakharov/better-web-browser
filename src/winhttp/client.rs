//! Public HTTP client facade and bounded WinHTTP request transport.

use super::cookies::StoredCookie;
use super::ffi::*;
use crate::branding::USER_AGENT;
use crate::navigation::ParsedUrl;
use std::collections::HashMap;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

pub struct HttpResponse {
    pub body: Vec<u8>,
    pub final_url: String,
    pub status: u32,
    pub content_type: Option<String>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..=299).contains(&self.status)
    }
}

pub struct HttpClient {
    connections: Mutex<HashMap<(String, String, u16), Arc<InternetHandle>>>,
    pub(super) cookies: Mutex<Vec<StoredCookie>>,
    session: InternetHandle,
}

impl HttpClient {
    pub fn new() -> Result<Self, String> {
        Self::with_access_type(configured_access_type())
    }

    pub(super) fn with_access_type(access_type: u32) -> Result<Self, String> {
        let agent = wide(USER_AGENT);
        let session = InternetHandle::new(unsafe {
            WinHttpOpen(agent.as_ptr(), access_type, null(), null(), 0)
        })?;

        unsafe {
            WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 30_000);
        }
        Ok(Self {
            connections: Mutex::new(HashMap::new()),
            cookies: Mutex::new(Vec::new()),
            session,
        })
    }

    pub fn get(&self, url: &str) -> Result<HttpResponse, String> {
        let parsed = ParsedUrl::parse(url).map_err(|error| error.to_string())?;
        self.get_parsed(url, &parsed)
    }

    fn get_parsed(&self, url: &str, parsed: &ParsedUrl) -> Result<HttpResponse, String> {
        let connection = self.connection(parsed)?;

        let verb = wide("GET");
        let object = wide(&parsed.path_and_query);
        let accept = wide(ACCEPT_TYPES);
        let accept_types = [accept.as_ptr(), null()];
        let flags = if parsed.scheme == "https" {
            WINHTTP_FLAG_SECURE
        } else {
            0
        };
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
        })?;

        let mut decompression = WINHTTP_DECOMPRESSION_FLAG_GZIP
            | WINHTTP_DECOMPRESSION_FLAG_DEFLATE
            | WINHTTP_DECOMPRESSION_FLAG_BROTLI;
        unsafe {
            WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_DECOMPRESSION,
                (&mut decompression as *mut u32).cast(),
                size_of::<u32>() as u32,
            );
        }

        let mut request_headers = "Accept-Language: en-CA,en;q=0.9\r\n".to_string();
        if let Some(cookie_header) = self.cookie_header(parsed)? {
            request_headers.push_str(&cookie_header);
        }
        let request_headers = wide(&request_headers);
        check(
            unsafe {
                WinHttpSendRequest(
                    request.0,
                    request_headers.as_ptr(),
                    request_headers.len().saturating_sub(1) as u32,
                    null_mut(),
                    0,
                    0,
                    0,
                )
            },
            "send request",
        )?;
        check(
            unsafe { WinHttpReceiveResponse(request.0, null_mut()) },
            "receive response",
        )?;

        let status = query_status(request.0)?;
        let final_url = query_final_url(request.0).unwrap_or_else(|| url.to_string());
        let content_type = query_header_string(request.0, WINHTTP_QUERY_CONTENT_TYPE);
        let mut body = Vec::with_capacity(32 * 1024);
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let mut bytes_read = 0_u32;
            check(
                unsafe {
                    WinHttpReadData(
                        request.0,
                        buffer.as_mut_ptr().cast(),
                        buffer.len() as u32,
                        &mut bytes_read,
                    )
                },
                "read response",
            )?;
            if bytes_read == 0 {
                break;
            }
            if body.len() + bytes_read as usize > MAX_RESPONSE_BYTES {
                return Err(format!(
                    "document exceeds the MVP limit of {} MiB",
                    MAX_RESPONSE_BYTES / 1024 / 1024
                ));
            }
            body.extend_from_slice(&buffer[..bytes_read as usize]);
        }

        Ok(HttpResponse {
            body,
            final_url,
            status,
            content_type,
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

pub fn get(url: &str) -> Result<HttpResponse, String> {
    HttpClient::new()?.get(url)
}
