//! Native protocol negotiation with fallback for older Windows versions.
use super::ffi::*;
use std::io;

pub(super) fn configure(session: HInternet) -> Result<(), String> {
    negotiate(|mut protocols| {
        let ok = unsafe {
            WinHttpSetOption(
                session,
                WINHTTP_OPTION_ENABLE_HTTP_PROTOCOL,
                (&mut protocols as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
    .map_err(|error| format!("configure HTTP protocol negotiation: {error}"))
}

fn negotiate(mut set: impl FnMut(u32) -> io::Result<()>) -> io::Result<()> {
    // HTTP/1.1 remains enabled by WinHTTP. This is not HTTP_PROTOCOL_REQUIRED and
    // does not weaken TLS, certificate validation, or proxy policy.
    // https://learn.microsoft.com/windows/win32/winhttp/option-flags
    for protocols in [
        WINHTTP_PROTOCOL_FLAG_HTTP2 | WINHTTP_PROTOCOL_FLAG_HTTP3,
        WINHTTP_PROTOCOL_FLAG_HTTP2,
    ] {
        match set(protocols) {
            Ok(()) => return Ok(()),
            // Older Windows can reject a new flag or the option itself. Only capability
            // errors allow fallback; unexpected failures must remain actionable.
            Err(error) if matches!(error.raw_os_error(), Some(87 | 12009)) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(()) // Unmodified WinHTTP default on systems without the advanced option.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_both_modern_protocols_without_requiring_them() {
        let mut calls = Vec::new();
        negotiate(|mask| {
            calls.push(mask);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, [3]);
    }

    #[test]
    fn retries_http2_only_for_an_unsupported_modern_mask() {
        let mut calls = Vec::new();
        negotiate(|mask| {
            calls.push(mask);
            if mask == 3 {
                Err(io::Error::from_raw_os_error(87))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(calls, [3, 1]);
    }

    #[test]
    fn unsupported_option_preserves_legacy_default() {
        let mut calls = Vec::new();
        negotiate(|mask| {
            calls.push(mask);
            Err(io::Error::from_raw_os_error(12009))
        })
        .unwrap();
        assert_eq!(calls, [3, 1]);
    }

    #[test]
    fn unexpected_configuration_errors_are_not_hidden_by_fallback() {
        let mut calls = 0;
        let error = negotiate(|_| {
            calls += 1;
            Err(io::Error::from_raw_os_error(5))
        })
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
        assert_eq!(calls, 1);
    }
}
