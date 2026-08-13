use crate::branding::USER_AGENT;
use crate::navigation::{ParsedUrl, UrlError};
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

type HInternet = *mut c_void;

const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_ACCESS_TYPE_NO_PROXY: u32 = 1;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_CONTENT_TYPE: u32 = 1;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const WINHTTP_OPTION_URL: u32 = 34;
const WINHTTP_OPTION_DECOMPRESSION: u32 = 118;
const WINHTTP_DECOMPRESSION_FLAG_GZIP: u32 = 1;
const WINHTTP_DECOMPRESSION_FLAG_DEFLATE: u32 = 2;
const WINHTTP_DECOMPRESSION_FLAG_BROTLI: u32 = 4;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const ACCEPT_TYPES: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        user_agent: *const u16,
        access_type: u32,
        proxy_name: *const u16,
        proxy_bypass: *const u16,
        flags: u32,
    ) -> HInternet;
    fn WinHttpGetDefaultProxyConfiguration(proxy_info: *mut WinHttpProxyInfo) -> i32;
    fn WinHttpConnect(
        session: HInternet,
        server_name: *const u16,
        server_port: u16,
        reserved: u32,
    ) -> HInternet;
    fn WinHttpOpenRequest(
        connection: HInternet,
        verb: *const u16,
        object_name: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: u32,
    ) -> HInternet;
    fn WinHttpSetTimeouts(
        internet: HInternet,
        resolve_timeout: i32,
        connect_timeout: i32,
        send_timeout: i32,
        receive_timeout: i32,
    ) -> i32;
    fn WinHttpSetOption(
        internet: HInternet,
        option: u32,
        buffer: *mut c_void,
        buffer_length: u32,
    ) -> i32;
    fn WinHttpSendRequest(
        request: HInternet,
        headers: *const u16,
        headers_length: u32,
        optional: *mut c_void,
        optional_length: u32,
        total_length: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: HInternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: HInternet,
        info_level: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_length: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpQueryOption(
        internet: HInternet,
        option: u32,
        buffer: *mut c_void,
        buffer_length: *mut u32,
    ) -> i32;
    fn WinHttpReadData(
        request: HInternet,
        buffer: *mut c_void,
        bytes_to_read: u32,
        bytes_read: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(internet: HInternet) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
}

#[repr(C)]
struct WinHttpProxyInfo {
    access_type: u32,
    proxy: *mut u16,
    proxy_bypass: *mut u16,
}

pub struct HttpResponse {
    pub body: Vec<u8>,
    pub final_url: String,
    pub status: u32,
    pub content_type: Option<String>,
}

pub struct HttpClient {
    connections: Mutex<HashMap<(String, String, u16), Arc<InternetHandle>>>,
    cookies: Mutex<Vec<StoredCookie>>,
    session: InternetHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    host_only: bool,
}

impl HttpClient {
    pub fn new() -> Result<Self, String> {
        Self::with_access_type(configured_access_type())
    }

    fn with_access_type(access_type: u32) -> Result<Self, String> {
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

    pub fn set_cookie(&self, document_url: &str, assignment: &str) -> Result<(), String> {
        let parsed = ParsedUrl::parse(document_url).map_err(|error| error.to_string())?;
        let Some((cookie, expired)) = parse_cookie(&parsed, assignment) else {
            return Ok(());
        };
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        cookies.retain(|stored| {
            stored.name != cookie.name
                || stored.domain != cookie.domain
                || stored.path != cookie.path
        });
        if !expired {
            cookies.push(cookie);
        }
        Ok(())
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
        if !(200..=299).contains(&status) {
            return Err(format!("server returned HTTP {status}"));
        }

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

    fn cookie_header(&self, parsed: &ParsedUrl) -> Result<Option<String>, String> {
        let cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        let mut matching = cookies
            .iter()
            .filter(|cookie| cookie_matches(cookie, parsed))
            .collect::<Vec<_>>();
        matching.sort_unstable_by_key(|cookie| std::cmp::Reverse(cookie.path.len()));
        if matching.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            "Cookie: {}\r\n",
            matching
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                .collect::<Vec<_>>()
                .join("; ")
        )))
    }
}

