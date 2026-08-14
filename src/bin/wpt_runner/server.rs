use crate::manifest::TestCase;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const REPORTER: &str = include_str!("../../../tests/wpt/reporter.js");
const MAX_REQUEST_BYTES: usize = 64 * 1024;

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

fn handle_connection(
    stream: &mut TcpStream,
    root: &Path,
    tests: &[TestCase],
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("make WPT connection blocking: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("set WPT request timeout: {error}"))?;
    let request = read_request(stream)?;
    let (method, target) = parse_request_line(&request)?;
    if method != "GET" && method != "HEAD" {
        return write_response(
            stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
            false,
        );
    }
    let path = target.split(['?', '#']).next().unwrap_or(target);
    let response = route(root, tests, path);
    write_response(
        stream,
        response.status,
        response.content_type,
        &response.body,
        method == "HEAD",
    )
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn route(root: &Path, tests: &[TestCase], request_path: &str) -> Response {
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
        return ok(
            "text/html; charset=utf-8",
            wrapper_html(&test.path).into_bytes(),
        );
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
        Ok(body) => ok(content_type(&canonical), body),
        Err(_) => error_response(500, "fixture could not be read"),
    }
}

fn wrapper_index(path: &str) -> Option<usize> {
    path.strip_prefix("/__breeze_wpt/")?
        .strip_suffix(".html")?
        .parse()
        .ok()
}

fn wrapper_html(test_path: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>{test_path}</title>\
         <script src=/resources/testharness.js></script>\
         <script src=/resources/testharnessreport.js></script>\
         <script src=/{test_path}></script><div id=log></div>"
    )
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

fn parse_request_line(request: &str) -> Result<(&str, &str), String> {
    let line = request
        .lines()
        .next()
        .ok_or_else(|| "empty HTTP request".to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "missing HTTP method".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| "missing HTTP target".to_string())?;
    if !target.starts_with('/') {
        return Err("HTTP target must be origin-form".to_string());
    }
    Ok((method, target))
}

fn read_request(stream: &mut TcpStream) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    while bytes.len() < MAX_REQUEST_BYTES {
        let count = stream
            .read(&mut chunk)
            .map_err(|error| format!("read WPT request: {error}"))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if bytes.len() >= MAX_REQUEST_BYTES {
        return Err("WPT request headers are too large".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "WPT request is not UTF-8".to_string())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| {
            if head_only {
                Ok(())
            } else {
                stream.write_all(body)
            }
        })
        .map_err(|error| format!("write WPT response: {error}"))
}

fn ok(content_type: &'static str, body: Vec<u8>) -> Response {
    Response {
        status: 200,
        content_type,
        body,
    }
}

fn error_response(status: u16, message: &str) -> Response {
    Response {
        status,
        content_type: "text/plain; charset=utf-8",
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
mod tests {
    use super::*;

    #[test]
    fn rejects_plain_and_encoded_traversal() {
        assert!(safe_relative_path("/../secret").is_err());
        assert!(safe_relative_path("/%2e%2e/secret").is_err());
        assert!(safe_relative_path("/..%5csecret").is_err());
    }

    #[test]
    fn accepts_an_upstream_resource_path() {
        assert_eq!(
            safe_relative_path("/resources/testharness.js").unwrap(),
            PathBuf::from("resources/testharness.js")
        );
    }

    #[test]
    fn recognizes_only_numeric_wrapper_routes() {
        assert_eq!(wrapper_index("/__breeze_wpt/12.html"), Some(12));
        assert_eq!(wrapper_index("/__breeze_wpt/test.html"), None);
    }
}
