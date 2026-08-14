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

#[test]
fn returns_http_error_responses_with_their_body() {
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
        let body = b"<h1>Try again later</h1>";
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
    });

    let url = format!("http://127.0.0.1:{port}/limited");
    let client = HttpClient::with_access_type(WINHTTP_ACCESS_TYPE_NO_PROXY).unwrap();
    let response = client.get(&url).unwrap();
    receiver.join().unwrap();
    assert_eq!(response.status, 429);
    assert!(!response.is_success());
    assert_eq!(response.content_type.as_deref(), Some("text/html"));
    assert_eq!(response.body, b"<h1>Try again later</h1>");
}