fn parse_cookie(parsed: &ParsedUrl, assignment: &str) -> Option<(StoredCookie, bool)> {
    let mut parts = assignment.split(';');
    let (name, value) = parts.next()?.trim().split_once('=')?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty()
        || name
            .bytes()
            .any(|byte| byte <= 0x20 || matches!(byte, b';' | b',' | b'='))
        || value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | b';'))
    {
        return None;
    }

    let host = parsed.host.to_ascii_lowercase();
    let mut domain = host.clone();
    let mut host_only = true;
    let mut path = default_cookie_path(&parsed.path_and_query);
    let mut secure = false;
    let mut expired = false;
    for attribute in parts {
        let attribute = attribute.trim();
        let (attribute_name, attribute_value) = attribute
            .split_once('=')
            .map(|(name, value)| (name.trim(), Some(value.trim())))
            .unwrap_or((attribute, None));
        if attribute_name.eq_ignore_ascii_case("domain") {
            let candidate = attribute_value?
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if candidate.is_empty()
                || (host != candidate && !host.ends_with(&format!(".{candidate}")))
            {
                return None;
            }
            domain = candidate;
            host_only = false;
        } else if attribute_name.eq_ignore_ascii_case("path") {
            if let Some(candidate) = attribute_value.filter(|value| value.starts_with('/')) {
                path = candidate.to_string();
            }
        } else if attribute_name.eq_ignore_ascii_case("secure") {
            secure = true;
        } else if attribute_name.eq_ignore_ascii_case("max-age") {
            expired = attribute_value
                .and_then(|value| value.parse::<i64>().ok())
                .is_some_and(|seconds| seconds <= 0);
        }
    }

    Some((
        StoredCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path,
            secure,
            host_only,
        },
        expired,
    ))
}

fn default_cookie_path(path_and_query: &str) -> String {
    let path = path_and_query.split('?').next().unwrap_or("/");
    if !path.starts_with('/') || path == "/" {
        return "/".into();
    }
    let Some((directory, _)) = path.rsplit_once('/') else {
        return "/".into();
    };
    if directory.is_empty() {
        "/".into()
    } else {
        directory.to_string()
    }
}

fn cookie_matches(cookie: &StoredCookie, parsed: &ParsedUrl) -> bool {
    if cookie.secure && parsed.scheme != "https" {
        return false;
    }
    let host = parsed.host.to_ascii_lowercase();
    let domain_matches = if cookie.host_only {
        host == cookie.domain
    } else {
        host == cookie.domain || host.ends_with(&format!(".{}", cookie.domain))
    };
    if !domain_matches {
        return false;
    }
    let request_path = parsed.path_and_query.split('?').next().unwrap_or("/");
    request_path == cookie.path
        || (request_path.starts_with(&cookie.path)
            && (cookie.path.ends_with('/')
                || request_path.as_bytes().get(cookie.path.len()) == Some(&b'/')))
}

fn configured_access_type() -> u32 {
    let mut info = WinHttpProxyInfo {
        access_type: WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        proxy: null_mut(),
        proxy_bypass: null_mut(),
    };
    let success = unsafe { WinHttpGetDefaultProxyConfiguration(&mut info) } != 0;
    let access_type = if success && info.access_type == WINHTTP_ACCESS_TYPE_NO_PROXY {
        WINHTTP_ACCESS_TYPE_NO_PROXY
    } else {
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
    };
    unsafe {
        if !info.proxy.is_null() {
            GlobalFree(info.proxy.cast());
        }
        if !info.proxy_bypass.is_null() {
            GlobalFree(info.proxy_bypass.cast());
        }
    }
    access_type
}

pub fn get(url: &str) -> Result<HttpResponse, String> {
    HttpClient::new()?.get(url)
}

pub fn decode_text(bytes: &[u8], content_type: Option<&str>) -> String {
    if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_with_encoding(bytes, UTF_8);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_with_encoding(bytes, UTF_16LE);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_with_encoding(bytes, UTF_16BE);
    }
    let encoding = content_type
        .and_then(charset_from_content_type)
        .or_else(|| sniff_meta_charset(bytes))
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    decode_with_encoding(bytes, encoding)
}

fn decode_with_encoding(bytes: &[u8], encoding: &'static Encoding) -> String {
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

fn charset_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(['\'', '"']).to_ascii_lowercase())
    })
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let prefix = &bytes[..bytes.len().min(1024)];
    let ascii = prefix
        .iter()
        .map(|byte| (*byte as char).to_ascii_lowercase())
        .collect::<String>();
    let charset = ascii.find("charset")?;
    let after = ascii[charset + "charset".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let after = after.trim_start_matches(['\'', '"']);
    let end = after
        .find(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '\'' | '"' | ';' | '>')
        })
        .unwrap_or(after.len());
    (!after[..end].is_empty()).then(|| after[..end].to_string())
}

fn query_status(request: HInternet) -> Result<u32, String> {
    let mut status = 0_u32;
    let mut length = size_of::<u32>() as u32;
    check(
        unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut length,
                null_mut(),
            )
        },
        "read response status",
    )?;
    Ok(status)
}

