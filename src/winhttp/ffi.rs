//! WinHTTP FFI declarations, configuration, response queries, and owned handles.

use crate::navigation::UrlError;
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};

pub(super) type HInternet = *mut c_void;

pub(super) const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
pub(super) const WINHTTP_ACCESS_TYPE_NO_PROXY: u32 = 1;
pub(super) const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
pub(super) const WINHTTP_FLAG_REFRESH: u32 = 0x0000_0100;
pub(super) const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
pub(super) const WINHTTP_QUERY_RAW_HEADERS_CRLF: u32 = 22;
pub(super) const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
pub(super) const WINHTTP_OPTION_DECOMPRESSION: u32 = 118;
pub(super) const WINHTTP_OPTION_DISABLE_FEATURE: u32 = 63;
pub(super) const WINHTTP_DISABLE_COOKIES: u32 = 1;
pub(super) const WINHTTP_DISABLE_REDIRECTS: u32 = 2;
pub(super) const WINHTTP_DISABLE_AUTHENTICATION: u32 = 4;
pub(super) const WINHTTP_DECOMPRESSION_FLAG_GZIP: u32 = 1;
pub(super) const WINHTTP_DECOMPRESSION_FLAG_DEFLATE: u32 = 2;
pub(super) const WINHTTP_DECOMPRESSION_FLAG_BROTLI: u32 = 4;
pub(super) const ACCEPT_TYPES: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

#[link(name = "winhttp")]
unsafe extern "system" {
    pub(super) fn WinHttpOpen(
        user_agent: *const u16,
        access_type: u32,
        proxy_name: *const u16,
        proxy_bypass: *const u16,
        flags: u32,
    ) -> HInternet;
    fn WinHttpGetDefaultProxyConfiguration(proxy_info: *mut WinHttpProxyInfo) -> i32;
    pub(super) fn WinHttpConnect(
        session: HInternet,
        server_name: *const u16,
        server_port: u16,
        reserved: u32,
    ) -> HInternet;
    pub(super) fn WinHttpOpenRequest(
        connection: HInternet,
        verb: *const u16,
        object_name: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: u32,
    ) -> HInternet;
    pub(super) fn WinHttpSetTimeouts(
        internet: HInternet,
        resolve_timeout: i32,
        connect_timeout: i32,
        send_timeout: i32,
        receive_timeout: i32,
    ) -> i32;
    pub(super) fn WinHttpSetOption(
        internet: HInternet,
        option: u32,
        buffer: *mut c_void,
        buffer_length: u32,
    ) -> i32;
    pub(super) fn WinHttpSendRequest(
        request: HInternet,
        headers: *const u16,
        headers_length: u32,
        optional: *mut c_void,
        optional_length: u32,
        total_length: u32,
        context: usize,
    ) -> i32;
    pub(super) fn WinHttpReceiveResponse(request: HInternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: HInternet,
        info_level: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_length: *mut u32,
        index: *mut u32,
    ) -> i32;
    pub(super) fn WinHttpQueryDataAvailable(request: HInternet, bytes_available: *mut u32) -> i32;
    pub(super) fn WinHttpReadData(
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

pub(super) fn configured_access_type() -> u32 {
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

pub(super) fn query_status(request: HInternet) -> Result<u32, String> {
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

pub(super) fn query_raw_headers(request: HInternet) -> Result<String, String> {
    let mut bytes = 0_u32;
    unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            null(),
            null_mut(),
            &mut bytes,
            null_mut(),
        );
    }
    if bytes < 2 {
        return Ok(String::new());
    }
    let mut buffer = vec![0_u16; (bytes as usize).div_ceil(2)];
    check(
        unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                null(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
                null_mut(),
            )
        },
        "read response headers",
    )?;
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..length]))
}

pub(super) fn check(success: i32, operation: &str) -> Result<(), String> {
    if success != 0 {
        Ok(())
    } else {
        Err(format!(
            "failed to {operation}: {}",
            io::Error::last_os_error()
        ))
    }
}

pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub(super) struct InternetHandle(pub(super) HInternet);

// SAFETY: WinHTTP permits independent request handles to be created and used concurrently from a
// shared session/connection hierarchy. Breeze never closes a parent until all scoped request
// workers have joined, and each worker exclusively owns its request handle.
unsafe impl Send for InternetHandle {}
unsafe impl Sync for InternetHandle {}

impl InternetHandle {
    pub(super) fn new(handle: HInternet) -> Result<Self, String> {
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
