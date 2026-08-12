use crate::navigation::{ParsedUrl, UrlError};
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};

type HInternet = *mut c_void;

const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const WINHTTP_OPTION_URL: u32 = 34;
const WINHTTP_OPTION_DECOMPRESSION: u32 = 118;
const WINHTTP_DECOMPRESSION_FLAG_GZIP: u32 = 1;
const WINHTTP_DECOMPRESSION_FLAG_DEFLATE: u32 = 2;
const WINHTTP_DECOMPRESSION_FLAG_BROTLI: u32 = 4;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        user_agent: *const u16,
        access_type: u32,
        proxy_name: *const u16,
        proxy_bypass: *const u16,
        flags: u32,
    ) -> HInternet;
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

pub struct HttpResponse {
    pub body: Vec<u8>,
    pub final_url: String,
    pub status: u32,
}

pub fn get(url: &str) -> Result<HttpResponse, String> {
    let parsed = ParsedUrl::parse(url).map_err(|error| error.to_string())?;
    let agent = wide("Breeze/0.1 (+https://localhost)");
    let session = InternetHandle::new(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        )
    })?;

    unsafe {
        WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 30_000);
    }
    let host = wide(&parsed.host);
    let connection =
        InternetHandle::new(unsafe { WinHttpConnect(session.0, host.as_ptr(), parsed.port, 0) })?;

    let verb = wide("GET");
    let object = wide(&parsed.path_and_query);
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
            null(),
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

    check(
        unsafe { WinHttpSendRequest(request.0, null(), 0, null_mut(), 0, 0, 0) },
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
    })
}

pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
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

    #[test]
    fn decodes_utf_boms() {
        assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'o', b'k']), "ok");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'o', 0, b'k', 0]), "ok");
    }
}