fn query_final_url(request: HInternet) -> Option<String> {
    let mut bytes = 0_u32;
    unsafe {
        WinHttpQueryOption(request, WINHTTP_OPTION_URL, null_mut(), &mut bytes);
    }
    if bytes < 2 {
        return None;
    }
    let mut buffer = vec![0_u16; (bytes as usize).div_ceil(2)];
    if unsafe {
        WinHttpQueryOption(
            request,
            WINHTTP_OPTION_URL,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    } == 0
    {
        return None;
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn query_header_string(request: HInternet, query: u32) -> Option<String> {
    let mut bytes = 0_u32;
    unsafe {
        WinHttpQueryHeaders(request, query, null(), null_mut(), &mut bytes, null_mut());
    }
    if bytes < 2 {
        return None;
    }
    let mut buffer = vec![0_u16; (bytes as usize).div_ceil(2)];
    if unsafe {
        WinHttpQueryHeaders(
            request,
            query,
            null(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
            null_mut(),
        )
    } == 0
    {
        return None;
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn check(success: i32, operation: &str) -> Result<(), String> {
    if success != 0 {
        Ok(())
    } else {
        Err(format!(
            "failed to {operation}: {}",
            io::Error::last_os_error()
        ))
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

struct InternetHandle(HInternet);

// SAFETY: WinHTTP permits independent request handles to be created and used concurrently from a
// shared session/connection hierarchy. Breeze never closes a parent until all scoped request
// workers have joined, and each worker exclusively owns its request handle.
unsafe impl Send for InternetHandle {}
unsafe impl Sync for InternetHandle {}

impl InternetHandle {
    fn new(handle: HInternet) -> Result<Self, String> {
        if handle.is_null() {
            Err(io::Error::last_os_error().to_string())
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for InternetHandle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}

impl From<UrlError> for String {
    fn from(error: UrlError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn decodes_utf_boms() {
        assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'o', b'k'], None), "ok");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'o', 0, b'k', 0], None), "ok");
    }

    #[test]
    fn honors_http_charset_before_meta() {
        assert_eq!(
            decode_text(b"Fran\xe7ais", Some("text/html; charset=ISO-8859-1")),
            "Français"
        );
        assert_eq!(
            decode_text(
                b"<meta charset=windows-1252>\x93quoted\x94",
                Some("text/html")
            ),
            "<meta charset=windows-1252>“quoted”"
        );
    }

    #[test]
    fn parses_and_scopes_javascript_cookies() {
        let origin = ParsedUrl::parse("https://www.google.com/search?q=test").unwrap();
        let (cookie, expired) = parse_cookie(
            &origin,
            "SG_SS=proof-token; Domain=.google.com; Path=/; Secure; SameSite=None",
        )
        .unwrap();
        assert!(!expired);
        assert_eq!(cookie.name, "SG_SS");
        assert_eq!(cookie.domain, "google.com");
        assert!(!cookie.host_only);
        assert!(cookie_matches(
            &cookie,
            &ParsedUrl::parse("https://www.google.com/search?sg_ss=proof-token").unwrap()
        ));
        assert!(!cookie_matches(
            &cookie,
            &ParsedUrl::parse("http://www.google.com/search").unwrap()
        ));
        assert!(!cookie_matches(
            &cookie,
            &ParsedUrl::parse("https://example.com/search").unwrap()
        ));
    }

    #[test]
    fn rejects_cookie_header_injection_and_foreign_domains() {
        let origin = ParsedUrl::parse("https://www.google.com/").unwrap();
        assert!(parse_cookie(&origin, "safe=value\r\nX-Evil: yes").is_none());
        assert!(parse_cookie(&origin, "safe=value; Domain=example.com").is_none());
    }

    #[test]
    fn sends_javascript_cookies_on_the_next_http_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
            String::from_utf8(request).unwrap()
        });

        let url = format!("http://127.0.0.1:{port}/search");
        let client = HttpClient::with_access_type(WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap();
        client
            .set_cookie(&url, "bridge=proof-token; Path=/")
            .unwrap();
        assert_eq!(client.get(&url).unwrap().body, b"ok");
        let request = receiver.join().unwrap();
        assert!(
            request.contains("Cookie: bridge=proof-token\r\n"),
            "{request}"
        );
        assert!(
            request.contains(&format!("Accept: {ACCEPT_TYPES}\r\n")),
            "{request}"
        );
        assert!(
            request.contains("Accept-Language: en-CA,en;q=0.9\r\n"),
            "{request}"
        );
    }
}
