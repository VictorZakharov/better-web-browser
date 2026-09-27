//! Full hidden-browser WebSocket exchange across page realm, renderer IPC, and WinHTTP.
use super::*;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[test]
fn websocket_roundtrip_reaches_the_retained_document_and_closes_cleanly() {
    let http = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    let http_address = http.local_addr().unwrap();
    let websocket = TcpListener::bind("127.0.0.1:0").expect("bind WebSocket fixture");
    let websocket_address = websocket.local_addr().unwrap();
    let html = format!(
        r#"<!doctype html><title>socket pending</title>
        <style>body {{ background: rgb(220,20,20); }} #state {{height:600px}}</style>
        <div id="state">pending</div><script>
        const socket = new WebSocket('ws://{websocket_address}/echo', 'chat');
        socket.binaryType = 'arraybuffer';
        const messages = [];
        socket.onopen = event => {{
            if (!event.isTrusted || socket.readyState !== WebSocket.OPEN ||
                socket.protocol !== 'chat') throw new Error('WebSocket open contract');
            socket.send('hello');
            socket.send(new Uint8Array([0, 255, 42]));
        }};
        socket.onmessage = event => {{
            if (!event.isTrusted) throw new Error('untrusted WebSocket message');
            messages.push(event.data);
            if (messages.length !== 2) return;
            if (messages[0] !== 'hello' ||
                !(messages[1] instanceof ArrayBuffer) ||
                String(new Uint8Array(messages[1])) !== '0,255,42')
                throw new Error('WebSocket frame conversion or order');
            socket.close(1000);
        }};
        socket.onclose = event => {{
            if (!event.wasClean || event.code !== 1000 ||
                socket.readyState !== WebSocket.CLOSED || socket.bufferedAmount !== 0)
                throw new Error('WebSocket close contract');
            document.getElementById('state').textContent = 'socket complete';
            document.body.style.backgroundColor = 'rgb(17,170,34)';
            document.title = 'socket complete';
        }};
        socket.onerror = () => {{ throw new Error('WebSocket transport failed'); }};
        </script>"#
    );
    let page_server = thread::spawn(move || {
        serve_parallel_fixtures(http, 1, move |_| FixtureResponse::html(html.clone()))
    });
    let socket_server = thread::spawn(move || serve_socket(websocket));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{http_address}/page");
    let mut child = hidden_benchmark(&url, &artifacts, 1600);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    page_server.join().unwrap().unwrap();
    socket_server.join().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(
        report.contains("socket complete"),
        "socket callback did not finish: {report}"
    );
    assert_green_capture(&artifacts, "WebSocket exchange did not repaint the page");
}

#[test]
fn dedicated_worker_websocket_uses_worker_csp_and_roundtrips_frames() {
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let http_address = http.local_addr().unwrap();
    let websocket = TcpListener::bind("127.0.0.1:0").unwrap();
    let websocket_address = websocket.local_addr().unwrap();
    let html = r#"<!doctype html><title>worker socket pending</title>
        <style>body { background: rgb(220,20,20); } #state { height:600px; }</style>
        <div id="state">pending</div><script>
        const worker = new Worker('/worker-socket.js');
        worker.onerror = event => { document.title = 'worker socket error ' + event.message; };
        worker.onmessage = event => {
            if (event.data !== 'complete') throw new Error('unexpected Worker socket result');
            document.title = 'worker socket complete';
            document.getElementById('state').textContent = document.title;
            document.body.style.backgroundColor = 'rgb(17,170,34)';
            worker.terminate();
        };
        </script>"#
        .to_string();
    let worker = format!(
        r#"const socket = new WebSocket('ws://{websocket_address}/echo', 'chat');
        socket.binaryType = 'arraybuffer';
        const messages = [];
        socket.onopen = event => {{
            if (!event.isTrusted || socket.protocol !== 'chat') throw new Error('open contract');
            socket.send('hello'); socket.send(new Uint8Array([0, 255, 42]));
        }};
        socket.onmessage = event => {{
            if (!event.isTrusted) throw new Error('untrusted message');
            messages.push(event.data);
            if (messages.length !== 2) return;
            if (messages[0] !== 'hello' || !(messages[1] instanceof ArrayBuffer) ||
                String(new Uint8Array(messages[1])) !== '0,255,42')
                throw new Error('frame conversion or order');
            socket.close(1000);
        }};
        socket.onclose = event => {{
            if (!event.isTrusted || !event.wasClean || event.code !== 1000)
                throw new Error('close contract');
            postMessage('complete');
        }};
        socket.onerror = () => {{ throw new Error('Worker WebSocket transport failed'); }};"#
    );
    let page_server = thread::spawn(move || {
        serve_parallel_fixtures(http, 2, move |request| {
            if request.contains("GET /worker-socket.js ") {
                FixtureResponse::script(worker.clone(), Duration::ZERO).header(
                    "Content-Security-Policy",
                    format!("connect-src ws://{websocket_address}"),
                )
            } else {
                FixtureResponse::html(html.clone()).header(
                    "Content-Security-Policy",
                    "connect-src 'none'; worker-src 'self'",
                )
            }
        })
    });
    let socket_server = thread::spawn(move || serve_socket(websocket));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{http_address}/page"), &artifacts, 2200);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    page_server.join().unwrap().unwrap();
    socket_server.join().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("worker socket complete"), "{report}");
    assert_green_capture(&artifacts, "Worker WebSocket did not repaint the page");
}

#[test]
fn terminating_worker_revokes_its_inflight_websocket_handshake() {
    run_worker_socket_teardown(false, false);
}
#[test]
fn navigation_revokes_worker_inflight_websocket_handshake() {
    run_worker_socket_teardown(true, false);
}
#[test]
fn terminating_worker_closes_its_active_websocket() {
    run_worker_socket_teardown(false, true);
}

