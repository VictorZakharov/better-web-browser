use super::*;

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpQueryOption(
        handle: HInternet,
        option: u32,
        buffer: *mut std::ffi::c_void,
        length: *mut u32,
    ) -> i32;
}

fn option(handle: HInternet, number: u32) -> u32 {
    let mut value = 0_u32;
    let mut length = size_of::<u32>() as u32;
    check(
        unsafe { WinHttpQueryOption(handle, number, (&mut value as *mut u32).cast(), &mut length) },
        "query protocol option",
    )
    .unwrap();
    assert_eq!(length, size_of::<u32>() as u32);
    value
}

#[test]
fn native_session_accepts_modern_protocol_configuration() {
    // ENABLE_HTTP_PROTOCOL is set-only; HTTP_PROTOCOL_USED is queried on a response.
    let _client = HttpClient::with_access_type(WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap();
}

#[test]
#[ignore = "live transport probe; set BREEZE_PROTOCOL_TEST_URL to an HTTPS endpoint"]
fn reports_actual_negotiated_protocol_without_requiring_http3() {
    let url = std::env::var("BREEZE_PROTOCOL_TEST_URL").expect("set HTTPS probe URL");
    assert!(url.starts_with("https://"));
    let client = HttpClient::new().unwrap();
    for attempt in 1..=3 {
        let request = FetchRequest::navigation(&url).unwrap();
        let started = std::time::Instant::now();
        let mut response = client
            .send_once_stream(TransportRequest {
                url: &request.url,
                method: &request.method,
                headers: &request.headers,
                body: request.body.as_ref(),
                response_body_limit: 16 * 1024 * 1024,
                signal: &request.signal,
                cache: request.cache,
            })
            .unwrap();
        let protocol = option(response.body.request.0, 134);
        let headers_ms = started.elapsed().as_secs_f64() * 1000.0;
        while response.body.next_chunk().unwrap().is_some() {}
        println!(
            "attempt={attempt} protocol_flags={protocol} status={} headers_ms={headers_ms:.1} total_ms={:.1}",
            response.status,
            started.elapsed().as_secs_f64() * 1000.0
        );
        assert!(matches!(protocol, 0..=2));
    }
}
