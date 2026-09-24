use super::*;
use crate::branding::UserAgentMode;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

#[test]
fn websocket_url_normalizes_http_schemes_and_rejects_credentials() {
    let parsed = SocketUrl::parse("https://example.test:8443/a?q=1").unwrap();
    assert_eq!(parsed.public, "wss://example.test:8443/a?q=1");
    assert_eq!(parsed.transport.scheme, "https");
    assert_eq!(parsed.transport.port, 8443);
    assert!(SocketUrl::parse("wss://a.test/path#fragment").is_err());
    assert!(SocketUrl::parse("ws://user@a.test/").is_err());
    assert!(SocketUrl::parse("ftp://a.test/").is_err());
}

#[test]
fn negotiated_subprotocol_must_be_a_single_valid_token() {
    assert!(valid_protocol("chat.v2"));
    assert!(!valid_protocol("one, two"));
    assert!(!valid_protocol("space name"));
    assert_eq!(
        selected_protocol(
            "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Protocol: chat.v2\r\n"
        )
        .unwrap(),
        "chat.v2"
    );
    assert!(selected_protocol(
        "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Protocol: chat\r\nSec-WebSocket-Protocol: other\r\n"
    ).is_err());
}

#[test]
fn loopback_websocket_negotiates_and_exchanges_text_binary_and_close_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request = read_headers(&mut stream);
        assert!(request.contains("Origin: http://127.0.0.1:"));
        assert!(request.contains("Sec-WebSocket-Protocol: chat"));
        let key = request
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-key"))
                    .map(|(_, value)| value.trim())
            })
            .expect("WinHTTP supplied a WebSocket key");
        let accept = base64(&sha1(
            format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
        ));
        stream.write_all(format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Protocol: chat\r\n\r\n"
        ).as_bytes()).unwrap();
        assert_eq!(read_client_frame(&mut stream), (1, b"hello".to_vec()));
        stream
            .write_all(&[0x81, 5, b'h', b'e', b'l', b'l', b'o'])
            .unwrap();
        assert_eq!(read_client_frame(&mut stream), (2, vec![0, 255, 42]));
        stream.write_all(&[0x82, 3, 0, 255, 42]).unwrap();
        assert_eq!(read_client_frame(&mut stream).0, 8);
        stream.write_all(&[0x88, 2, 0x03, 0xe8]).unwrap();
    });

    let client = HttpClient::with_access_type_and_user_agent(
        WINHTTP_ACCESS_TYPE_NO_PROXY,
        UserAgentMode::Breeze,
    )
    .unwrap();
    let origin = format!("http://{address}");
    let opened = client
        .open_websocket(
            &format!("ws://{address}/socket"),
            &format!("{origin}/page"),
            &origin,
            &PolicyContainer::default(),
            &["chat".into()],
        )
        .unwrap();
    assert_eq!(opened.protocol, "chat");
    opened.connection.send_text("hello").unwrap();
    assert_eq!(
        opened.connection.receive().unwrap(),
        WebSocketFrame::Text("hello".into())
    );
    opened.connection.send_binary(&[0, 255, 42]).unwrap();
    assert_eq!(
        opened.connection.receive().unwrap(),
        WebSocketFrame::Binary(vec![0, 255, 42])
    );
    opened.connection.shutdown(1000, "").unwrap();
    assert_eq!(
        opened.connection.receive().unwrap(),
        WebSocketFrame::Close {
            code: 1000,
            reason: String::new()
        }
    );
    server.join().unwrap();
}

fn read_headers(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
    }
    String::from_utf8(bytes).unwrap()
}

fn read_client_frame(stream: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut header = [0; 2];
    stream.read_exact(&mut header).unwrap();
    assert_ne!(header[1] & 0x80, 0, "client frames are masked");
    let length = usize::from(header[1] & 0x7f);
    assert!(length < 126);
    let mut mask = [0; 4];
    stream.read_exact(&mut mask).unwrap();
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload).unwrap();
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
    (header[0] & 0x0f, payload)
}

// The loopback server uses Windows CNG only for RFC 6455's opening accept key.
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptOpenAlgorithmProvider(
        handle: *mut *mut c_void,
        algorithm: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32;
    fn BCryptHash(
        handle: *mut c_void,
        secret: *mut u8,
        secret_len: u32,
        input: *mut u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: *mut c_void, flags: u32) -> i32;
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut handle = null_mut();
    assert!(
        unsafe { BCryptOpenAlgorithmProvider(&mut handle, wide("SHA1").as_ptr(), null(), 0) } >= 0
    );
    let mut result = [0; 20];
    assert!(
        unsafe {
            BCryptHash(
                handle,
                null_mut(),
                0,
                input.as_ptr().cast_mut(),
                input.len() as u32,
                result.as_mut_ptr(),
                20,
            )
        } >= 0
    );
    assert!(unsafe { BCryptCloseAlgorithmProvider(handle, 0) } >= 0);
    result
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in input.chunks(3) {
        let value = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(TABLE[((value >> 18) & 63) as usize] as char);
        output.push(TABLE[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