fn run_worker_socket_teardown(navigate: bool, active: bool) {
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let http_address = http.local_addr().unwrap();
    let websocket = TcpListener::bind("127.0.0.1:0").unwrap();
    let websocket_address = websocket.local_addr().unwrap();
    let seen = Arc::new(AtomicBool::new(false));
    let page_seen = Arc::clone(&seen);
    let socket_seen = Arc::clone(&seen);
    let transition = if navigate {
        "location.href = '/replacement';"
    } else {
        "worker.terminate(); document.title = 'worker socket retired'; document.body.style.backgroundColor = 'rgb(17,170,34)';"
    };
    let html = format!(
        r#"<!doctype html><title>worker socket pending</title>
        <style>body {{ background: rgb(220,20,20); height:600px; }}</style><script>
        const worker = new Worker('/worker-socket.js');
        worker.onerror = event => {{ throw new Error(event.message); }};
        worker.onmessage = event => {{
            if (event.data !== 'created') throw new Error('late Worker socket event');
            fetch('/handshake-seen').then(response => response.text()).then(value => {{
                if (value !== 'seen') throw new Error('handshake not seen');
                {transition}
            }});
        }};
        </script>"#
    );
    let ready = if active {
        "socket.onopen = () => postMessage('created');"
    } else {
        "socket.onopen = () => postMessage('late-open'); postMessage('created');"
    };
    let worker = format!(
        r#"const socket = new WebSocket('ws://{websocket_address}/delayed');
        {ready}
        socket.onerror = () => postMessage('late-error');
        "#
    );
    let page_server = thread::spawn(move || {
        serve_parallel_fixtures(http, if navigate { 4 } else { 3 }, move |request| {
            if request.contains("GET /worker-socket.js ") {
                FixtureResponse::script(worker.clone(), Duration::ZERO)
            } else if request.contains("GET /handshake-seen ") {
                let deadline = Instant::now() + Duration::from_secs(8);
                while !page_seen.load(Ordering::Acquire) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                FixtureResponse::resource(
                    if page_seen.load(Ordering::Acquire) {
                        "seen"
                    } else {
                        "missing"
                    },
                    "text/plain",
                    Duration::ZERO,
                )
            } else if request.contains("GET /replacement ") {
                FixtureResponse::html(
                    "<title>worker socket retired</title><style>body{background:rgb(17,170,34);height:600px}</style>",
                )
            } else {
                FixtureResponse::html(html.clone())
            }
        })
    });
    let socket_server =
        thread::spawn(move || serve_teardown_socket(websocket, socket_seen, active));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{http_address}/page"), &artifacts, 2500);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    page_server.join().unwrap().unwrap();
    socket_server.join().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("worker socket retired"), "{report}");
    assert_green_capture(&artifacts, "retired Worker socket changed the page");
}

fn serve_socket(listener: TcpListener) {
    let mut stream = accept_socket(listener);
    let request = read_headers(&mut stream);
    assert!(request.contains("Origin: http://127.0.0.1:"));
    assert!(request.contains("Sec-WebSocket-Protocol: chat"));
    upgrade_socket(&mut stream, &request, "Sec-WebSocket-Protocol: chat\r\n");
    assert_eq!(read_client_frame(&mut stream), (1, b"hello".to_vec()));
    stream
        .write_all(&[0x81, 5, b'h', b'e', b'l', b'l', b'o'])
        .unwrap();
    assert_eq!(read_client_frame(&mut stream), (2, vec![0, 255, 42]));
    stream.write_all(&[0x82, 3, 0, 255, 42]).unwrap();
    assert_eq!(read_client_frame(&mut stream).0, 8);
    stream.write_all(&[0x88, 2, 0x03, 0xe8]).unwrap();
}

fn serve_teardown_socket(listener: TcpListener, seen: Arc<AtomicBool>, active: bool) {
    let mut stream = accept_socket(listener);
    let request = read_headers(&mut stream);
    assert!(request.contains("Origin: http://127.0.0.1:"));
    if !active {
        seen.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(700));
    }
    upgrade_socket(&mut stream, &request, "");
    if active {
        seen.store(true, Ordering::Release);
        let (kind, payload) = read_client_frame(&mut stream);
        assert_eq!((kind, payload), (8, vec![0x03, 0xe9]));
        stream.write_all(&[0x88, 2, 0x03, 0xe9]).unwrap();
        return;
    }
    let mut byte = [0];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        other => panic!("retired Worker sent data or left socket open: {other:?}"),
    }
}

fn accept_socket(listener: TcpListener) -> TcpStream {
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    stream
}

fn upgrade_socket(stream: &mut TcpStream, request: &str, extra_headers: &str) {
    let key = request
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-key"))
                .map(|(_, value)| value.trim())
        })
        .unwrap();
    let accept = base64(&sha1(
        format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
    ));
    stream.write_all(format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{extra_headers}\r\n"
    ).as_bytes()).unwrap();
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
    assert_ne!(header[1] & 0x80, 0);
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
    let algorithm: Vec<u16> = "SHA1".encode_utf16().chain(std::iter::once(0)).collect();
    assert!(
        unsafe { BCryptOpenAlgorithmProvider(&mut handle, algorithm.as_ptr(), null(), 0) } >= 0
    );
    let mut output = [0; 20];
    assert!(
        unsafe {
            BCryptHash(
                handle,
                null_mut(),
                0,
                input.as_ptr().cast_mut(),
                input.len() as u32,
                output.as_mut_ptr(),
                20,
            )
        } >= 0
    );
    assert!(unsafe { BCryptCloseAlgorithmProvider(handle, 0) } >= 0);
    output
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
