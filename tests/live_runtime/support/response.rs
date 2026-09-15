use super::*;

pub(super) fn write_fixture_response(
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
    let chunked = response
        .headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("transfer-encoding") && value == "chunked");
    let length_header = if chunked {
        String::new()
    } else {
        format!("Content-Length: {content_length}\r\n")
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n{extra_headers}{length_header}Connection: close\r\n\r\n",
        response.status, response.reason, response.content_type
    );
    if let Err(error) = stream.write_all(headers.as_bytes()) {
        return handle_fixture_write_error(error, response.allow_disconnect);
    }
    let Some(chunks) = &response.stream_chunks else {
        return stream
            .write_all(response.body.as_bytes())
            .map_err(|error| format!("write fixture response: {error}"));
    };
    for (index, chunk) in chunks.iter().enumerate() {
        if index > 0 {
            thread::sleep(response.stream_chunk_delay);
        }
        let wire = if chunked {
            [format!("{:x}\r\n", chunk.len()).as_bytes(), chunk, b"\r\n"].concat()
        } else {
            chunk.clone()
        };
        if let Err(error) = stream.write_all(&wire).and_then(|_| stream.flush()) {
            return handle_fixture_write_error(error, response.allow_disconnect);
        }
    }
    if chunked {
        stream
            .write_all(b"0\r\n\r\n")
            .map_err(|error| error.to_string())?;
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
