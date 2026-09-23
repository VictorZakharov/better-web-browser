//! Browser-owned WebSocket transport. The renderer never receives a socket handle.
//!
//! WinHTTP performs the RFC 6455 opening handshake and framing. Breeze supplies
//! the page's authoritative Origin and cookie policy, validates the negotiated
//! subprotocol, and bounds complete messages before forwarding them over IPC.
//! https://websockets.spec.whatwg.org/#the-websocket-interface

use super::client::{HttpClient, configure_request};
use super::ffi::*;
use crate::fetch::csp::PolicyContainer;
use crate::fetch::{CredentialsMode, FetchRequest};
use crate::limits::{MAX_URL_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES};
use crate::navigation::ParsedUrl;
use std::io;
use std::ptr::{null, null_mut};
use std::sync::Arc;

const RECEIVE_CHUNK_BYTES: usize = 32 * 1024;
const BINARY_MESSAGE: u32 = 0;
const BINARY_FRAGMENT: u32 = 1;
const UTF8_MESSAGE: u32 = 2;
const UTF8_FRAGMENT: u32 = 3;
const CLOSE_BUFFER: u32 = 4;

#[derive(Debug, PartialEq, Eq)]
pub enum WebSocketFrame {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
}

pub struct WebSocketConnection {
    handle: Arc<InternetHandle>,
    // WinHTTP parents must outlive the socket handle. The client's cached
    // session remains owned by the browser for the entire document lifetime.
    _connection: Arc<InternetHandle>,
}

pub struct WebSocketOpenResult {
    pub connection: WebSocketConnection,
    pub protocol: String,
}

struct SocketUrl {
    public: String,
    transport: ParsedUrl,
}

impl SocketUrl {
    fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > MAX_URL_BYTES {
            return Err("WebSocket URL exceeds the browser limit".into());
        }
        let mut url = url::Url::parse(raw).map_err(|_| "Invalid WebSocket URL")?;
        if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
            return Err("WebSocket URLs cannot contain credentials or fragments".into());
        }
        let secure = match url.scheme() {
            "ws" | "http" => false,
            "wss" | "https" => true,
            _ => return Err("WebSocket URL must use ws: or wss:".into()),
        };
        url.set_scheme(if secure { "wss" } else { "ws" })
            .map_err(|_| "Invalid WebSocket URL scheme")?;
        let public = url.to_string();
        url.set_scheme(if secure { "https" } else { "http" })
            .map_err(|_| "Invalid WebSocket transport scheme")?;
        let transport = ParsedUrl::parse(url.as_str()).map_err(|error| error.to_string())?;
        Ok(Self { public, transport })
    }
}

fn valid_protocol(protocol: &str) -> bool {
    !protocol.is_empty()
        && protocol.bytes().all(|byte| {
            matches!(byte, 0x21..=0x7e)
                && !matches!(
                    byte,
                    b'(' | b')'
                        | b'<'
                        | b'>'
                        | b'@'
                        | b','
                        | b';'
                        | b':'
                        | b'\\'
                        | b'"'
                        | b'/'
                        | b'['
                        | b']'
                        | b'?'
                        | b'='
                        | b'{'
                        | b'}'
                )
        })
}

fn selected_protocol(raw_headers: &str) -> Result<String, String> {
    let mut selected = None;
    for line in raw_headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("sec-websocket-protocol") {
            if selected.is_some() {
                return Err("WebSocket response repeated the subprotocol header".into());
            }
            let value = value.trim();
            if !valid_protocol(value) {
                return Err("WebSocket response selected an invalid subprotocol".into());
            }
            selected = Some(value.to_string());
        }
    }
    Ok(selected.unwrap_or_default())
}

impl HttpClient {
    /// Opens one bounded browser-owned connection. URL, Origin and offered
    /// protocols are independently revalidated even when renderer IPC did so.
    pub fn open_websocket(
        &self,
        raw_url: &str,
        document_url: &str,
        origin: &str,
        policy: &PolicyContainer,
        protocols: &[String],
    ) -> Result<WebSocketOpenResult, String> {
        let url = SocketUrl::parse(raw_url)?;
        if protocols.len() > 32
            || protocols.iter().any(|protocol| !valid_protocol(protocol))
            || protocols.iter().enumerate().any(|(index, protocol)| {
                protocols[..index].iter().any(|earlier| earlier == protocol)
            })
        {
            return Err("Invalid WebSocket subprotocol list".into());
        }
        let document = url::Url::parse(document_url).map_err(|_| "Invalid document origin")?;
        if !matches!(document.scheme(), "http" | "https") {
            return Err("WebSocket requires a network document origin".into());
        }
        if document.scheme() == "https" && url.transport.scheme != "https" {
            return Err("Mixed-content WebSocket was blocked".into());
        }
        if !policy.allows_url("connect-src", &url.public, 0) {
            return Err("Content Security Policy blocked the WebSocket connection".into());
        }
        if origin != "null" && origin != document.origin().ascii_serialization() {
            return Err("WebSocket client origin does not match its document".into());
        }
        let connection = self.connection(&url.transport)?;
        let verb = wide("GET");
        let object = wide(&url.transport.path_and_query);
        let request = InternetHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                object.as_ptr(),
                null(),
                null(),
                null(),
                if url.transport.scheme == "https" {
                    WINHTTP_FLAG_SECURE
                } else {
                    0
                },
            )
        })?;
        configure_request(request.0).map_err(|error| error.to_string())?;
        check(
            unsafe {
                WinHttpSetOption(
                    request.0,
                    WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET,
                    null_mut(),
                    0,
                )
            },
            "enable WebSocket upgrade",
        )?;
        let mut headers = format!("Origin: {origin}\r\n");
        if !protocols.is_empty() {
            headers.push_str("Sec-WebSocket-Protocol: ");
            headers.push_str(&protocols.join(", "));
            headers.push_str("\r\n");
        }
        let http_url = url.public.replacen(
            if url.transport.scheme == "https" {
                "wss:"
            } else {
                "ws:"
            },
            if url.transport.scheme == "https" {
                "https:"
            } else {
                "http:"
            },
            1,
        );
        let mut cookie_request =
            FetchRequest::script(&http_url, document_url).map_err(|error| error.to_string())?;
        cookie_request.credentials = CredentialsMode::Include;
        if let Some(cookies) = self.cookie_header_value(&cookie_request)? {
            headers.push_str("Cookie: ");
            headers.push_str(&cookies);
            headers.push_str("\r\n");
        }
        let headers = wide(&headers);
        check(
            unsafe {
                WinHttpSendRequest(
                    request.0,
                    headers.as_ptr(),
                    (headers.len() - 1) as u32,
                    null_mut(),
                    0,
                    0,
                    0,
                )
            },
            "send WebSocket opening handshake",
        )?;
        check(
            unsafe { WinHttpReceiveResponse(request.0, null_mut()) },
            "receive WebSocket opening handshake",
        )?;
        if query_status(request.0)? != 101 {
            return Err("WebSocket server did not accept the opening handshake".into());
        }
        let protocol = selected_protocol(&query_raw_headers(request.0)?)?;
        if !protocol.is_empty() && !protocols.contains(&protocol) {
            return Err("WebSocket server selected an unoffered subprotocol".into());
        }
        let socket = InternetHandle::new(unsafe { WinHttpWebSocketCompleteUpgrade(request.0, 0) })?;
        Ok(WebSocketOpenResult {
            connection: WebSocketConnection {
                handle: Arc::new(socket),
                _connection: connection,
            },
            protocol,
        })
    }
}

