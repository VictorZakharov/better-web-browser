use super::{TestCase, route};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 64 * 1024;

pub(super) fn handle_connection(
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
    // Browsers may cancel speculative or in-flight resource requests. A peer
    // disconnect is not a fixture-server failure or a WPT assertion failure.
    let Some(request) = read_request(stream)? else {
        return Ok(());
    };
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
        &response.content_type,
        &response.body,
        method == "HEAD",
    )
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

fn read_request(stream: &mut impl Read) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    while bytes.len() < MAX_REQUEST_BYTES {
        let count = match stream.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if is_client_disconnect(&error) => return Ok(None),
            Err(error) => return Err(format!("read WPT request: {error}")),
        };
        if count == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if bytes.len() >= MAX_REQUEST_BYTES {
        return Err("WPT request headers are too large".to_string());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "WPT request is not UTF-8".to_string())
}

fn is_client_disconnect(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NotConnected
    )
}

fn write_response(
    stream: &mut impl Write,
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
        .or_else(|error| {
            if is_client_disconnect(&error) {
                Ok(())
            } else {
                Err(format!("write WPT response: {error}"))
            }
        })
}

#[cfg(test)]
mod tests;
