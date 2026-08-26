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
    hidden_benchmark_with_args(url, artifacts, settle_ms, &[])
}

pub(super) fn hidden_benchmark_with_args(
    url: &str,
    artifacts: &TestArtifacts,
    settle_ms: u64,
    extra_arguments: &[&str],
) -> std::process::Child {
    spawn_hidden_benchmark(url, artifacts, settle_ms, extra_arguments, None)
}

pub(super) fn hidden_benchmark_with_fresh_profile_args(
    url: &str,
    artifacts: &TestArtifacts,
    settle_ms: u64,
    extra_arguments: &[&str],
) -> std::process::Child {
    let profile = artifacts.root.join("profile");
    fs::create_dir(&profile).expect("create hidden Breeze profile");
    spawn_hidden_benchmark(url, artifacts, settle_ms, extra_arguments, Some(&profile))
}

fn spawn_hidden_benchmark(
    url: &str,
    artifacts: &TestArtifacts,
    settle_ms: u64,
    extra_arguments: &[&str],
    profile: Option<&Path>,
) -> std::process::Child {
    let settle_ms = settle_ms.to_string();
    let mut command = Command::new(env!("CARGO_BIN_EXE_better-web-browser"));
    command
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
        .args(extra_arguments)
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(profile) = profile {
        command.env("BREEZE_PROFILE_DIRECTORY", profile);
    }
    command.spawn().expect("launch hidden Breeze benchmark")
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

#[derive(Clone)]
pub(super) struct FixtureResponse {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: String,
    stream_chunks: Option<Vec<Vec<u8>>>,
    stream_chunk_delay: Duration,
    allow_disconnect: bool,
    delay: Duration,
    headers: Vec<(String, String)>,
}

impl FixtureResponse {
    pub(super) fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "text/html; charset=utf-8",
            body: body.into(),
            stream_chunks: None,
            stream_chunk_delay: Duration::ZERO,
            allow_disconnect: false,
            delay: Duration::ZERO,
            headers: Vec::new(),
        }
    }

    pub(super) fn script(body: impl Into<String>, delay: Duration) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "text/javascript; charset=utf-8",
            body: body.into(),
            stream_chunks: None,
            stream_chunk_delay: Duration::ZERO,
            allow_disconnect: false,
            delay,
            headers: Vec::new(),
        }
    }

    pub(super) fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "application/json; charset=utf-8",
            body: body.into(),
            stream_chunks: None,
            stream_chunk_delay: Duration::ZERO,
            allow_disconnect: false,
            delay: Duration::ZERO,
            headers: Vec::new(),
        }
    }

    pub(super) fn streamed(
        content_type: &'static str,
        chunks: Vec<Vec<u8>>,
        chunk_delay: Duration,
    ) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type,
            body: String::new(),
            stream_chunks: Some(chunks),
            stream_chunk_delay: chunk_delay,
            allow_disconnect: false,
            delay: Duration::ZERO,
            headers: Vec::new(),
        }
    }

    pub(super) fn allow_disconnect(mut self) -> Self {
        self.allow_disconnect = true;
        self
    }

    pub(super) fn status(mut self, status: u16, reason: &'static str) -> Self {
        self.status = status;
        self.reason = reason;
        self
    }

    pub(super) fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
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
    write_fixture_response(&mut stream, &response)
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

fn write_fixture_response(
    stream: &mut TcpStream,
    response: &FixtureResponse,
) -> Result<(), String> {
    let extra_headers = response
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let content_length = response
        .stream_chunks
        .as_ref()
        .map(|chunks| chunks.iter().map(Vec::len).sum())
        .unwrap_or(response.body.len());
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.status, response.reason, response.content_type, extra_headers, content_length
    );
    if let Err(error) = stream.write_all(headers.as_bytes()) {
        return handle_fixture_write_error(error, response.allow_disconnect);
    }
    let Some(chunks) = &response.stream_chunks else {
        return stream
            .write_all(response.body.as_bytes())
            .map_err(|error| format!("write fixture response: {error}"));
    };
    for chunk in chunks {
        if let Err(error) = stream.write_all(chunk).and_then(|_| stream.flush()) {
            return handle_fixture_write_error(error, response.allow_disconnect);
        }
        thread::sleep(response.stream_chunk_delay);
    }
    Ok(())
}

fn handle_fixture_write_error(error: std::io::Error, allow_disconnect: bool) -> Result<(), String> {
    if allow_disconnect
        && matches!(
            error.kind(),
            ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
        )
    {
        Ok(())
    } else {
        Err(format!("write fixture response: {error}"))
    }
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
