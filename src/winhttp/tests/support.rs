use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug)]
pub(super) struct TestRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub(super) struct TestResponse {
    status: u16,
    reason: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    chunk_size: usize,
    chunk_delay: Duration,
}

impl TestResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            reason: reason_phrase(status),
            headers: Vec::new(),
            body: body.into(),
            chunk_size: usize::MAX,
            chunk_delay: Duration::ZERO,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn streamed(mut self, chunk_size: usize, delay: Duration) -> Self {
        self.chunk_size = chunk_size.max(1);
        self.chunk_delay = delay;
        self
    }
}

pub(super) struct LoopbackServer {
    address: String,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl LoopbackServer {
    pub fn start(handler: impl Fn(TestRequest) -> TestResponse + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = Arc::clone(&stopping);
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut connections = Vec::new();
            while !worker_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let handler = Arc::clone(&handler);
                        let stopping = Arc::clone(&worker_stopping);
                        connections.push(thread::spawn(move || {
                            serve(stream, handler.as_ref(), &stopping)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
            for connection in connections {
                let _ = connection.join();
            }
        });
        Self {
            address,
            stopping,
            worker: Some(worker),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    pub fn port(&self) -> u16 {
        self.address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .unwrap()
    }
}

impl Drop for LoopbackServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(stream) = TcpStream::connect(&self.address) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    handler: &(impl Fn(TestRequest) -> TestResponse + ?Sized),
    stopping: &AtomicBool,
) {
    // WinHTTP may establish the TCP connection before its request worker is scheduled. Keep the
    // loopback peer patient enough that unrelated parallel Rust tests cannot create false network
    // failures; dropping the client closes persistent sockets immediately at test teardown.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    loop {
        let Some(request) = read_request(&mut stream, stopping) else {
            return;
        };
        let response = handler(request);
        let mut wire = format!("HTTP/1.1 {} {}\r\n", response.status, response.reason);
        let has_length = response
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
        for (name, value) in &response.headers {
            wire.push_str(&format!("{name}: {value}\r\n"));
        }
        if !has_length {
            wire.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
        }
        wire.push_str("Connection: keep-alive\r\n\r\n");
        if stream.write_all(wire.as_bytes()).is_err() {
            return;
        }
        for chunk in response.body.chunks(response.chunk_size) {
            if stopping.load(Ordering::Acquire) {
                return;
            }
            if stream.write_all(chunk).is_err() {
                return;
            }
            if !response.chunk_delay.is_zero() {
                thread::sleep(response.chunk_delay);
            }
        }
    }
}

fn read_request(stream: &mut TcpStream, stopping: &AtomicBool) -> Option<TestRequest> {
    let mut wire = Vec::new();
    let mut buffer = [0_u8; 2048];
    let header_end = loop {
        let read = loop {
            match stream.read(&mut buffer) {
                Ok(read) => break read,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) && !stopping.load(Ordering::Acquire) => {}
                Err(_) => return None,
            }
        };
        if read == 0 {
            return None;
        }
        wire.extend_from_slice(&buffer[..read]);
        if let Some(end) = wire.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if wire.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8(wire[..header_end].to_vec()).ok()?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let body_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while wire.len() - header_end < body_length {
        let read = loop {
            match stream.read(&mut buffer) {
                Ok(read) => break read,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) && !stopping.load(Ordering::Acquire) => {}
                Err(_) => return None,
            }
        };
        if read == 0 {
            break;
        }
        wire.extend_from_slice(&buffer[..read]);
    }
    Some(TestRequest {
        method,
        path,
        headers,
        body: wire[header_end..]
            .iter()
            .copied()
            .take(body_length)
            .collect(),
    })
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        302 => "Found",
        307 => "Temporary Redirect",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Test Response",
    }
}
