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
