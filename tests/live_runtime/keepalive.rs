use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const ENTRY: &str = r#"<!doctype html><title>Keepalive entry</title>
<script>
  fetch('/collect', {method:'POST', body:'proof=survived', keepalive:true}).catch(() => {});
  location.href = '/next';
</script>"#;
const NEXT: &str = r#"<!doctype html><title>Keepalive next</title>
<style>html,body{margin:0;background:rgb(17,170,34)}</style>
<p>Navigation completed while a keepalive Fetch was in flight.</p>"#;
const ABORT_ENTRY: &str = r#"<!doctype html><title>Aborted keepalive entry</title>
<script>
  const controller = new AbortController();
  fetch('/collect', {method:'POST', body:'should-not-send', keepalive:true,
                     signal:controller.signal}).catch(() => {});
  controller.abort();
  location.href = '/next';
</script>"#;
const CSP_ENTRY: &str = r#"<!doctype html><title>Blocked Beacon entry</title>
<script>
  const meta = document.createElement('meta');
  meta.setAttribute('http-equiv', 'Content-Security-Policy');
  meta.setAttribute('content', "connect-src 'none'");
  document.head.appendChild(meta);
  navigator.sendBeacon('/blocked', 'proof');
  location.href = '/next';
</script>"#;
const COOKIE_ENTRY: &str = r#"<!doctype html><title>Cookie keepalive entry</title>
<script>
  document.cookie = 'proof=updated; path=/';
  fetch('/collect', {method:'POST', body:'cookie-proof', keepalive:true}).catch(() => {});
  location.href = '/next';
</script>"#;
const PARSER_COOKIE_ENTRY: &str = r#"<!doctype html><title>Parser cookie entry</title>
<script>
  document.cookie = 'parser_proof=updated; path=/';
  const script = document.createElement('script');
  script.src = '/parser-external.js';
  document.head.appendChild(script);
</script>"#;

#[test]
fn same_task_meta_csp_blocks_beacon_before_navigation() {
    let records = capture_case("/csp-entry", Duration::from_secs(5));
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("GET /next ")),
        "navigation did not complete: {records:?}"
    );
    assert!(
        !records
            .iter()
            .any(|request| request.starts_with("POST /blocked ")),
        "Beacon bypassed same-task meta CSP: {records:?}"
    );
}

#[test]
fn same_task_cookie_write_is_sent_on_keepalive_post() {
    let records = capture_case("/cookie-entry", Duration::from_secs(5));
    let post = records
        .iter()
        .find(|request| request.starts_with("POST /collect "))
        .unwrap_or_else(|| panic!("keepalive POST missing: {records:?}"));
    assert!(
        post.lines()
            .any(|line| line.to_ascii_lowercase().starts_with("cookie:")
                && line.contains("proof=updated")),
        "POST missed same-task cookie: {post}"
    );
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("GET /next ")),
        "navigation did not complete: {records:?}"
    );
}

#[test]
fn parser_task_cookie_write_precedes_dynamic_script_request() {
    let records = capture_case("/parser-cookie-entry", Duration::from_secs(5));
    let request = records
        .iter()
        .find(|request| request.starts_with("GET /parser-external.js "))
        .unwrap_or_else(|| panic!("dynamic script request missing: {records:?}"));
    assert!(
        request.lines().any(|line| {
            line.to_ascii_lowercase().starts_with("cookie:")
                && line.contains("parser_proof=updated")
        }),
        "dynamic script request missed the parser task cookie: {request}"
    );
}

fn capture_case(path: &str, linger: Duration) -> Vec<String> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind state-order fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let records = Arc::clone(&received);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + linger;
        let mut workers = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    let records = Arc::clone(&records);
                    workers.push(thread::spawn(move || serve_one(stream, records)));
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(10035) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept state-order fixture: {error}"),
            }
        }
        for worker in workers {
            worker.join().expect("fixture worker panicked");
        }
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}{path}"), &artifacts, 1500);
    let status = wait_for_child(&mut child, Duration::from_secs(12));
    server.join().expect("state-order fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    received.lock().unwrap().clone()
}

#[test]
fn fetch_keepalive_post_survives_document_navigation_without_stalling_it() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind keepalive fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let records = Arc::clone(&received);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut workers = Vec::new();
        while workers.len() < 3 && Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    let records = Arc::clone(&records);
                    workers.push(thread::spawn(move || serve_one(stream, records)));
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(10035) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept keepalive fixture: {error}"),
            }
        }
        let count = workers.len();
        for worker in workers {
            worker.join().expect("keepalive fixture worker panicked");
        }
        assert_eq!(
            count,
            3,
            "missing keepalive upload or navigation: {:?}",
            records.lock().unwrap()
        );
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 1500);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().expect("keepalive fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let records = received.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("POST /collect ")
                && request.ends_with("proof=survived")),
        "keepalive POST missing: {records:?}"
    );
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("GET /next ")),
        "navigation did not continue: {records:?}"
    );
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("/next"),
        "final document was not committed: {report}"
    );
    assert_green_capture(
        &artifacts,
        "keepalive navigation did not paint the next page",
    );
}

#[test]
fn aborted_keepalive_is_not_admitted_during_navigation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind keepalive fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let records = Arc::clone(&received);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut workers = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    let records = Arc::clone(&records);
                    workers.push(thread::spawn(move || serve_one(stream, records)));
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(10035) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept keepalive fixture: {error}"),
            }
        }
        for worker in workers {
            worker.join().expect("keepalive fixture worker panicked");
        }
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/abort-entry"), &artifacts, 1500);
    let status = wait_for_child(&mut child, Duration::from_secs(10));
    server.join().expect("keepalive fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let records = received.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("GET /next ")),
        "navigation did not continue: {records:?}"
    );
    assert!(
        !records
            .iter()
            .any(|request| request.starts_with("POST /collect ")),
        "aborted keepalive POST reached the network: {records:?}"
    );
}

fn serve_one(mut stream: TcpStream, records: Arc<Mutex<Vec<String>>>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).expect("read keepalive request");
        assert!(count > 0, "keepalive request closed before headers/body");
        request.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
            let body_len = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + body_len {
                break;
            }
        }
        assert!(request.len() < 16 * 1024, "fixture request exceeded bound");
    }
    let request = String::from_utf8(request).expect("ASCII keepalive request");
    let path = request.lines().next().unwrap_or_default();
    let (status, body) = if path.starts_with("GET /entry ") {
        ("200 OK", ENTRY)
    } else if path.starts_with("GET /abort-entry ") {
        ("200 OK", ABORT_ENTRY)
    } else if path.starts_with("GET /csp-entry ") {
        ("200 OK", CSP_ENTRY)
    } else if path.starts_with("GET /cookie-entry ") {
        ("200 OK", COOKIE_ENTRY)
    } else if path.starts_with("GET /parser-cookie-entry ") {
        ("200 OK", PARSER_COOKIE_ENTRY)
    } else if path.starts_with("GET /parser-external.js ") {
        ("200 OK", "window.parserScriptLoaded = true;")
    } else if path.starts_with("GET /next ") {
        ("200 OK", NEXT)
    } else if path.starts_with("POST /collect ") {
        thread::sleep(Duration::from_millis(300));
        ("204 No Content", "")
    } else if path.starts_with("POST /blocked ") {
        ("204 No Content", "")
    } else {
        panic!("unexpected keepalive fixture request: {path}");
    };
    records.lock().unwrap().push(request);
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
}
