use super::*;
use crate::branding::{USER_AGENT, UserAgentMode};
use crate::navigation::ParsedUrl;
use crate::winhttp::cookies::{cookie_matches, parse_cookie};
use crate::winhttp::ffi::{ACCEPT_TYPES, WINHTTP_ACCESS_TYPE_NO_PROXY};
use std::io::{Read, Write};
use std::net::TcpListener;

mod cache;
mod fetch_pipeline;
mod prefetch;
mod support;

#[test]
fn selected_user_agent_is_sent_on_the_wire() {
    for mode in [UserAgentMode::Chrome, UserAgentMode::Firefox] {
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
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        let client =
            HttpClient::with_access_type_and_user_agent(WINHTTP_ACCESS_TYPE_NO_PROXY, mode)
                .unwrap();
        client.get(&format!("http://127.0.0.1:{port}/")).unwrap();
        let request = receiver.join().unwrap();
        assert!(
            request.to_ascii_lowercase().contains(&format!(
                "user-agent: {}\r\n",
                mode.user_agent().to_ascii_lowercase()
            )),
            "selected mode {mode:?} was not sent: {request}"
        );
    }
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
    assert!(parse_cookie(&origin, "hidden=value; HttpOnly").is_none());
    assert!(
        parse_cookie(
            &ParsedUrl::parse("http://www.google.com/").unwrap(),
            "secure=value; Secure"
        )
        .is_none()
    );
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
    assert_eq!(client.get(&url).unwrap().body.as_bytes(), b"ok");
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
    assert!(
        request.to_ascii_lowercase().contains(&format!(
            "user-agent: {}\r\n",
            USER_AGENT.to_ascii_lowercase()
        )),
        "{request}"
    );
    assert!(
        request.contains("sec-fetch-dest: document\r\n"),
        "{request}"
    );
    assert!(
        request.contains("sec-fetch-mode: navigate\r\n"),
        "{request}"
    );
    assert!(request.contains("sec-fetch-site: none\r\n"), "{request}");
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
    assert_eq!(response.content_type(), Some("text/html"));
    assert_eq!(response.body.as_bytes(), b"<h1>Try again later</h1>");
}
