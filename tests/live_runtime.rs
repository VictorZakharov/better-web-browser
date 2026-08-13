#![cfg(target_os = "windows")]

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FIXTURE_HTML: &str = r#"<!doctype html>
<title>runtime pending</title>
<style>
  html, body { margin: 0; background: rgb(16, 32, 48); }
  #state { width: 100%; height: 600px; background: rgb(34, 51, 68); color: white; }
</style>
<div id="state">initial</div>
<script>
  setTimeout(() => {
    const state = document.getElementById('state');
    state.textContent = 'live runtime updated';
    state.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'runtime updated';
  }, 1750);
</script>"#;
const NAVIGATING_HTML: &str = r#"<!doctype html>
<style>html, body { margin: 0; background: rgb(34, 51, 68); }</style>
<script>
  setTimeout(() => { location.href = '/replacement'; }, 1600);
  setTimeout(() => { document.body.style.backgroundColor = 'rgb(220, 20, 20)'; }, 2000);
</script>"#;
const REPLACEMENT_HTML: &str = r#"<!doctype html>
<title>replacement document</title>
<div style="width:100%;height:600px;background-color:rgb(17,170,34)">
  replacement document
</div>"#;

#[test]
fn hidden_browser_repaints_after_a_post_load_timer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| FIXTURE_HTML));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/live-runtime");

    let mut child = hidden_benchmark(&url, &artifacts, 800);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "hidden run reported JavaScript errors:\n{report}"
    );
    assert!(
        json_integer(&report, "javascript_dom_mutations").is_some_and(|count| count >= 3),
        "post-load mutations were not recorded:\n{report}"
    );

    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] < 40 && pixel[1] > 130 && pixel[2] < 70,
        "delayed green repaint was absent; center pixel was {pixel:?}"
    );
}

#[test]
fn navigation_cancels_the_previous_documents_pending_timer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_fixtures(listener, 2, |request| {
            if request.contains("/replacement") {
                REPLACEMENT_HTML
            } else {
                NAVIGATING_HTML
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/navigating");

    let mut child = hidden_benchmark(&url, &artifacts, 900);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("/replacement"),
        "script navigation did not commit the replacement document:\n{report}"
    );
    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] < 40 && pixel[1] > 130 && pixel[2] < 70,
        "the stale red callback repainted after navigation; center pixel was {pixel:?}"
    );
}

fn hidden_benchmark(url: &str, artifacts: &TestArtifacts, settle_ms: u64) -> std::process::Child {
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

fn serve_fixtures(
    listener: TcpListener,
    request_count: usize,
    response_for: impl Fn(&str) -> &'static str,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set fixture nonblocking: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0..request_count {
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(format!("accept fixture request: {error}")),
            }
        };
        stream
            .set_nonblocking(false)
            .map_err(|error| format!("make fixture connection blocking: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| format!("set fixture read timeout: {error}"))?;
        let mut request_bytes = [0_u8; 4096];
        let bytes_read = stream
            .read(&mut request_bytes)
            .map_err(|error| format!("read fixture request: {error}"))?;
        let request = String::from_utf8_lossy(&request_bytes[..bytes_read]);
        let html = response_for(&request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        stream
            .write_all(response.as_bytes())
            .map_err(|error| format!("write fixture response: {error}"))?;
    }
    Ok(())
}

fn wait_for_child(child: &mut std::process::Child, timeout: Duration) -> ExitStatus {
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

fn json_integer(json: &str, key: &str) -> Option<u64> {
    let prefix = format!("\"{key}\":");
    let value = json
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))?;
    value.trim().trim_end_matches(',').parse::<u64>().ok()
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("test artifact path is valid UTF-8")
}

struct TestArtifacts {
    root: PathBuf,
    json: PathBuf,
    screenshot: PathBuf,
}

impl TestArtifacts {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock precedes Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "breeze-live-runtime-{}-{nonce}",
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
