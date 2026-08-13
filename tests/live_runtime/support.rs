use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static NEXT_ARTIFACT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn hidden_benchmark(
    url: &str,
    artifacts: &TestArtifacts,
    settle_ms: u64,
) -> std::process::Child {
    let settle_ms = settle_ms.to_string();
    Command::new(env!("CARGO_BIN_EXE_better-web-browser"))
        .args([
            "--benchmark",
            url,
            "--output",
            path_text(&artifacts.json),
            "--screenshot",
            path_text(&artifacts.screenshot),
            "--settle-ms",
            &settle_ms,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("launch hidden Breeze benchmark")
}

pub(super) fn serve_fixtures(
    listener: TcpListener,
    request_count: usize,
    response_for: impl Fn(&str) -> &'static str,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set fixture nonblocking: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0..request_count {
        let (mut stream, _) = accept_until(&listener, deadline)?;
        stream
            .set_nonblocking(false)
            .map_err(|error| format!("make fixture connection blocking: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| format!("set fixture read timeout: {error}"))?;
        let request = read_request(&mut stream)?;
        let html = response_for(&request);
        write_response(&mut stream, "text/html; charset=utf-8", html)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct FixtureResponse {
    content_type: &'static str,
    body: &'static str,
    delay: Duration,
}

impl FixtureResponse {
    pub(super) fn html(body: &'static str) -> Self {
        Self {
            content_type: "text/html; charset=utf-8",
            body,
            delay: Duration::ZERO,
        }
    }

    pub(super) fn script(body: &'static str, delay: Duration) -> Self {
        Self {
            content_type: "text/javascript; charset=utf-8",
            body,
            delay,
        }
    }
}

pub(super) fn serve_parallel_fixtures(
    listener: TcpListener,
    request_count: usize,
    response_for: impl Fn(&str) -> FixtureResponse + Send + Sync + 'static,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set fixture nonblocking: {error}"))?;
    let response_for = std::sync::Arc::new(response_for);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut workers = Vec::new();
    for _ in 0..request_count {
        let (stream, _) = accept_until(&listener, deadline)?;
        let response_for = std::sync::Arc::clone(&response_for);
        workers.push(thread::spawn(move || {
            serve_fixture_connection(stream, &*response_for)
        }));
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| "fixture response worker panicked".to_string())??;
    }
    Ok(())
}

fn accept_until(
    listener: &TcpListener,
    deadline: Instant,
) -> Result<(TcpStream, std::net::SocketAddr), String> {
    loop {
        match listener.accept() {
            Ok(connection) => return Ok(connection),
            Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("accept fixture request: {error}")),
        }
    }
}

fn serve_fixture_connection(
    mut stream: TcpStream,
    response_for: &dyn Fn(&str) -> FixtureResponse,
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("make fixture connection blocking: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("set fixture read timeout: {error}"))?;
    let request = read_request(&mut stream)?;
    let response = response_for(&request);
    thread::sleep(response.delay);
    write_response(&mut stream, response.content_type, response.body)
}

fn read_request(stream: &mut TcpStream) -> Result<String, String> {
    let mut request_bytes = [0_u8; 4096];
    let bytes_read = stream
        .read(&mut request_bytes)
        .map_err(|error| format!("read fixture request: {error}"))?;
    Ok(String::from_utf8_lossy(&request_bytes[..bytes_read]).into_owned())
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &str) -> Result<(), String> {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(body.as_bytes()))
        .map_err(|error| format!("write fixture response: {error}"))
}

pub(super) fn wait_for_child(child: &mut std::process::Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll hidden Breeze process") {
            return status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("terminate timed-out Breeze process");
            let _ = child.wait();
            panic!("hidden Breeze benchmark exceeded {timeout:?}");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn json_integer(json: &str, key: &str) -> Option<u64> {
    json_value(json, key)?.parse::<u64>().ok()
}

pub(super) fn json_number(json: &str, key: &str) -> Option<f64> {
    json_value(json, key)?.parse::<f64>().ok()
}

fn json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("\"{key}\":");
    json.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .map(|value| value.trim().trim_end_matches(','))
}

pub(super) fn assert_green_capture(artifacts: &TestArtifacts, message: &str) {
    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] < 40 && pixel[1] > 130 && pixel[2] < 70,
        "{message}; center pixel was {pixel:?}"
    );
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("test artifact path is valid UTF-8")
}

pub(super) struct TestArtifacts {
    root: PathBuf,
    pub(super) json: PathBuf,
    pub(super) screenshot: PathBuf,
}

impl TestArtifacts {
    pub(super) fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock precedes Unix epoch")
            .as_nanos();
        let artifact_id = NEXT_ARTIFACT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "breeze-live-runtime-{}-{nonce}-{artifact_id}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create live-runtime artifact directory");
        Self {
            json: root.join("result.json"),
            screenshot: root.join("capture.png"),
            root,
        }
    }
}

impl Drop for TestArtifacts {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
