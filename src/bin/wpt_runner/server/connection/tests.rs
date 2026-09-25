use super::*;

struct FailingConnection(ErrorKind);

impl Read for FailingConnection {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(self.0))
    }
}

impl Write for FailingConnection {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(self.0))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn canceled_browser_requests_are_not_server_failures() {
    for kind in [
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::BrokenPipe,
        ErrorKind::NotConnected,
    ] {
        assert_eq!(read_request(&mut FailingConnection(kind)).unwrap(), None);
        assert!(
            write_response(
                &mut FailingConnection(kind),
                200,
                "text/plain",
                b"ok",
                false
            )
            .is_ok()
        );
    }
    assert_eq!(read_request(&mut std::io::Cursor::new([])).unwrap(), None);
    #[cfg(windows)]
    assert!(is_client_disconnect(&std::io::Error::from_raw_os_error(10054)));
}

#[test]
fn unrelated_request_and_response_errors_remain_failures() {
    let read_error = read_request(&mut FailingConnection(ErrorKind::PermissionDenied)).unwrap_err();
    assert!(read_error.starts_with("read WPT request:"));
    let write_error = write_response(
        &mut FailingConnection(ErrorKind::PermissionDenied),
        200,
        "text/plain",
        b"ok",
        false,
    )
    .unwrap_err();
    assert!(write_error.starts_with("write WPT response:"));
}
