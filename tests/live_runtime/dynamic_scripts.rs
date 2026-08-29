use super::support::{
    FixtureResponse, TestArtifacts, assert_green_capture, hidden_benchmark,
    serve_parallel_fixtures, wait_for_child,
};
use std::fs;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DYNAMIC_SCRIPT_HTML: &str = r#"<!doctype html>
<title>dynamic scripts pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script>
  window.dynamicScriptsLoaded = 0;
  window.markDynamicScriptLoaded = () => {
    window.dynamicScriptsLoaded++;
    if (window.dynamicScriptsLoaded === 3) {
      document.body.style.backgroundColor = 'rgb(17, 170, 34)';
      document.title = 'dynamic scripts complete';
    }
  };
  for (const source of ['/dynamic-one.js', '/dynamic-two.js', '/dynamic-three.js']) {
    const script = document.createElement('script');
    script.src = source;
    document.head.appendChild(script);
  }
</script>"#;

const FETCH_CALLBACK_DYNAMIC_SCRIPT_HTML: &str = r#"<!doctype html>
<title>Fetch callback dynamic scripts pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script>
  window.dynamicScriptsLoaded = 0;
  window.markDynamicScriptLoaded = () => {
    window.dynamicScriptsLoaded++;
    if (window.dynamicScriptsLoaded === 3) {
      document.body.style.backgroundColor = 'rgb(17, 170, 34)';
      document.title = 'Fetch callback dynamic scripts complete';
    }
  };
  fetch('/trigger').then(() => {
    for (const source of ['/dynamic-one.js', '/dynamic-two.js', '/dynamic-three.js']) {
      const script = document.createElement('script');
      script.src = source;
      document.head.appendChild(script);
    }
  });
</script>"#;

#[test]
fn dynamically_inserted_scripts_fetch_concurrently() {
    const FETCH_DELAY: Duration = Duration::from_millis(600);
    const MAX_START_SPREAD: Duration = Duration::from_millis(400);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let request_times = Arc::new(Mutex::new(Vec::new()));
    let server_times = Arc::clone(&request_times);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 4, move |request| {
            if request.contains("GET /dynamic-") && request.contains(".js ") {
                server_times
                    .lock()
                    .expect("lock dynamic request times")
                    .push(Instant::now());
                FixtureResponse::script("window.markDynamicScriptLoaded();", FETCH_DELAY)
            } else {
                FixtureResponse::html(DYNAMIC_SCRIPT_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/dynamic-script-page");

    let mut child = hidden_benchmark(&url, &artifacts, 2200);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        status.success(),
        "hidden Breeze run failed: {status}\n{report}"
    );
    let server_result = server.join().expect("fixture server panicked");
    assert!(
        server_result.is_ok(),
        "fixture server failed: {server_result:?}\n{report}"
    );

    let times = request_times.lock().expect("lock dynamic request times");
    assert_eq!(times.len(), 3, "not all dynamic scripts were requested");
    let spread = times
        .iter()
        .max()
        .unwrap()
        .duration_since(*times.iter().min().unwrap());
    assert!(
        spread < MAX_START_SPREAD,
        "dynamic script requests started {spread:?} apart instead of concurrently"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "dynamic scripts reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "dynamic scripts did not all execute");
}

#[test]
fn fetch_callback_dynamic_scripts_return_to_the_concurrent_loader() {
    const FETCH_DELAY: Duration = Duration::from_millis(600);
    const MAX_START_SPREAD: Duration = Duration::from_millis(400);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let request_times = Arc::new(Mutex::new(Vec::new()));
    let server_times = Arc::clone(&request_times);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 5, move |request| {
            if request.contains("GET /dynamic-") && request.contains(".js ") {
                server_times
                    .lock()
                    .expect("lock dynamic request times")
                    .push(Instant::now());
                FixtureResponse::script("window.markDynamicScriptLoaded();", FETCH_DELAY)
            } else if request.contains("GET /trigger ") {
                FixtureResponse::json("{}")
            } else {
                FixtureResponse::html(FETCH_CALLBACK_DYNAMIC_SCRIPT_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/fetch-callback-dynamic-script-page");

    let mut child = hidden_benchmark(&url, &artifacts, 2200);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        status.success(),
        "hidden Breeze run failed: {status}\n{report}"
    );
    let server_result = server.join().expect("fixture server panicked");
    assert!(
        server_result.is_ok(),
        "fixture server failed: {server_result:?}\n{report}"
    );

    let times = request_times.lock().expect("lock dynamic request times");
    assert_eq!(times.len(), 3, "not all dynamic scripts were requested");
    let spread = times
        .iter()
        .max()
        .unwrap()
        .duration_since(*times.iter().min().unwrap());
    assert!(
        spread < MAX_START_SPREAD,
        "Fetch callback script requests started {spread:?} apart instead of concurrently"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "Fetch callback dynamic scripts reported errors:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "Fetch callback dynamic scripts did not all execute",
    );
}
