use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const ENTRY: &str = r#"<!doctype html><title>Multipart upload pending</title>
<form action='/upload' method='post' enctype='multipart/form-data'></form>
<script>
  const form = document.querySelector('form');
  form.addEventListener('formdata', event => {
    const bytes = new Uint8Array(192 * 1024);
    for (let index = 0; index < bytes.length; index++) bytes[index] = index & 255;
    event.formData.append('upload', new File([bytes], 'sample.bin',
      {type:'application/octet-stream'}));
  });
  form.submit();
</script>"#;
const COMPLETE: &str = r#"<!doctype html><title>Multipart upload complete</title>
<style>html,body{margin:0;background:rgb(17,170,34)}</style>
<p>Binary multipart upload arrived.</p>"#;
const WORKER_ENTRY: &str = r#"<!doctype html><title>Worker upload pending</title>
<style>html,body{margin:0;background:rgb(80,0,0)}body{min-height:100vh}</style>
<script>
  const worker = new Worker('/worker.js');
  worker.onmessage = event => {
    document.title = 'Worker upload ' + event.data;
    if (event.data === 'ok') document.body.style.backgroundColor = 'rgb(17,170,34)';
  };
  worker.onerror = event => { document.title = 'Worker upload error ' + event.message; };
</script>"#;
const WORKER_SCRIPT: &str = r#"const form = new FormData();
form.append('note', 'worker');
form.append('file', new File([new Uint8Array([0,255,42])], 'worker.bin',
  {type:'application/octet-stream'}));
fetch('/upload', {method:'POST', body:form}).then(response => response.text())
  .then(text => postMessage(text === 'ok' ? 'ok' : 'bad'))
  .catch(() => postMessage('bad'));"#;

#[test]
fn hidden_browser_sends_binary_file_in_multipart_form_navigation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind upload fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(Mutex::new(None));
    let upload = Arc::clone(&received);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    let (head, body) = serve_upload_request(stream, false);
                    if head.starts_with("POST /upload ") {
                        *upload.lock().unwrap() = Some((head, body));
                        return;
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(10035) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept upload fixture: {error}"),
            }
        }
        panic!("browser did not submit multipart form before timeout");
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 1600);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().expect("upload fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let (head, body) = received
        .lock()
        .unwrap()
        .take()
        .expect("missing upload POST");
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary="),
        "incorrect multipart headers: {head}"
    );
    assert!(
        body.windows(b"name=\"upload\"; filename=\"sample.bin\"".len())
            .any(|part| part == b"name=\"upload\"; filename=\"sample.bin\""),
        "file part is missing"
    );
    assert!(
        body.windows(b"Content-Type: application/octet-stream".len())
            .any(|part| part == b"Content-Type: application/octet-stream"),
        "file MIME type is missing"
    );
    let payload = (0..192 * 1024).map(|index| index as u8).collect::<Vec<_>>();
    assert!(
        body.windows(payload.len())
            .any(|part| part == payload.as_slice()),
        "the complete binary payload was not delivered"
    );
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("/upload"),
        "upload response was not committed: {report}"
    );
    assert_green_capture(&artifacts, "multipart upload response did not render");
}

#[test]
fn hidden_dedicated_worker_serializes_formdata_file_upload() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind worker upload fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(Mutex::new(None));
    let upload = Arc::clone(&received);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    let (head, body) = serve_upload_request(stream, true);
                    if head.starts_with("POST /upload ") {
                        *upload.lock().unwrap() = Some((head, body));
                        return;
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(10035) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept worker upload fixture: {error}"),
            }
        }
        panic!("worker did not submit FormData before timeout");
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 2300);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server
        .join()
        .expect("worker upload fixture server panicked");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let (head, body) = received
        .lock()
        .unwrap()
        .take()
        .expect("missing worker upload POST");
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary=")
    );
    assert!(
        body.windows(b"filename=\"worker.bin\"".len())
            .any(|part| part == b"filename=\"worker.bin\"")
    );
    assert!(body.windows(3).any(|part| part == [0, 255, 42]));
    assert!(
        body.windows(b"name=\"note\"".len())
            .any(|part| part == b"name=\"note\"")
    );
    let report = fs::read_to_string(&artifacts.json).expect("read worker benchmark report");
    assert!(
        report.contains("Worker upload ok"),
        "worker did not acknowledge upload: {report}"
    );
    assert_green_capture(
        &artifacts,
        "worker FormData POST did not render complete state",
    );
}

fn serve_upload_request(mut stream: TcpStream, worker: bool) -> (String, Vec<u8>) {
    // Winsock may inherit the listener's nonblocking mode on accepted sockets.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 8192];
    let (header_end, body_len) = loop {
        let count = stream.read(&mut buffer).expect("read upload request");
        assert!(count > 0, "upload request closed before headers/body");
        request.extend_from_slice(&buffer[..count]);
        if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
            let body_len = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + body_len {
                break (end, body_len);
            }
        }
        assert!(
            request.len() <= 6 * 1024 * 1024,
            "fixture request exceeded bound"
        );
    };
    let head = String::from_utf8(request[..header_end].to_vec()).expect("ASCII upload headers");
    let body = request[header_end + 4..header_end + 4 + body_len].to_vec();
    let path = head.lines().next().unwrap_or_default();
    let (status, content_type, html) = if path.starts_with("GET /entry ") {
        (
            "200 OK",
            "text/html",
            if worker { WORKER_ENTRY } else { ENTRY },
        )
    } else if path.starts_with("GET /worker.js ") && worker {
        ("200 OK", "text/javascript", WORKER_SCRIPT)
    } else if path.starts_with("POST /upload ") {
        if worker {
            ("200 OK", "text/plain", "ok")
        } else {
            ("200 OK", "text/html", COMPLETE)
        }
    } else {
        ("404 Not Found", "text/plain", "")
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        html.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(html.as_bytes()).unwrap();
    (head, body)
}
