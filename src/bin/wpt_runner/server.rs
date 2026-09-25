use crate::manifest::TestCase;
mod connection;
mod metadata;
use connection::handle_connection;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const REPORTER: &str = include_str!("../../../tests/wpt/reporter.js");

pub(crate) struct TestServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    tests: Vec<TestCase>,
    errors: Arc<Mutex<Vec<String>>>,
    active_connections: Arc<AtomicUsize>,
}

impl TestServer {
    pub(crate) fn start(root: PathBuf, tests: Vec<TestCase>) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| format!("bind local WPT server: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("read local WPT server address: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configure local WPT server: {error}"))?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let active_connections = Arc::new(AtomicUsize::new(0));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_errors = Arc::clone(&errors);
        let worker_tests = Arc::new(tests.clone());
        let worker_root = Arc::new(root);
        let worker_connections = Arc::clone(&active_connections);
        let worker = std::thread::spawn(move || {
            serve(
                listener,
                worker_root,
                worker_tests,
                &worker_shutdown,
                worker_errors,
                worker_connections,
            );
        });
        Ok(Self {
            address,
            shutdown,
            worker: Some(worker),
            tests,
            errors,
            active_connections,
        })
    }

    pub(crate) fn url_for(&self, index: usize) -> String {
        let path = if self.tests[index].needs_wrapper() {
            format!("/__breeze_wpt/{index}.html")
        } else {
            format!("/{}", self.tests[index].path)
        };
        format!("http://{}{path}", self.address)
    }

    pub(crate) fn drain_errors(&self) -> Vec<String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while self.active_connections.load(Ordering::Acquire) != 0
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        if self.active_connections.load(Ordering::Acquire) != 0 {
            return vec!["local WPT server did not become idle".to_string()];
        }
        let Ok(mut errors) = self.errors.lock() else {
            return vec!["local WPT server error log is unavailable".to_string()];
        };
        std::mem::take(&mut *errors)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(
    listener: TcpListener,
    root: Arc<PathBuf>,
    tests: Arc<Vec<TestCase>>,
    shutdown: &AtomicBool,
    errors: Arc<Mutex<Vec<String>>>,
    active_connections: Arc<AtomicUsize>,
) {
    let mut connections = Vec::new();
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                active_connections.fetch_add(1, Ordering::AcqRel);
                let connection_root = Arc::clone(&root);
                let connection_tests = Arc::clone(&tests);
                let connection_errors = Arc::clone(&errors);
                let connection_count = Arc::clone(&active_connections);
                connections.push(std::thread::spawn(move || {
                    let _active_connection = ActiveConnection(connection_count);
                    if let Err(error) =
                        handle_connection(&mut stream, &connection_root, &connection_tests)
                        && let Ok(mut errors) = connection_errors.lock()
                    {
                        errors.push(error);
                    }
                }));
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
    for connection in connections {
        let _ = connection.join();
    }
}

struct ActiveConnection(Arc<AtomicUsize>);

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Response {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

fn route(root: &Path, tests: &[TestCase], request_path: &str) -> Response {
    // Match upstream tools/serve/serve.py's legacy parser URL rewrite. The bytes stay
    // unmodified in the pinned, external WPT checkout.
    let request_path = if request_path == "/resources/WebIDLParser.js" {
        "/resources/webidl2/lib/webidl2.js"
    } else {
        request_path
    };
    if request_path == "/resources/testharnessreport.js" {
        return ok(
            "text/javascript; charset=utf-8",
            REPORTER.as_bytes().to_vec(),
        );
    }
    if let Some(index) = wrapper_index(request_path)
        && let Some(test) = tests.get(index)
        && test.needs_wrapper()
    {
        return match crate::wrapper::load(root, &test.path) {
            Ok(html) => ok("text/html; charset=utf-8", html.into_bytes()),
            Err(message) => error_response(500, &message),
        };
    }
    let relative = match safe_relative_path(request_path) {
        Ok(path) => path,
        Err(message) => return error_response(400, message),
    };
    let path = root.join(relative);
    let canonical = match path.canonicalize() {
        Ok(path) if path.starts_with(root) && path.is_file() => path,
        _ => return error_response(404, "fixture not found"),
    };
    match std::fs::read(&canonical) {
        Ok(body) => match metadata::content_type(root, &canonical) {
            Ok(Some(content_type)) => Response {
                status: 200,
                content_type,
                body,
            },
            Ok(None) => ok(content_type(&canonical), body),
            Err(message) => error_response(500, &message),
        },
        Err(_) => error_response(500, "fixture could not be read"),
    }
}

fn wrapper_index(path: &str) -> Option<usize> {
    path.strip_prefix("/__breeze_wpt/")?
        .strip_suffix(".html")?
        .parse()
        .ok()
}

fn safe_relative_path(request_path: &str) -> Result<PathBuf, &'static str> {
    let decoded = percent_decode(request_path).ok_or("invalid URL encoding")?;
    if decoded.contains('\\') || decoded.contains('\0') {
        return Err("unsafe fixture path");
    }
    let trimmed = decoded.trim_start_matches('/');
    let path = Path::new(trimmed);
    if trimmed.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("unsafe fixture path");
    }
    Ok(path.to_path_buf())
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn ok(content_type: &'static str, body: Vec<u8>) -> Response {
    Response {
        status: 200,
        content_type: content_type.into(),
        body,
    }
}

fn error_response(status: u16, message: &str) -> Response {
    Response {
        status,
        content_type: "text/plain; charset=utf-8".into(),
        body: message.as_bytes().to_vec(),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        // Do not override fixture BOM/meta encoding with an injected HTTP charset.
        Some("html" | "htm") => "text/html",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests;
