use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const ENTRY: &str = r#"<!doctype html><title>Beacon entry</title>
<script>
  if (!navigator.sendBeacon('/collect', 'proof=delivered'))
    throw new Error('Beacon was not queued');
  location.href = '/next';
</script>"#;
const NEXT: &str = r#"<!doctype html><title>Beacon next</title>
<style>html,body{margin:0;background:rgb(17,170,34)}</style>
<p>Navigation completed while the Beacon was in flight.</p>"#;

#[test]
fn beacon_post_survives_document_navigation_without_a_response_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Beacon fixture");
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
                Err(error) => panic!("accept Beacon fixture: {error}"),
            }
        }
        let count = workers.len();
        for worker in workers {
            worker.join().expect("Beacon fixture worker panicked");
        }
        assert_eq!(
            count,
            3,
            "missing Beacon or navigation request: {:?}",
            records
                .lock()
                .unwrap()
                .iter()
                .map(|request| request.lines().next().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        );
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 1400);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().expect("Beacon fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let records = received.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|request| request.starts_with("POST /collect ")
                && request.ends_with("proof=delivered")),
        "Beacon POST missing: {records:?}"
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
        "Beacon navigation did not render the next document",
    );
}

fn serve_one(mut stream: TcpStream, records: Arc<Mutex<Vec<String>>>) {
    // Winsock may inherit the listener's nonblocking mode on accepted sockets.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).expect("read Beacon request");
        assert!(count > 0, "Beacon request closed before headers/body");
        request.extend_from_slice(&buffer[..count]);
        let header_end = request.windows(4).position(|bytes| bytes == b"\r\n\r\n");
        if let Some(header_end) = header_end {
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
    let request = String::from_utf8(request).expect("ASCII Beacon request");
    let path = request.lines().next().unwrap_or_default();
    let (status, content_type, body) = if path.starts_with("GET /entry ") {
        ("200 OK", "text/html", ENTRY)
    } else if path.starts_with("GET /next ") {
        ("200 OK", "text/html", NEXT)
    } else if path.starts_with("POST /collect ") {
        // The browser must not block navigation waiting for this response.
        thread::sleep(Duration::from_millis(300));
        ("204 No Content", "text/plain", "")
    } else {
        panic!("unexpected Beacon fixture request: {path}");
    };
    records.lock().unwrap().push(request);
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
}
