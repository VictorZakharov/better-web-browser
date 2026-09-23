use super::*;

const EVENT_SOURCE_HTML: &str = r#"<!doctype html>
<title>event source pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }
       #state { width: 100%; height: 600px; }</style>
<div id="state">waiting</div>
<script>
  const source = new EventSource('/events');
  const events = [];
  source.onopen = event => {
    if (!event.isTrusted || source.readyState !== EventSource.OPEN)
      throw new Error('EventSource open state was not observable');
  };
  source.addEventListener('update', event => {
    events.push(event);
    if (events.length !== 2) return;
    if (events[0].data !== 'caf\u00e9' || events[1].data !== 'done' ||
        events[0].lastEventId !== 'first' || events[1].lastEventId !== 'second' ||
        events.some(item => !item.isTrusted))
      throw new Error('EventSource stream framing or UTF-8 failed');
    source.close();
    if (source.readyState !== EventSource.CLOSED)
      throw new Error('EventSource close did not update readyState');
    document.getElementById('state').textContent = 'stream complete';
    document.body.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'event source complete';
  });
  source.onerror = () => {
    if (source.readyState === EventSource.CLOSED) return;
    throw new Error('EventSource lost a live stream');
  };
</script>"#;

#[test]
fn event_source_streams_real_http_chunks_without_waiting_for_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind SSE fixture");
    let address = listener.local_addr().expect("read SSE fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /events ") {
                FixtureResponse::streamed(
                    "text/event-stream; charset=utf-8",
                    vec![
                        b": keepalive\r\nid: first\r\nevent: update\r\ndata: caf\xc3".to_vec(),
                        b"\xa9\r\n\r\nid: second\nevent: update\ndata: done\n\n".to_vec(),
                        b": connection deliberately remains open\n\n".to_vec(),
                    ],
                    Duration::from_millis(100),
                )
                .allow_disconnect()
            } else {
                FixtureResponse::html(EVENT_SOURCE_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/event-source");
    let mut child = hidden_benchmark(&url, &artifacts, 1200);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("SSE fixture server panicked")
        .expect("SSE fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).expect("read SSE benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "SSE integration reported JavaScript errors:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "SSE chunks did not update the retained document",
    );
}
