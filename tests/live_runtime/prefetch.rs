use super::*;
use std::sync::{Arc, Mutex};

const ENTRY: &str = r#"<!doctype html><title>prefetch entry</title><script>
setTimeout(() => {
  const link = document.createElement('link');
  link.rel = 'prefetch';
  link.href = '/next';
  link.onload = () => {
    console.log('prefetch load event');
    location.href = '/next';
  };
  link.onerror = () => console.log('prefetch error event');
  document.head.appendChild(link);
}, 100);
</script>"#;
const NEXT: &str = r#"<!doctype html><title>prefetched next</title>
<style>html,body{margin:0;background:rgb(17,170,34)}</style>
<p>Prefetched document committed.</p>"#;

#[test]
fn prefetch_fires_load_and_next_navigation_reuses_its_complete_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind prefetch fixture");
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorded = Arc::clone(&requests);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            let line = request.lines().next().unwrap_or_default().to_string();
            recorded.lock().unwrap().push(line.clone());
            if line.starts_with("GET /entry ") {
                FixtureResponse::html(ENTRY)
            } else if line.starts_with("GET /next ") {
                FixtureResponse::html(NEXT).header("Cache-Control", "private, max-age=60")
            } else {
                panic!("unexpected prefetch fixture request: {line}");
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 1800);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "unexpected network requests: {requests:?}"
    );
    assert!(requests[0].starts_with("GET /entry "), "{requests:?}");
    assert!(requests[1].starts_with("GET /next "), "{requests:?}");
    let report = fs::read_to_string(&artifacts.json).expect("read prefetch benchmark report");
    assert!(report.contains("prefetch load event"), "{report}");
    assert!(report.contains("/next"), "{report}");
    assert_green_capture(&artifacts, "prefetched navigation did not render");
}

#[test]
fn completed_http_error_prefetch_fires_load_not_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind prefetch fixture");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            let line = request.lines().next().unwrap_or_default();
            if line.starts_with("GET /entry ") {
                FixtureResponse::html(
                    r#"<!doctype html><script>
setTimeout(() => {
  const link = document.createElement('link');
  link.rel = 'prefetch'; link.href = '/missing';
  link.onload = () => console.log('prefetch 404 load');
  link.onerror = () => console.log('prefetch 404 error');
  document.head.appendChild(link);
}, 100);
</script>"#,
                )
            } else if line.starts_with("GET /missing ") {
                FixtureResponse::html("missing").status(404, "Not Found")
            } else {
                panic!("unexpected prefetch request: {line}");
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/entry"), &artifacts, 1200);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("prefetch 404 load"), "{report}");
    assert!(!report.contains("prefetch 404 error"), "{report}");
}

#[test]
fn network_failure_prefetch_fires_error_not_load() {
    let source = TcpListener::bind("127.0.0.1:0").expect("bind source fixture");
    let source_address = source.local_addr().unwrap();
    let broken = TcpListener::bind("127.0.0.1:0").expect("bind broken fixture");
    let broken_address = broken.local_addr().unwrap();
    let broken_server = thread::spawn(move || {
        // A response-less close is a deterministic transport error, not an HTTP status.
        broken.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            match broken.accept() {
                Ok((stream, _)) => {
                    drop(stream);
                    return;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("no speculative request reached broken fixture: {error}"),
            }
        }
    });
    let server = thread::spawn(move || {
        serve_parallel_fixtures(source, 1, move |_| {
            FixtureResponse::html(format!(
                r#"<!doctype html><script>
setTimeout(() => {{
  const link = document.createElement('link');
  link.rel = 'prefetch'; link.href = 'http://{broken_address}/broken';
  link.onload = () => console.log('prefetch network load');
  link.onerror = () => console.log('prefetch network error');
  document.head.appendChild(link);
}}, 100);
</script>"#
            ))
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{source_address}/entry"), &artifacts, 1200);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    broken_server.join().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("prefetch network error"), "{report}");
    assert!(!report.contains("prefetch network load"), "{report}");
}

#[test]
fn cross_origin_cors_prefetch_has_empty_destination_and_load_event() {
    let source = TcpListener::bind("127.0.0.1:0").expect("bind source fixture");
    let source_address = source.local_addr().unwrap();
    let target = TcpListener::bind("127.0.0.1:0").expect("bind target fixture");
    let target_address = target.local_addr().unwrap();
    let target_request = Arc::new(Mutex::new(String::new()));
    let recorded = target_request.clone();
    let target_server = thread::spawn(move || {
        serve_parallel_fixtures(target, 1, move |request| {
            *recorded.lock().unwrap() = request.to_string();
            FixtureResponse::resource("asset", "text/plain", Duration::ZERO).header(
                "Access-Control-Allow-Origin",
                format!("http://{source_address}"),
            )
        })
    });
    let source_server = thread::spawn(move || {
        serve_parallel_fixtures(source, 1, move |_| {
            FixtureResponse::html(format!(
                r#"<!doctype html><script>
setTimeout(() => {{
  const link = document.createElement('link');
  link.rel = 'prefetch'; link.href = 'http://{target_address}/asset';
  link.crossOrigin = 'anonymous'; link.referrerPolicy = 'no-referrer';
  link.onload = () => console.log('cross-origin prefetch load');
  link.onerror = () => console.log('cross-origin prefetch error');
  document.head.appendChild(link);
}}, 100);
</script>"#
            ))
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{source_address}/entry"), &artifacts, 1300);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    source_server.join().unwrap().unwrap();
    target_server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("cross-origin prefetch load"), "{report}");
    assert!(!report.contains("cross-origin prefetch error"), "{report}");
    let request = target_request.lock().unwrap().to_ascii_lowercase();
    assert!(request.contains("sec-purpose: prefetch"), "{request}");
    assert!(request.contains("sec-fetch-dest: empty"), "{request}");
    assert!(request.contains("sec-fetch-mode: cors"), "{request}");
    assert!(
        request.contains(&format!("origin: http://{source_address}")),
        "{request}"
    );
    assert!(!request.contains("referer:"), "{request}");
}