impl WebSocketConnection {
    pub fn send_text(&self, text: &str) -> Result<(), String> {
        self.send(UTF8_MESSAGE, text.as_bytes())
    }

    pub fn send_binary(&self, bytes: &[u8]) -> Result<(), String> {
        self.send(BINARY_MESSAGE, bytes)
    }

    fn send(&self, kind: u32, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
            return Err("WebSocket message exceeds the browser limit".into());
        }
        let result = unsafe {
            WinHttpWebSocketSend(
                self.handle.0,
                kind,
                bytes.as_ptr().cast_mut().cast(),
                bytes.len() as u32,
            )
        };
        socket_result(result, "send WebSocket message")
    }

    pub fn receive(&self) -> Result<WebSocketFrame, String> {
        let mut assembled = Vec::new();
        let mut expected_kind = None;
        loop {
            let mut chunk = [0_u8; RECEIVE_CHUNK_BYTES];
            let mut bytes_read = 0;
            let mut kind = 0;
            let result = unsafe {
                WinHttpWebSocketReceive(
                    self.handle.0,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut bytes_read,
                    &mut kind,
                )
            };
            socket_result(result, "receive WebSocket message")?;
            if kind == CLOSE_BUFFER {
                let mut code = 1005_u16;
                let mut reason = [0_u8; 123];
                let mut reason_length = 0_u32;
                let result = unsafe {
                    WinHttpWebSocketQueryCloseStatus(
                        self.handle.0,
                        &mut code,
                        reason.as_mut_ptr().cast(),
                        reason.len() as u32,
                        &mut reason_length,
                    )
                };
                socket_result(result, "read WebSocket close status")?;
                let reason = std::str::from_utf8(&reason[..reason_length as usize])
                    .map_err(|_| "WebSocket close reason was not UTF-8")?;
                return Ok(WebSocketFrame::Close {
                    code,
                    reason: reason.to_string(),
                });
            }
            let binary = match kind {
                BINARY_MESSAGE | BINARY_FRAGMENT => true,
                UTF8_MESSAGE | UTF8_FRAGMENT => false,
                _ => return Err("WinHTTP returned an invalid WebSocket buffer type".into()),
            };
            if expected_kind.is_some_and(|earlier| earlier != binary) {
                return Err("WebSocket fragments changed message type".into());
            }
            expected_kind = Some(binary);
            if assembled.len() + bytes_read as usize > MAX_WEBSOCKET_MESSAGE_BYTES {
                return Err("WebSocket message exceeds the browser limit".into());
            }
            assembled.extend_from_slice(&chunk[..bytes_read as usize]);
            if kind == BINARY_FRAGMENT || kind == UTF8_FRAGMENT {
                continue;
            }
            return if binary {
                Ok(WebSocketFrame::Binary(assembled))
            } else {
                String::from_utf8(assembled)
                    .map(WebSocketFrame::Text)
                    .map_err(|_| "WebSocket text message was not UTF-8".into())
            };
        }
    }

    /// Send a close frame on the send side; the concurrent receive side then
    /// observes its peer's close. Callers join both sides before dropping it.
    pub fn shutdown(&self, code: u16, reason: &str) -> Result<(), String> {
        if reason.len() > 123 {
            return Err("WebSocket close reason exceeds 123 UTF-8 bytes".into());
        }
        socket_result(
            unsafe {
                WinHttpWebSocketShutdown(
                    self.handle.0,
                    code,
                    if reason.is_empty() {
                        null_mut()
                    } else {
                        reason.as_ptr().cast_mut().cast()
                    },
                    reason.len() as u32,
                )
            },
            "send WebSocket close frame",
        )
    }
}

fn socket_result(result: u32, operation: &str) -> Result<(), String> {
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "failed to {operation}: {}",
            io::Error::from_raw_os_error(result as i32)
        ))
    }
}

#[cfg(test)]
mod tests;
